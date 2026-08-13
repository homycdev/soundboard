use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use kira::Decibels;
use kira::backend::cpal::CpalBackendSettings;
use kira::sound::static_sound::StaticSoundData;
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, PlaySoundError};
use uuid::Uuid;

use super::routing::{MicrophonePassthrough, enumerate_devices, find_device};
use crate::domain::AudioRoutingSettings;
use crate::dto::{PlaybackFailed, PlaybackStarted};
use crate::error::ApiError;
use crate::ports::{
    AudioMetadata, AudioRoutingRuntime, AudioService, PlaybackEventSink, PlaybackRequest,
};

const AUDIO_QUEUE_CAPACITY: usize = 256;
const VOICE_CAPACITY: usize = 256;

enum Message {
    Probe {
        path: PathBuf,
        reply: mpsc::Sender<Result<AudioMetadata, ApiError>>,
    },
    Load {
        sound_id: String,
        path: PathBuf,
        reply: mpsc::Sender<Result<AudioMetadata, ApiError>>,
    },
    Unload {
        sound_id: String,
    },
    Play {
        request: PlaybackRequest,
        reply: Option<mpsc::Sender<Result<String, ApiError>>>,
    },
    RoutingRuntime {
        reply: mpsc::Sender<Result<AudioRoutingRuntime, ApiError>>,
    },
    ConfigureRouting {
        settings: AudioRoutingSettings,
        reply: mpsc::Sender<Result<(), ApiError>>,
    },
    DisableRouting {
        reply: mpsc::Sender<Result<(), ApiError>>,
    },
}

struct RunningRouting {
    manager: AudioManager<DefaultBackend>,
    microphone: MicrophonePassthrough,
    settings: AudioRoutingSettings,
}

impl RunningRouting {
    fn start(settings: AudioRoutingSettings) -> Result<Self, ApiError> {
        settings.validate()?;
        let input_id = settings
            .input_device_id
            .as_deref()
            .expect("enabled routing has an input device");
        let output_id = settings
            .virtual_output_device_id
            .as_deref()
            .expect("enabled routing has an output device");
        let input_device = find_device(input_id, true)?;
        let output_device = find_device(output_id, false)?;
        let manager_settings = AudioManagerSettings {
            main_track_builder: MainTrackBuilder::default().sound_capacity(VOICE_CAPACITY),
            backend_settings: CpalBackendSettings {
                device: Some(output_device.clone()),
                config: None,
            },
            ..AudioManagerSettings::default()
        };
        let manager = AudioManager::<DefaultBackend>::new(manager_settings).map_err(|error| {
            log::warn!("virtual soundboard output could not initialize: {error}");
            ApiError::new(
                "AUDIO_ROUTING_FAILED",
                "The virtual output could not be opened. Confirm the virtual audio driver is installed and not in exclusive use.",
            )
        })?;
        let microphone = MicrophonePassthrough::start(
            &input_device,
            &output_device,
            settings.microphone_gain_percent,
        )?;
        Ok(Self {
            manager,
            microphone,
            settings,
        })
    }
}

pub struct KiraAudioService {
    sender: SyncSender<Message>,
    available: Arc<AtomicBool>,
    events: Arc<dyn PlaybackEventSink>,
}

impl KiraAudioService {
    pub fn start(events: Arc<dyn PlaybackEventSink>) -> Arc<Self> {
        let (sender, receiver) = mpsc::sync_channel(AUDIO_QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::channel();
        let available = Arc::new(AtomicBool::new(false));
        let worker_available = Arc::clone(&available);
        let worker_events = Arc::clone(&events);
        thread::Builder::new()
            .name("soundboard-audio".to_owned())
            .spawn(move || run_worker(receiver, worker_available, worker_events, ready_sender))
            .expect("failed to start audio worker thread");
        let _ = ready_receiver.recv();
        Arc::new(Self {
            sender,
            available,
            events,
        })
    }

    fn request<T>(
        &self,
        build: impl FnOnce(mpsc::Sender<Result<T, ApiError>>) -> Message,
    ) -> Result<T, ApiError> {
        let (reply, receiver) = mpsc::channel();
        self.sender
            .send(build(reply))
            .map_err(|_| ApiError::internal())?;
        receiver.recv().map_err(|_| ApiError::internal())?
    }

    fn queue_failure(&self, request: &PlaybackRequest, error: &ApiError) {
        self.events.failed(PlaybackFailed {
            sound_id: Some(request.sound_id.clone()),
            cell_id: Some(request.cell_id.clone()),
            code: error.code.clone(),
            message: error.message.clone(),
        });
    }
}

impl AudioService for KiraAudioService {
    fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }

