use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, DeviceId, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::error::ApiError;
use crate::ports::AudioDeviceInfo;

const BUFFER_MILLISECONDS: usize = 500;
const STARTING_LATENCY_MILLISECONDS: usize = 60;
const MAX_DRIFT_CORRECTION: f64 = 0.005;

pub struct MicrophonePassthrough {
    _input_stream: Stream,
    _output_stream: Stream,
    health: Arc<Mutex<Option<String>>>,
}

impl MicrophonePassthrough {
    pub fn start(
        input_device: &Device,
        output_device: &Device,
        microphone_gain_percent: u16,
    ) -> Result<Self, ApiError> {
        let input_supported = input_device.default_input_config().map_err(|error| {
            routing_error(format!("The selected microphone cannot be opened: {error}"))
        })?;
        let output_supported = output_device.default_output_config().map_err(|error| {
            routing_error(format!(
                "The selected virtual output cannot be opened: {error}"
            ))
        })?;
        let input_config = input_supported.config();
        let output_config = output_supported.config();
        let input_rate = input_config.sample_rate;
        let output_rate = output_config.sample_rate;
        let capacity = (input_rate as usize * BUFFER_MILLISECONDS / 1_000).max(1_024);
        let target_fill = (input_rate as usize * STARTING_LATENCY_MILLISECONDS / 1_000).max(128);
        let (mut producer, consumer) = RingBuffer::<f32>::new(capacity);
        for _ in 0..target_fill.min(capacity) {
            let _ = producer.push(0.0);
        }

        let health = Arc::new(Mutex::new(None));
        let input_stream = build_input_stream(
            input_device,
            &input_config,
            input_supported.sample_format(),
            producer,
            f32::from(microphone_gain_percent) / 100.0,
            Arc::clone(&health),
        )?;
        let output_stream = build_output_stream(
            output_device,
            &output_config,
            output_supported.sample_format(),
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            Arc::clone(&health),
        )?;

        input_stream.play().map_err(|error| {
            routing_error(format!("The selected microphone could not start: {error}"))
        })?;
        output_stream.play().map_err(|error| {
            routing_error(format!(
                "The selected virtual output could not start: {error}"
            ))
        })?;

        Ok(Self {
            _input_stream: input_stream,
            _output_stream: output_stream,
            health,
        })
    }

    pub fn error(&self) -> Option<String> {
        lock(&self.health).clone()
    }
}

pub fn enumerate_devices() -> Result<(Vec<AudioDeviceInfo>, Vec<AudioDeviceInfo>), ApiError> {
    let host = cpal::default_host();
    let default_input = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let default_output = host
        .default_output_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let inputs = host
        .input_devices()
        .map_err(|error| enumeration_error(&error.to_string()))?
        .filter_map(|device| device_info(device, default_input.as_deref()))
        .collect::<Vec<_>>();
    let outputs = host
        .output_devices()
        .map_err(|error| enumeration_error(&error.to_string()))?
        .filter_map(|device| device_info(device, default_output.as_deref()))
        .collect::<Vec<_>>();
    Ok((sorted_devices(inputs), sorted_devices(outputs)))
}

pub fn find_device(id: &str, input: bool) -> Result<Device, ApiError> {
    let device_id = DeviceId::from_str(id).map_err(|_| {
        if input {
            missing_input()
        } else {
            missing_output()
        }
    })?;
    let host = cpal::default_host();
    let device = host.device_by_id(&device_id).ok_or_else(|| {
        if input {
            missing_input()
        } else {
            missing_output()
        }
    })?;
    let supported = if input {
        device.supports_input()
    } else {
        device.supports_output()
    };
    if !supported {
        return Err(if input {
            missing_input()
        } else {
            missing_output()
        });
    }
    Ok(device)
}

pub fn is_virtual_device_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "blackhole",
        "vb-audio",
        "vb-cable",
        "cable input",
        "voicemeeter",
        "virtual audio",
        "loopback",
    ]
    .iter()
    .any(|candidate| normalized.contains(candidate))
}

