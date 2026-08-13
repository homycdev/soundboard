use std::path::{Path, PathBuf};

use crate::domain::{AudioRoutingSettings, PersistedState};
use crate::dto::{PlaybackFailed, PlaybackStarted, Trigger};
use crate::error::ApiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMetadata {
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaybackRequest {
    pub sound_id: String,
    pub cell_id: String,
    pub trigger: Trigger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_virtual: bool,
}

#[derive(Debug, Clone)]
pub struct AudioRoutingRuntime {
    pub active: bool,
    pub input_devices: Vec<AudioDeviceInfo>,
    pub output_devices: Vec<AudioDeviceInfo>,
    pub error: Option<ApiError>,
}

impl AudioRoutingRuntime {
    pub fn unsupported() -> Self {
        Self {
            active: false,
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            error: Some(ApiError::new(
                "AUDIO_ROUTING_UNSUPPORTED",
                "Virtual-microphone routing is not available on this platform.",
            )),
        }
    }
}

pub trait AudioService: Send + Sync {
    fn is_available(&self) -> bool;
    fn probe(&self, path: &Path) -> Result<AudioMetadata, ApiError>;
    fn load(&self, sound_id: &str, path: &Path) -> Result<AudioMetadata, ApiError>;
    fn unload(&self, sound_id: &str);
    fn play(&self, request: PlaybackRequest) -> Result<String, ApiError>;
    fn try_play(&self, request: PlaybackRequest);

    fn routing_runtime(&self) -> Result<AudioRoutingRuntime, ApiError> {
        Ok(AudioRoutingRuntime::unsupported())
    }

    fn configure_routing(&self, _settings: &AudioRoutingSettings) -> Result<(), ApiError> {
        Err(ApiError::new(
            "AUDIO_ROUTING_UNSUPPORTED",
            "Virtual-microphone routing is not available on this platform.",
        ))
    }

    fn disable_routing(&self) -> Result<(), ApiError> {
        Ok(())
    }
}

pub trait PlaybackEventSink: Send + Sync {
    fn started(&self, event: PlaybackStarted);
    fn failed(&self, event: PlaybackFailed);
}

pub trait FilePicker: Send + Sync {
    fn pick_audio_file(&self) -> Result<Option<PathBuf>, ApiError>;
}

#[derive(Debug, Clone)]
pub struct RepositoryLoad {
    pub state: PersistedState,
    pub warnings: Vec<crate::dto::AppWarningDto>,
}

pub trait StateRepository: Send + Sync {
    fn load(&self) -> Result<RepositoryLoad, ApiError>;
    fn save(&self, state: &PersistedState) -> Result<(), ApiError>;
    fn audio_dir(&self) -> &Path;
    fn audio_path(&self, stored_file_name: &str) -> Result<PathBuf, ApiError>;

    fn load_audio_routing(&self) -> Result<AudioRoutingSettings, ApiError> {
        Ok(AudioRoutingSettings::default())
    }

    fn save_audio_routing(&self, _settings: &AudioRoutingSettings) -> Result<(), ApiError> {
        Ok(())
    }
}
