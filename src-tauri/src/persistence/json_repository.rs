use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::domain::{AudioRoutingSettings, PersistedState, SCHEMA_VERSION};
use crate::dto::AppWarningDto;
use crate::error::ApiError;
use crate::ports::{RepositoryLoad, StateRepository};

pub struct JsonRepository {
    root: PathBuf,
    audio_dir: PathBuf,
    backups_dir: PathBuf,
    current: PathBuf,
    next: PathBuf,
    previous: PathBuf,
    routing_current: PathBuf,
    routing_next: PathBuf,
    routing_previous: PathBuf,
}

enum Candidate {
    Missing,
    Valid(PersistedState),
    Future,
    Invalid,
}

impl JsonRepository {
    pub fn new(root: PathBuf) -> Result<Self, ApiError> {
        let repository = Self {
            audio_dir: root.join("audio"),
            backups_dir: root.join("backups"),
            current: root.join("state.json"),
            next: root.join("state.next.json"),
            previous: root.join("state.previous.json"),
            routing_current: root.join("audio-routing.json"),
            routing_next: root.join("audio-routing.next.json"),
            routing_previous: root.join("audio-routing.previous.json"),
            root,
        };
        repository.ensure_directories()?;
        Ok(repository)
    }

    fn ensure_directories(&self) -> Result<(), ApiError> {
        fs::create_dir_all(&self.root).map_err(|_| ApiError::persistence())?;
        fs::create_dir_all(&self.audio_dir).map_err(|_| ApiError::persistence())?;
        fs::create_dir_all(&self.backups_dir).map_err(|_| ApiError::persistence())?;
        Ok(())
    }

    fn read_candidate(path: &Path) -> Candidate {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Candidate::Missing;
            }
            Err(_) => return Candidate::Invalid,
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Candidate::Invalid,
        };
        let Some(version) = value.get("schemaVersion").and_then(Value::as_u64) else {
            return Candidate::Invalid;
        };
        if version > u64::from(SCHEMA_VERSION) {
            return Candidate::Future;
        }
        let migrated = match migrate_to_current(value, version as u32) {
            Ok(value) => value,
            Err(()) => return Candidate::Invalid,
        };
        let state: PersistedState = match serde_json::from_value(migrated) {
            Ok(state) => state,
            Err(_) => return Candidate::Invalid,
        };
        match state.validate() {
            Ok(()) => Candidate::Valid(state),
            Err(_) => Candidate::Invalid,
        }
    }

    fn backup_corrupt_current(&self) -> Result<(), ApiError> {
        if !self.current.exists() {
            return Ok(());
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let destination = self
            .backups_dir
            .join(format!("state.corrupt-{timestamp}.json"));
        fs::rename(&self.current, destination).map_err(|_| ApiError::persistence())
    }

    fn recovery_warning(message: &str) -> AppWarningDto {
        AppWarningDto {
            code: "STATE_RECOVERED".to_owned(),
            message: message.to_owned(),
            cell_id: None,
        }
    }

    fn replace_path(source: &Path, destination: &Path) -> std::io::Result<()> {
        if destination.exists() {
            fs::remove_file(destination)?;
        }
        fs::rename(source, destination)
    }

    #[cfg(unix)]
    fn sync_directory(&self) -> std::io::Result<()> {
        fs::File::open(&self.root)?.sync_all()
    }

    #[cfg(not(unix))]
    fn sync_directory(&self) -> std::io::Result<()> {
        Ok(())
    }
}

