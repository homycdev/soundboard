use std::path::PathBuf;

use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

use crate::error::ApiError;
use crate::ports::FilePicker;

pub struct TauriFilePicker<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriFilePicker<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> FilePicker for TauriFilePicker<R> {
    fn pick_audio_file(&self) -> Result<Option<PathBuf>, ApiError> {
        let selection = self
            .app
            .dialog()
            .file()
            .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
            .blocking_pick_file();
        match selection {
            None => Ok(None),
            Some(path) => path
                .into_path()
                .map(Some)
                .map_err(|_| ApiError::new("UNSUPPORTED_FORMAT", "Choose a local audio file.")),
        }
    }
}