fn device_info(device: Device, default_id: Option<&str>) -> Option<AudioDeviceInfo> {
    let id = device.id().ok()?.to_string();
    let name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| device.to_string());
    Some(AudioDeviceInfo {
        is_default: default_id == Some(id.as_str()),
        is_virtual: is_virtual_device_name(&name),
        id,
        name,
    })
}

fn sorted_devices(mut devices: Vec<AudioDeviceInfo>) -> Vec<AudioDeviceInfo> {
    devices.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
    devices.dedup_by(|left, right| left.id == right.id);
    devices
}

fn build_input_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    producer: Producer<f32>,
    gain: f32,
    health: Arc<Mutex<Option<String>>>,
) -> Result<Stream, ApiError> {
    match format {
        SampleFormat::I8 => build_input::<i8>(device, config, producer, gain, health),
        SampleFormat::I16 => build_input::<i16>(device, config, producer, gain, health),
        SampleFormat::I24 => build_input::<cpal::I24>(device, config, producer, gain, health),
        SampleFormat::I32 => build_input::<i32>(device, config, producer, gain, health),
        SampleFormat::I64 => build_input::<i64>(device, config, producer, gain, health),
        SampleFormat::U8 => build_input::<u8>(device, config, producer, gain, health),
        SampleFormat::U16 => build_input::<u16>(device, config, producer, gain, health),
        SampleFormat::U24 => build_input::<cpal::U24>(device, config, producer, gain, health),
        SampleFormat::U32 => build_input::<u32>(device, config, producer, gain, health),
        SampleFormat::U64 => build_input::<u64>(device, config, producer, gain, health),
        SampleFormat::F32 => build_input::<f32>(device, config, producer, gain, health),
        SampleFormat::F64 => build_input::<f64>(device, config, producer, gain, health),
        _ => Err(routing_error(
            "The selected microphone uses an unsupported sample format.".to_owned(),
        )),
    }
}

fn build_input<T>(
    device: &Device,
    config: &StreamConfig,
    mut producer: Producer<f32>,
    gain: f32,
    health: Arc<Mutex<Option<String>>>,
) -> Result<Stream, ApiError>
where
    T: SizedSample + Sample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels.max(1));
    let error_health = Arc::clone(&health);
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                for frame in data.chunks(channels) {
                    let mono = frame.iter().copied().map(f32::from_sample).sum::<f32>()
                        / frame.len() as f32;
                    let sample = (mono * gain).clamp(-1.0, 1.0);
                    let _ = producer.push(sample);
                }
            },
            move |error| {
                *lock(&error_health) = Some(format!("Microphone stream stopped: {error}"));
            },
            None,
        )
        .map_err(|error| {
            *lock(&health) = Some(error.to_string());
            routing_error(format!(
                "The selected microphone could not be opened: {error}"
            ))
        })
}

#[allow(clippy::too_many_arguments)]
fn build_output_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    consumer: Consumer<f32>,
    input_rate: u32,
    output_rate: u32,
    capacity: usize,
    target_fill: usize,
    health: Arc<Mutex<Option<String>>>,
) -> Result<Stream, ApiError> {
    match format {
        SampleFormat::I8 => build_output::<i8>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::I16 => build_output::<i16>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::I24 => build_output::<cpal::I24>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::I32 => build_output::<i32>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::I64 => build_output::<i64>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::U8 => build_output::<u8>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::U16 => build_output::<u16>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::U24 => build_output::<cpal::U24>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::U32 => build_output::<u32>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::U64 => build_output::<u64>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::F32 => build_output::<f32>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        SampleFormat::F64 => build_output::<f64>(
            device,
            config,
            consumer,
            input_rate,
            output_rate,
            capacity,
            target_fill,
            health,
        ),
        _ => Err(routing_error(
            "The selected virtual output uses an unsupported sample format.".to_owned(),
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_output<T>(
    device: &Device,
    config: &StreamConfig,
    consumer: Consumer<f32>,
    input_rate: u32,
    output_rate: u32,
    capacity: usize,
    target_fill: usize,
    health: Arc<Mutex<Option<String>>>,
) -> Result<Stream, ApiError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels.max(1));
    let error_health = Arc::clone(&health);
    let mut resampler =
        VoiceResampler::new(consumer, input_rate, output_rate, capacity, target_fill);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let sample = T::from_sample(resampler.next_sample());
                    frame.fill(sample);
                }
            },
            move |error| {
                *lock(&error_health) = Some(format!("Virtual output stream stopped: {error}"));
            },
            None,
        )
        .map_err(|error| {
            *lock(&health) = Some(error.to_string());
            routing_error(format!(
                "The selected virtual output could not be opened: {error}"
            ))
        })
}