impl StateRepository for JsonRepository {
    fn load(&self) -> Result<RepositoryLoad, ApiError> {
        self.ensure_directories()?;
        let current = Self::read_candidate(&self.current);
        match current {
            Candidate::Valid(state) => {
                let _ = fs::remove_file(&self.next);
                let _ = fs::remove_file(&self.previous);
                return Ok(RepositoryLoad {
                    state,
                    warnings: Vec::new(),
                });
            }
            Candidate::Future => {
                return Err(ApiError::new(
                    "STATE_VERSION_UNSUPPORTED",
                    "This soundboard was created by a newer app version and was left untouched.",
                ));
            }
            Candidate::Missing | Candidate::Invalid => {}
        }

        let current_was_corrupt = matches!(current, Candidate::Invalid);
        for candidate_path in [&self.next, &self.previous] {
            match Self::read_candidate(candidate_path) {
                Candidate::Valid(state) => {
                    if current_was_corrupt {
                        self.backup_corrupt_current()?;
                    }
                    self.save(&state)?;
                    return Ok(RepositoryLoad {
                        state,
                        warnings: vec![Self::recovery_warning(
                            "Soundboard data was recovered after an interrupted save.",
                        )],
                    });
                }
                Candidate::Future => {
                    return Err(ApiError::new(
                        "STATE_VERSION_UNSUPPORTED",
                        "This soundboard was created by a newer app version and was left untouched.",
                    ));
                }
                Candidate::Missing | Candidate::Invalid => {}
            }
        }

        if current_was_corrupt {
            self.backup_corrupt_current()?;
        }
        let state = PersistedState::default();
        self.save(&state)?;
        let warnings = if current_was_corrupt {
            vec![Self::recovery_warning(
                "Saved soundboard data was corrupt. A backup was kept and a new board was created.",
            )]
        } else {
            Vec::new()
        };
        Ok(RepositoryLoad { state, warnings })
    }

    fn save(&self, state: &PersistedState) -> Result<(), ApiError> {
        state.validate().map_err(|_| ApiError::persistence())?;
        self.ensure_directories()?;
        let bytes = serde_json::to_vec_pretty(state).map_err(|_| ApiError::persistence())?;

        let write_result = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&self.next)?;
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);

            if self.current.exists() {
                Self::replace_path(&self.current, &self.previous)?;
            }
            if let Err(error) = Self::replace_path(&self.next, &self.current) {
                if !self.current.exists() && self.previous.exists() {
                    let _ = Self::replace_path(&self.previous, &self.current);
                }
                return Err(error);
            }
            self.sync_directory()?;

            if !matches!(Self::read_candidate(&self.current), Candidate::Valid(_)) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "written state did not validate",
                ));
            }
            let _ = fs::remove_file(&self.previous);
            let _ = fs::remove_file(&self.next);
            Ok(())
        })();

        write_result.map_err(|_| ApiError::persistence())
    }

    fn audio_dir(&self) -> &Path {
        &self.audio_dir
    }

    fn audio_path(&self, stored_file_name: &str) -> Result<PathBuf, ApiError> {
        let path = Path::new(stored_file_name);
        let mut components = path.components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(ApiError::new(
                "PERSISTENCE_FAILED",
                "A managed audio file name is invalid.",
            ));
        }
        Ok(self.audio_dir.join(path))
    }

    fn load_audio_routing(&self) -> Result<AudioRoutingSettings, ApiError> {
        let candidates = [
            &self.routing_current,
            &self.routing_next,
            &self.routing_previous,
        ];
        let any_candidate = candidates.iter().any(|path| path.exists());
        for path in candidates {
            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            let Ok(settings) = serde_json::from_slice::<AudioRoutingSettings>(&bytes) else {
                continue;
            };
            if settings.validate().is_err() {
                continue;
            }
            if path != &self.routing_current {
                self.save_audio_routing(&settings)?;
            } else {
                let _ = fs::remove_file(&self.routing_next);
                let _ = fs::remove_file(&self.routing_previous);
            }
            return Ok(settings);
        }
        if any_candidate {
            Err(ApiError::new(
                "AUDIO_ROUTING_SETTINGS_INVALID",
                "Saved audio-routing settings could not be read and were not applied.",
            ))
        } else {
            Ok(AudioRoutingSettings::default())
        }
    }

    fn save_audio_routing(&self, settings: &AudioRoutingSettings) -> Result<(), ApiError> {
        settings.validate().map_err(|_| ApiError::persistence())?;
        self.ensure_directories()?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|_| ApiError::persistence())?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.routing_next)
            .map_err(|_| ApiError::persistence())?;
        file.write_all(&bytes)
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_all())
            .map_err(|_| ApiError::persistence())?;
        drop(file);
        if self.routing_current.exists() {
            Self::replace_path(&self.routing_current, &self.routing_previous)
                .map_err(|_| ApiError::persistence())?;
        }
        if let Err(error) = Self::replace_path(&self.routing_next, &self.routing_current) {
            if !self.routing_current.exists() && self.routing_previous.exists() {
                let _ = Self::replace_path(&self.routing_previous, &self.routing_current);
            }
            log::warn!("audio-routing settings commit failed: {error}");
            return Err(ApiError::persistence());
        }
        self.sync_directory().map_err(|_| ApiError::persistence())?;
        let written = fs::read(&self.routing_current)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<AudioRoutingSettings>(&bytes).ok())
            .is_some_and(|written| written == *settings && written.validate().is_ok());
        if !written {
            return Err(ApiError::persistence());
        }
        let _ = fs::remove_file(&self.routing_previous);
        let _ = fs::remove_file(&self.routing_next);
        Ok(())
    }
}

