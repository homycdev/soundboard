use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use kira::sound::static_sound::StaticSoundData;
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, DefaultBackend, PlaySoundError};
use uuid::Uuid;

use crate::dto::{PlaybackFailed, PlaybackStarted};
use crate::error::ApiError;
use crate::ports::{AudioMetadata, AudioService, PlaybackEventSink, PlaybackRequest};

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
                let result = play_one(manager.as_mut(), &sounds, &request);
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
    sounds: &HashMap<String, StaticSoundData>,
    request: &PlaybackRequest,
) -> Result<String, ApiError> {
    let Some(manager) = manager else {
        return Err(ApiError::audio_device());
    };
    let Some(sound) = sounds.get(&request.sound_id) else {
        return Err(ApiError::new(
            "AUDIO_DECODE_FAILED",
            "This sound is not available for playback.",
        ));
    };
    manager.play(sound.clone()).map_err(|error| match error {
        PlaySoundError::SoundLimitReached => ApiError::new(
            "PLAYBACK_LIMIT_REACHED",
            "The simultaneous playback limit has been reached.",
        ),
        PlaySoundError::IntoSoundError(_) => ApiError::internal(),
    })?;
    Ok(Uuid::new_v4().to_string())
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
