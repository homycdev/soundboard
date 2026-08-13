use tauri::{AppHandle, Emitter};

use crate::dto::{PlaybackFailed, PlaybackStarted};
use crate::ports::PlaybackEventSink;

pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl PlaybackEventSink for TauriEventSink {
    fn started(&self, event: PlaybackStarted) {
        if let Err(error) = self.app.emit("soundboard://playback-started", event) {
            log::warn!("failed to emit playback-started event: {error}");
        }
    }

    fn failed(&self, event: PlaybackFailed) {
        if let Err(error) = self.app.emit("soundboard://playback-failed", event) {
            log::warn!("failed to emit playback-failed event: {error}");
        }
    }
}
