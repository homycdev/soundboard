pub mod normalize;
pub mod tauri_service;

use crate::domain::Shortcut;
use crate::error::ApiError;
use crate::ports::PlaybackRequest;

pub type HotkeyId = u32;

#[derive(Debug, Clone)]
pub struct HotkeyTarget {
    pub request: PlaybackRequest,
}

pub trait HotkeyService: Send + Sync {
    fn register(&self, shortcut: &Shortcut) -> Result<HotkeyId, ApiError>;
    fn unregister(&self, shortcut: &Shortcut) -> Result<(), ApiError>;
    fn activate(&self, id: HotkeyId, target: HotkeyTarget);
    fn deactivate(&self, id: HotkeyId);
    fn set_capture_active(&self, active: bool);
}