    fn probe(&self, path: &Path) -> Result<AudioMetadata, ApiError> {
        self.request(|reply| Message::Probe {
            path: path.to_owned(),
            reply,
        })
    }

    fn load(&self, sound_id: &str, path: &Path) -> Result<AudioMetadata, ApiError> {
        self.request(|reply| Message::Load {
            sound_id: sound_id.to_owned(),
            path: path.to_owned(),
            reply,
        })
    }

    fn unload(&self, sound_id: &str) {
        let _ = self.sender.send(Message::Unload {
            sound_id: sound_id.to_owned(),
        });
    }

    fn play(&self, request: PlaybackRequest) -> Result<String, ApiError> {
        self.request(|reply| Message::Play {
            request,
            reply: Some(reply),
        })
    }

    fn try_play(&self, request: PlaybackRequest) {
        if let Err(error) = self.sender.try_send(Message::Play {
            request: request.clone(),
            reply: None,
        }) {
            let api_error = match error {
                TrySendError::Full(_) => ApiError::new(
                    "PLAYBACK_LIMIT_REACHED",
                    "Too many sounds are already starting. Try again in a moment.",
                ),
                TrySendError::Disconnected(_) => ApiError::internal(),
            };
            self.queue_failure(&request, &api_error);
        }
    }

    fn routing_runtime(&self) -> Result<AudioRoutingRuntime, ApiError> {
        self.request(|reply| Message::RoutingRuntime { reply })
    }

    fn configure_routing(&self, settings: &AudioRoutingSettings) -> Result<(), ApiError> {
        self.request(|reply| Message::ConfigureRouting {
            settings: settings.clone(),
            reply,
        })
    }

    fn disable_routing(&self) -> Result<(), ApiError> {
        self.request(|reply| Message::DisableRouting { reply })
    }
}