fn migrate_to_current(value: Value, version: u32) -> Result<Value, ()> {
    match version {
        1 => Ok(value),
        // No earlier public schema exists yet. Future migrations belong here as
        // explicit vN -> vN+1 transformations.
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::domain::{Assignment, AudioFormat, Grid, Sound};

    fn populated_state() -> PersistedState {
        let sound_id = uuid::Uuid::new_v4();
        PersistedState {
            schema_version: 1,
            grid: Grid {
                rows: 2,
                columns: 2,
            },
            assignments: vec![Assignment {
                cell_id: "r1c1".into(),
                sound: Sound {
                    id: sound_id,
                    display_name: "Air horn".into(),
                    original_file_name: "air.mp3".into(),
                    stored_file_name: format!("{sound_id}.mp3"),
                    format: AudioFormat::Mp3,
                    duration_ms: 1234,
                    shortcut: None,
                },
            }],
        }
    }

    #[test]
    fn schema_v1_round_trips() {
        let directory = tempdir().unwrap();
        let repository = JsonRepository::new(directory.path().into()).unwrap();
        let state = populated_state();
        repository.save(&state).unwrap();
        assert_eq!(repository.load().unwrap().state, state);
    }

    #[test]
    fn audio_routing_settings_round_trip_separately_from_board_state() {
        let directory = tempdir().unwrap();
        let repository = JsonRepository::new(directory.path().into()).unwrap();
        let settings = AudioRoutingSettings {
            enabled: true,
            input_device_id: Some("coreaudio:microphone".into()),
            virtual_output_device_id: Some("coreaudio:BlackHole 2ch".into()),
            microphone_gain_percent: 85,
            soundboard_gain_percent: 120,
            monitor_enabled: false,
            ..AudioRoutingSettings::default()
        };

        repository.save_audio_routing(&settings).unwrap();

        assert_eq!(repository.load_audio_routing().unwrap(), settings);
        assert!(!repository.current.exists());
        assert!(repository.routing_current.exists());
    }

    #[test]
    fn malformed_and_duplicate_states_are_rejected() {
        let mut state = populated_state();
        state.assignments.push(state.assignments[0].clone());
        assert!(state.validate().is_err());

        let directory = tempdir().unwrap();
        let repository = JsonRepository::new(directory.path().into()).unwrap();
        fs::write(&repository.current, b"not-json").unwrap();
        let loaded = repository.load().unwrap();
        assert!(loaded.state.assignments.is_empty());
        assert_eq!(fs::read_dir(&repository.backups_dir).unwrap().count(), 1);
    }

    #[test]
    fn recovers_from_next_then_previous_candidates() {
        for file_name in ["state.next.json", "state.previous.json"] {
            let directory = tempdir().unwrap();
            let repository = JsonRepository::new(directory.path().into()).unwrap();
            let state = populated_state();
            fs::write(
                directory.path().join(file_name),
                serde_json::to_vec(&state).unwrap(),
            )
            .unwrap();
            let loaded = repository.load().unwrap();
            assert_eq!(loaded.state, state);
            assert_eq!(loaded.warnings.len(), 1);
        }
    }

    #[test]
    fn future_schema_is_never_overwritten_or_backed_up() {
        let directory = tempdir().unwrap();
        let repository = JsonRepository::new(directory.path().into()).unwrap();
        let future = serde_json::to_vec(&json!({
            "schemaVersion": 99,
            "grid": { "rows": 4, "columns": 4 },
            "assignments": []
        }))
        .unwrap();
        fs::write(&repository.current, &future).unwrap();

        let error = repository.load().unwrap_err();
        assert_eq!(error.code, "STATE_VERSION_UNSUPPORTED");
        assert_eq!(fs::read(&repository.current).unwrap(), future);
        assert_eq!(fs::read_dir(&repository.backups_dir).unwrap().count(), 0);
    }
}
