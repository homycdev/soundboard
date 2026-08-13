use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::json;
use uuid::Uuid;

use crate::domain::AudioFormat;
use crate::error::ApiError;
use crate::ports::AudioService;

pub mod tauri_picker;

pub const MAX_SOURCE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug)]
pub struct PreparedImport {
    pub id: Uuid,
    pub display_name: String,
    pub original_file_name: String,
    pub stored_file_name: String,
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub stored_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileFingerprint {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

pub fn prepare_import(
    source: &Path,
    audio_dir: &Path,
    audio: &dyn AudioService,
) -> Result<PreparedImport, ApiError> {
    let canonical = fs::canonicalize(source).map_err(|_| unreadable())?;
    let before_metadata = fs::metadata(&canonical).map_err(|_| unreadable())?;
    if !before_metadata.is_file() || before_metadata.len() == 0 {
        return Err(unreadable());
    }
    if before_metadata.len() > MAX_SOURCE_BYTES {
        return Err(ApiError::with_details(
            "FILE_TOO_LARGE",
            "The selected file is larger than the 50 MiB import limit.",
            json!({ "maxBytes": MAX_SOURCE_BYTES, "bytes": before_metadata.len() }),
        ));
    }
    let before = FileFingerprint::from_metadata(&before_metadata);

    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(unsupported)?;
    let format = AudioFormat::from_extension(extension).ok_or_else(unsupported)?;
    let metadata = audio.probe(&canonical)?;

    let id = Uuid::new_v4();
    let stored_file_name = format!("{id}.{}", format.extension());
    let stored_path = audio_dir.join(&stored_file_name);
    let temporary_path = audio_dir.join(format!("{id}.importing"));
    let copy_result = copy_verified(&canonical, &temporary_path, &before).and_then(|()| {
        fs::rename(&temporary_path, &stored_path).map_err(|_| ApiError::persistence())
    });
    if let Err(error) = copy_result {
        let _ = fs::remove_file(&temporary_path);
        let _ = fs::remove_file(&stored_path);
        return Err(error);
    }

    let loaded = match audio.load(&id.to_string(), &stored_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            let _ = fs::remove_file(&stored_path);
            return Err(error);
        }
    };

    let original_file_name = canonical
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("sound.{}", format.extension()));
    let display_name = safe_display_name(&canonical);

    Ok(PreparedImport {
        id,
        display_name,
        original_file_name,
        stored_file_name,
        format,
        duration_ms: loaded.duration_ms.max(metadata.duration_ms),
        stored_path,
    })
}

pub fn rollback_import(import: &PreparedImport, audio: &dyn AudioService) {
    audio.unload(&import.id.to_string());
    let _ = fs::remove_file(&import.stored_path);
}

fn copy_verified(
    source: &Path,
    destination: &Path,
    before: &FileFingerprint,
) -> Result<(), ApiError> {
    let mut input = File::open(source).map_err(|_| unreadable())?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(|_| ApiError::persistence())?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    loop {
        let read = input.read(&mut buffer).map_err(|_| unreadable())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|_| ApiError::persistence())?;
        copied += read as u64;
        if copied > MAX_SOURCE_BYTES {
            return Err(ApiError::with_details(
                "FILE_TOO_LARGE",
                "The selected file is larger than the 50 MiB import limit.",
                json!({ "maxBytes": MAX_SOURCE_BYTES, "bytes": copied }),
            ));
        }
    }
    output.flush().map_err(|_| ApiError::persistence())?;
    output.sync_all().map_err(|_| ApiError::persistence())?;

    let after = fs::metadata(source)
        .map(|metadata| FileFingerprint::from_metadata(&metadata))
        .map_err(|_| unreadable())?;
    if &after != before || copied != before.len {
        return Err(ApiError::new(
            "AUDIO_DECODE_FAILED",
            "The selected file changed during import. Try again.",
        ));
    }
    Ok(())
}

pub fn safe_display_name(path: &Path) -> String {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "Untitled sound".to_owned();
    }
    trimmed.chars().take(120).collect()
}

fn unsupported() -> ApiError {
    ApiError::new(
        "UNSUPPORTED_FORMAT",
        "Choose an MP3, WAV, OGG/Vorbis, or FLAC file.",
    )
}

fn unreadable() -> ApiError {
    ApiError::new(
        "AUDIO_DECODE_FAILED",
        "The selected file could not be read as audio.",
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tempfile::tempdir;

    use super::*;
    use crate::ports::{AudioMetadata, PlaybackRequest};

    struct FakeAudio {
        probe_error: bool,
        loaded: Mutex<Vec<String>>,
    }

    impl AudioService for FakeAudio {
        fn is_available(&self) -> bool {
            true
        }
        fn probe(&self, _path: &Path) -> Result<AudioMetadata, ApiError> {
            if self.probe_error {
                Err(ApiError::decode())
            } else {
                Ok(AudioMetadata { duration_ms: 42 })
            }
        }
        fn load(&self, id: &str, _path: &Path) -> Result<AudioMetadata, ApiError> {
            self.loaded.lock().unwrap().push(id.into());
            Ok(AudioMetadata { duration_ms: 42 })
        }
        fn unload(&self, _sound_id: &str) {}
        fn play(&self, _request: PlaybackRequest) -> Result<String, ApiError> {
            unreachable!()
        }
        fn try_play(&self, _request: PlaybackRequest) {
            unreachable!()
        }
    }

    #[test]
    fn checks_extension_size_decode_and_safe_name() {
        let directory = tempdir().unwrap();
        let audio_dir = directory.path().join("audio");
        fs::create_dir(&audio_dir).unwrap();
        let unsupported_path = directory.path().join("clip.aac");
        fs::write(&unsupported_path, b"audio").unwrap();
        let audio = FakeAudio {
            probe_error: false,
            loaded: Mutex::new(Vec::new()),
        };
        assert_eq!(
            prepare_import(&unsupported_path, &audio_dir, &audio)
                .unwrap_err()
                .code,
            "UNSUPPORTED_FORMAT"
        );

        let path = directory.path().join("   .mp3");
        fs::write(&path, b"audio").unwrap();
        let prepared = prepare_import(&path, &audio_dir, &audio).unwrap();
        assert_eq!(prepared.display_name, "Untitled sound");
        assert!(prepared.stored_path.exists());

        let broken_path = directory.path().join("broken.wav");
        fs::write(&broken_path, b"audio").unwrap();
        let broken_audio = FakeAudio {
            probe_error: true,
            loaded: Mutex::new(Vec::new()),
        };
        assert_eq!(
            prepare_import(&broken_path, &audio_dir, &broken_audio)
                .unwrap_err()
                .code,
            "AUDIO_DECODE_FAILED"
        );

        let oversized_path = directory.path().join("oversized.flac");
        File::create(&oversized_path)
            .unwrap()
            .set_len(MAX_SOURCE_BYTES + 1)
            .unwrap();
        let error = prepare_import(&oversized_path, &audio_dir, &audio).unwrap_err();
        assert_eq!(error.code, "FILE_TOO_LARGE");
        let details = error.details.unwrap();
        assert_eq!(details["maxBytes"], MAX_SOURCE_BYTES);
        assert_eq!(details["bytes"], MAX_SOURCE_BYTES + 1);
    }
}
