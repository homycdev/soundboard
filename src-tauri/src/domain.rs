use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiError;
use crate::hotkeys::normalize::{normalize_shortcut, validate_shortcut};

pub const SCHEMA_VERSION: u32 = 1;
pub const GRID_MIN: u8 = 1;
pub const GRID_MAX: u8 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Mp3,
    Wav,
    Ogg,
    Flac,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::Flac => "flac",
        }
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "mp3" => Some(Self::Mp3),
            "wav" => Some(Self::Wav),
            "ogg" => Some(Self::Ogg),
            "flac" => Some(Self::Flac),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Modifier {
    Control,
    Alt,
    Shift,
    Meta,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub modifiers: Vec<Modifier>,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid {
    pub rows: u8,
    pub columns: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: Uuid,
    pub display_name: String,
    pub original_file_name: String,
    pub stored_file_name: String,
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub shortcut: Option<Shortcut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub cell_id: String,
    pub sound: Sound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedState {
    pub schema_version: u32,
    pub grid: Grid,
    pub assignments: Vec<Assignment>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            grid: Grid {
                rows: 4,
                columns: 4,
            },
            assignments: Vec::new(),
        }
    }
}

impl PersistedState {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ApiError::new(
                "STATE_VERSION_UNSUPPORTED",
                "This soundboard was created by a newer app version and was left untouched.",
            ));
        }
        validate_grid(self.grid.rows, self.grid.columns)?;

        let mut cell_ids = HashSet::new();
        let mut sound_ids = HashSet::new();
        let mut stored_names = HashSet::new();
        let mut shortcuts = HashSet::new();

        for assignment in &self.assignments {
            let (row, column) = parse_cell_id(&assignment.cell_id)?;
            if row >= self.grid.rows || column >= self.grid.columns {
                return Err(invalid_state(
                    "An assignment is outside the configured grid.",
                ));
            }
            if !cell_ids.insert(assignment.cell_id.clone()) {
                return Err(invalid_state(
                    "The saved soundboard contains duplicate cells.",
                ));
            }
            if !sound_ids.insert(assignment.sound.id) {
                return Err(invalid_state(
                    "The saved soundboard contains duplicate sound IDs.",
                ));
            }
            if !stored_names.insert(assignment.sound.stored_file_name.clone()) {
                return Err(invalid_state(
                    "The saved soundboard contains duplicate stored files.",
                ));
            }

            validate_sound(&assignment.sound)?;
            if let Some(shortcut) = &assignment.sound.shortcut {
                validate_shortcut(shortcut)?;
                if normalize_shortcut(shortcut.clone())? != *shortcut {
                    return Err(invalid_state("A saved shortcut is not in canonical order."));
                }
                if !shortcuts.insert(shortcut.clone()) {
                    return Err(invalid_state(
                        "The saved soundboard contains duplicate shortcuts.",
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn assignment(&self, cell_id: &str) -> Option<&Assignment> {
        self.assignments.iter().find(|item| item.cell_id == cell_id)
    }

    pub fn assignment_mut(&mut self, cell_id: &str) -> Option<&mut Assignment> {
        self.assignments
            .iter_mut()
            .find(|item| item.cell_id == cell_id)
    }

    pub fn sort_assignments(&mut self) {
        self.assignments.sort_by_key(|assignment| {
            parse_cell_id(&assignment.cell_id).unwrap_or((u8::MAX, u8::MAX))
        });
    }
}

pub fn validate_grid(rows: u8, columns: u8) -> Result<(), ApiError> {
    if !(GRID_MIN..=GRID_MAX).contains(&rows) || !(GRID_MIN..=GRID_MAX).contains(&columns) {
        return Err(ApiError::with_details(
            "GRID_INVALID",
            format!("Rows and columns must be between {GRID_MIN} and {GRID_MAX}."),
            json!({
                "min": GRID_MIN,
                "max": GRID_MAX,
                "requested": { "rows": rows, "columns": columns }
            }),
        ));
    }
    Ok(())
}

pub fn parse_cell_id(cell_id: &str) -> Result<(u8, u8), ApiError> {
    let Some(rest) = cell_id.strip_prefix('r') else {
        return Err(invalid_cell());
    };
    let Some((row, column)) = rest.split_once('c') else {
        return Err(invalid_cell());
    };
    if row.is_empty()
        || column.is_empty()
        || row.starts_with('+')
        || column.starts_with('+')
        || row.len() > 1 && row.starts_with('0')
        || column.len() > 1 && column.starts_with('0')
    {
        return Err(invalid_cell());
    }
    let row = row.parse::<u8>().map_err(|_| invalid_cell())?;
    let column = column.parse::<u8>().map_err(|_| invalid_cell())?;
    Ok((row, column))
}

pub fn validate_cell_in_grid(cell_id: &str, grid: &Grid) -> Result<(u8, u8), ApiError> {
    let (row, column) = parse_cell_id(cell_id)?;
    if row >= grid.rows || column >= grid.columns {
        return Err(ApiError::new(
            "NOT_FOUND",
            "That sound cell does not exist.",
        ));
    }
    Ok((row, column))
}

fn validate_sound(sound: &Sound) -> Result<(), ApiError> {
    let trimmed = sound.display_name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 {
        return Err(invalid_state("A saved sound has an invalid display name."));
    }
    let original_path = Path::new(&sound.original_file_name);
    if sound.original_file_name.trim().is_empty()
        || original_path.file_name().and_then(|name| name.to_str())
            != Some(&sound.original_file_name)
        || original_path.components().count() != 1
    {
        return Err(invalid_state(
            "A saved sound has an invalid original file name.",
        ));
    }

    let stored_path = Path::new(&sound.stored_file_name);
    if stored_path.file_name().and_then(|name| name.to_str()) != Some(&sound.stored_file_name)
        || stored_path.components().count() != 1
    {
        return Err(invalid_state(
            "A saved sound has an unsafe stored file name.",
        ));
    }
    let expected = format!("{}.{}", sound.id, sound.format.extension());
    if sound.stored_file_name != expected {
        return Err(invalid_state(
            "A saved sound has an invalid stored file name.",
        ));
    }
    Ok(())
}

fn invalid_cell() -> ApiError {
    ApiError::new("NOT_FOUND", "That sound cell does not exist.")
}

fn invalid_state(message: &str) -> ApiError {
    ApiError::new("PERSISTENCE_FAILED", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_ids_parse_strictly() {
        assert_eq!(parse_cell_id("r0c0").unwrap(), (0, 0));
        assert_eq!(parse_cell_id("r11c7").unwrap(), (11, 7));
        for invalid in ["0c0", "r0", "r-1c0", "r00c0", "r0c", "r0c0extra"] {
            assert!(parse_cell_id(invalid).is_err(), "{invalid} should fail");
        }
    }

    #[test]
    fn cell_bounds_are_checked() {
        let grid = Grid {
            rows: 2,
            columns: 3,
        };
        assert!(validate_cell_in_grid("r1c2", &grid).is_ok());
        assert!(validate_cell_in_grid("r2c0", &grid).is_err());
        assert!(validate_cell_in_grid("r0c3", &grid).is_err());
    }
}