fn run_worker(
    receiver: Receiver<Message>,
    available: Arc<AtomicBool>,
    events: Arc<dyn PlaybackEventSink>,
    ready: mpsc::Sender<()>,
) {
    let settings = AudioManagerSettings {
        main_track_builder: MainTrackBuilder::default().sound_capacity(VOICE_CAPACITY),
        ..AudioManagerSettings::default()
    };
    let mut manager = match AudioManager::<DefaultBackend>::new(settings) {
        Ok(manager) => {
            available.store(true, Ordering::Release);
            Some(manager)
        }
        Err(error) => {
            log::warn!("default audio device could not be initialized: {error}");
            None
        }
    };
    let _ = ready.send(());
    let mut sounds: HashMap<String, StaticSoundData> = HashMap::new();
    let mut routing: Option<RunningRouting> = None;
    let mut last_routing_error: Option<ApiError> = None;

    while let Ok(message) = receiver.recv() {
        match message {
            Message::Probe { path, reply } => {
                let result = decode(&path).map(|sound| metadata(&sound));
                let _ = reply.send(result);
            }
            Message::Load {
                sound_id,
                path,
                reply,
            } => {
                let result = decode(&path).map(|sound| {
                    let metadata = metadata(&sound);
                    sounds.insert(sound_id, sound);
                    metadata
                });
                let _ = reply.send(result);
            }
            Message::Unload { sound_id } => {
                sounds.remove(&sound_id);
            }
            Message::Play { request, reply } => {
                let result = play_one(manager.as_mut(), routing.as_mut(), &sounds, &request);
                match &result {
                    Ok(instance_id) => events.started(playback_started_event(
                        &request,
                        instance_id.clone(),
                        unix_time_ms(),
                    )),
                    Err(error) if reply.is_none() => events.failed(PlaybackFailed {
                        sound_id: Some(request.sound_id.clone()),
                        cell_id: Some(request.cell_id.clone()),
                        code: error.code.clone(),
                        message: error.message.clone(),
                    }),
                    Err(_) => {}
                }
                if let Some(reply) = reply {
                    let _ = reply.send(result);
                }
            }
            Message::RoutingRuntime { reply } => {
                let result = enumerate_devices().map(|(input_devices, output_devices)| {
                    let stream_error = routing
                        .as_ref()
                        .and_then(|running| running.microphone.error())
                        .map(|message| {
                            ApiError::new(
                                "AUDIO_ROUTING_INTERRUPTED",
                                format!("Audio routing stopped unexpectedly: {message}"),
                            )
                        });
                    AudioRoutingRuntime {
                        active: routing.is_some() && stream_error.is_none(),
                        input_devices,
                        output_devices,
                        error: stream_error.or_else(|| last_routing_error.clone()),
                    }
                });
                let _ = reply.send(result);
            }
            Message::ConfigureRouting { settings, reply } => {
                let previous_settings = routing.as_ref().map(|running| running.settings.clone());
                routing = None;
                match RunningRouting::start(settings) {
                    Ok(next) => {
                        routing = Some(next);
                        last_routing_error = None;
                        available.store(true, Ordering::Release);
                        let _ = reply.send(Ok(()));
                    }
                    Err(error) => {
                        if let Some(previous_settings) = previous_settings {
                            routing = RunningRouting::start(previous_settings).ok();
                        }
                        last_routing_error = routing.is_none().then(|| error.clone());
                        available.store(manager.is_some() || routing.is_some(), Ordering::Release);
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Message::DisableRouting { reply } => {
                routing = None;
                last_routing_error = None;
                available.store(manager.is_some(), Ordering::Release);
                let _ = reply.send(Ok(()));
            }
        }
    }
}

fn playback_started_event(
    request: &PlaybackRequest,
    instance_id: String,
    started_at_ms: u64,
) -> PlaybackStarted {
    PlaybackStarted {
        instance_id,
        sound_id: request.sound_id.clone(),
        cell_id: request.cell_id.clone(),
        trigger: request.trigger,
        started_at_ms,
    }
}

fn decode(path: &Path) -> Result<StaticSoundData, ApiError> {
    StaticSoundData::from_file(path).map_err(|error| {
        log::debug!("audio decode failed: {error}");
        ApiError::decode()
    })
}

fn metadata(sound: &StaticSoundData) -> AudioMetadata {
    AudioMetadata {
        duration_ms: sound.duration().as_millis().min(u128::from(u64::MAX)) as u64,
    }
}

fn play_one(
    manager: Option<&mut AudioManager<DefaultBackend>>,
    routing: Option<&mut RunningRouting>,
    sounds: &HashMap<String, StaticSoundData>,
    request: &PlaybackRequest,
) -> Result<String, ApiError> {
    let Some(sound) = sounds.get(&request.sound_id) else {
        return Err(ApiError::new(
            "AUDIO_DECODE_FAILED",
            "This sound is not available for playback.",
        ));
    };

    if let Some(routing) = routing {
        let volume = gain_decibels(routing.settings.soundboard_gain_percent);
        routing
            .manager
            .play(sound.volume(volume))
            .map_err(play_error)?;
        if routing.settings.monitor_enabled
            && let Some(manager) = manager
            && let Err(error) = manager.play(sound.clone())
        {
            log::warn!(
                "local monitoring could not start: {}",
                play_error(error).message
            );
        }
    } else {
        let Some(manager) = manager else {
            return Err(ApiError::audio_device());
        };
        manager.play(sound.clone()).map_err(play_error)?;
    }
    Ok(Uuid::new_v4().to_string())
}

fn gain_decibels(percent: u16) -> Decibels {
    if percent == 0 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * (f32::from(percent) / 100.0).log10())
    }
}

fn play_error<E>(error: PlaySoundError<E>) -> ApiError {
    match error {
        PlaySoundError::SoundLimitReached => ApiError::new(
            "PLAYBACK_LIMIT_REACHED",
            "The simultaneous playback limit has been reached.",
        ),
        PlaySoundError::IntoSoundError(_) => ApiError::internal(),
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::Trigger;

    #[test]
    fn playback_started_event_preserves_trigger_and_ids() {
        let request = PlaybackRequest {
            sound_id: "sound-123".to_owned(),
            cell_id: "r2c3".to_owned(),
            trigger: Trigger::GlobalShortcut,
        };

        let event = playback_started_event(&request, "instance-456".to_owned(), 789);

        assert_eq!(event.instance_id, "instance-456");
        assert_eq!(event.sound_id, "sound-123");
        assert_eq!(event.cell_id, "r2c3");
        assert_eq!(event.trigger, Trigger::GlobalShortcut);
        assert_eq!(event.started_at_ms, 789);
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            serde_json::json!({
                "instanceId": "instance-456",
                "soundId": "sound-123",
                "cellId": "r2c3",
                "trigger": "globalShortcut",
                "startedAtMs": 789,
            })
        );
    }
}