struct VoiceResampler {
    consumer: Consumer<f32>,
    base_step: f64,
    phase: f64,
    current: f32,
    next: f32,
    capacity: usize,
    target_fill: usize,
}

impl VoiceResampler {
    fn new(
        mut consumer: Consumer<f32>,
        input_rate: u32,
        output_rate: u32,
        capacity: usize,
        target_fill: usize,
    ) -> Self {
        let current = consumer.pop().unwrap_or(0.0);
        let next = consumer.pop().unwrap_or(current);
        Self {
            consumer,
            base_step: f64::from(input_rate) / f64::from(output_rate.max(1)),
            phase: 0.0,
            current,
            next,
            capacity,
            target_fill,
        }
    }

    fn next_sample(&mut self) -> f32 {
        let result = self.current + (self.next - self.current) * self.phase as f32;
        let fill = self.consumer.slots();
        let fill_error = (fill as f64 - self.target_fill as f64) / self.capacity.max(1) as f64;
        let correction = fill_error.clamp(-1.0, 1.0) * MAX_DRIFT_CORRECTION;
        self.phase += self.base_step * (1.0 + correction);
        while self.phase >= 1.0 {
            self.current = self.next;
            self.next = self.consumer.pop().unwrap_or(0.0);
            self.phase -= 1.0;
        }
        result.clamp(-1.0, 1.0)
    }
}

fn missing_input() -> ApiError {
    ApiError::new(
        "AUDIO_INPUT_NOT_FOUND",
        "The selected microphone is no longer available. Refresh devices and choose another one.",
    )
}

fn missing_output() -> ApiError {
    ApiError::new(
        "VIRTUAL_OUTPUT_NOT_FOUND",
        "The selected virtual output is no longer available. Install or reconnect the virtual audio driver, then refresh devices.",
    )
}

fn enumeration_error(message: &str) -> ApiError {
    log::warn!("audio device enumeration failed: {message}");
    ApiError::new(
        "AUDIO_DEVICE_ENUMERATION_FAILED",
        "Audio devices could not be listed. Check operating-system audio permissions and try again.",
    )
}

fn routing_error(message: String) -> ApiError {
    log::warn!("audio routing failed: {message}");
    ApiError::new(
        "AUDIO_ROUTING_FAILED",
        "Audio routing could not start. Check microphone permission and confirm both selected devices are available.",
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_supported_virtual_device_names() {
        assert!(is_virtual_device_name("BlackHole 2ch"));
        assert!(is_virtual_device_name(
            "CABLE Input (VB-Audio Virtual Cable)"
        ));
        assert!(is_virtual_device_name("VoiceMeeter Input"));
        assert!(!is_virtual_device_name("MacBook Pro Speakers"));
    }

    #[test]
    fn resampler_converts_rates_and_stays_bounded() {
        let (mut producer, consumer) = RingBuffer::new(32);
        for sample in [0.0, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5] {
            producer.push(sample).unwrap();
        }
        let mut resampler = VoiceResampler::new(consumer, 48_000, 96_000, 32, 8);
        let samples = (0..12).map(|_| resampler.next_sample()).collect::<Vec<_>>();
        assert!(samples.iter().all(|sample| (-1.0..=1.0).contains(sample)));
        assert!(samples.iter().any(|sample| *sample > 0.0));
    }
}
