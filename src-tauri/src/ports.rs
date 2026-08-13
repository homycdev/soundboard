use std::path::{Path, PathBuf};

use crate::domain::PersistedState;
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

pub trait AudioService: Send + Sync {
    fn is_available(&self) -> bool;
    fn probe(&self, path: &Path) -> Result<AudioMetadata, ApiError>;
    fn load(&self, sound_id: &str, path: &Path) -> Result<AudioMetadata, ApiError>;
    fn unload(&self, sound_id: &str);
    fn play(&self, request: PlaybackRequest) -> Result<String, ApiError>;
    fn try_play(&self, request: PlaybackRequest);
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
}
