use serde::{Deserialize, Serialize};

use crate::domain::{AudioFormat, Modifier, PersistedState, Shortcut};
use crate::hotkeys::normalize::format_shortcut;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutDto {
    pub modifiers: Vec<Modifier>,
    pub code: String,
    pub display: String,
}

impl From<&Shortcut> for ShortcutDto {
    fn from(shortcut: &Shortcut) -> Self {
        Self {
            modifiers: shortcut.modifiers.clone(),
            code: shortcut.code.clone(),
            display: format_shortcut(shortcut),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutInput {
    pub modifiers: Vec<Modifier>,
    pub code: String,
}

impl From<ShortcutInput> for Shortcut {
    fn from(input: ShortcutInput) -> Self {
        Self {
            modifiers: input.modifiers,
            code: input.code,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutStatus {
    Registered,
    Unavailable,
    #[allow(dead_code)]
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundDto {
    pub id: String,
    pub display_name: String,
    pub format: AudioFormat,
    pub duration_ms: u64,
    pub shortcut: Option<ShortcutDto>,
    pub shortcut_status: Option<ShortcutStatus>,
    pub playable: bool,
    pub problem: Option<ProblemDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDto {
    pub cell_id: String,
    pub row: u8,
    pub column: u8,
    pub sound: Option<SoundDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridDto {
    pub rows: u8,
    pub columns: u8,
    pub min: u8,
    pub max: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWarningDto {
    pub code: String,
    pub message: String,
    pub cell_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub schema_version: u32,
    pub grid: GridDto,
    pub cells: Vec<CellDto>,
    pub warnings: Vec<AppWarningDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    Pointer,
    Keyboard,
    GlobalShortcut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandTrigger {
    Pointer,
    Keyboard,
}

impl From<CommandTrigger> for Trigger {
    fn from(trigger: CommandTrigger) -> Self {
        match trigger {
            CommandTrigger::Pointer => Self::Pointer,
            CommandTrigger::Keyboard => Self::Keyboard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStarted {
    pub instance_id: String,
    pub sound_id: String,
    pub cell_id: String,
    pub trigger: Trigger,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackFailed {
    pub sound_id: Option<String>,
    pub cell_id: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayResult {
    pub instance_id: String,
}

pub fn empty_snapshot(state: &PersistedState) -> AppSnapshot {
    AppSnapshot {
        schema_version: state.schema_version,
        grid: GridDto {
            rows: state.grid.rows,
            columns: state.grid.columns,
            min: crate::domain::GRID_MIN,
            max: crate::domain::GRID_MAX,
        },
        cells: Vec::new(),
        warnings: Vec::new(),
    }
}
