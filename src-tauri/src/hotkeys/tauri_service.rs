use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use serde_json::json;
use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut as NativeShortcut,
};

use crate::domain::{Modifier, Shortcut};
use crate::dto::ShortcutDto;
use crate::error::ApiError;
use crate::hotkeys::{HotkeyId, HotkeyService, HotkeyTarget};
use crate::ports::AudioService;

#[derive(Default)]
pub struct ShortcutRouter {
    targets: RwLock<HashMap<HotkeyId, HotkeyTarget>>,
    audio: RwLock<Option<Arc<dyn AudioService>>>,
    capture_active: AtomicBool,
}

impl ShortcutRouter {
    pub fn set_audio(&self, audio: Arc<dyn AudioService>) {
        *write(&self.audio) = Some(audio);
    }

    pub fn handle_pressed(&self, id: HotkeyId) {
        if self.capture_active.load(Ordering::Acquire) {
            return;
        }
        let target = read(&self.targets).get(&id).cloned();
        let audio = read(&self.audio).clone();
        if let (Some(target), Some(audio)) = (target, audio) {
            audio.try_play(target.request);
        }
    }

    pub fn set_capture_active(&self, active: bool) {
        self.capture_active.store(active, Ordering::Release);
    }
}

pub struct TauriHotkeyService<R: Runtime> {
    app: AppHandle<R>,
    router: Arc<ShortcutRouter>,
}

impl<R: Runtime> TauriHotkeyService<R> {
    pub fn new(app: AppHandle<R>, router: Arc<ShortcutRouter>) -> Self {
        Self { app, router }
    }
}

impl<R: Runtime> HotkeyService for TauriHotkeyService<R> {
    fn register(&self, shortcut: &Shortcut) -> Result<HotkeyId, ApiError> {
        let native = to_native(shortcut)?;
        let id = native.id();
        self.app
            .global_shortcut()
            .register(native)
            .map_err(|error| {
                log::debug!("global shortcut registration failed: {error}");
                ApiError::with_details(
                    "SHORTCUT_UNAVAILABLE",
                    "The shortcut could not be registered. It may be reserved by the operating system or another app.",
                    json!({ "shortcut": ShortcutDto::from(shortcut) }),
                )
            })?;
        Ok(id)
    }

    fn unregister(&self, shortcut: &Shortcut) -> Result<(), ApiError> {
        let native = to_native(shortcut)?;
        self.app
            .global_shortcut()
            .unregister(native)
            .map_err(|error| {
                log::debug!("global shortcut unregister failed: {error}");
                ApiError::internal()
            })
    }

    fn activate(&self, id: HotkeyId, target: HotkeyTarget) {
        write(&self.router.targets).insert(id, target);
    }

    fn deactivate(&self, id: HotkeyId) {
        write(&self.router.targets).remove(&id);
    }

    fn set_capture_active(&self, active: bool) {
        self.router.set_capture_active(active);
    }
}

fn to_native(shortcut: &Shortcut) -> Result<NativeShortcut, ApiError> {
    let mut modifiers = Modifiers::empty();
    for modifier in &shortcut.modifiers {
        modifiers |= match modifier {
            Modifier::Control => Modifiers::CONTROL,
            Modifier::Alt => Modifiers::ALT,
            Modifier::Shift => Modifiers::SHIFT,
            Modifier::Meta => Modifiers::META,
        };
    }
    let modifiers = (!modifiers.is_empty()).then_some(modifiers);
    Ok(NativeShortcut::new(modifiers, native_code(&shortcut.code)?))
}

fn native_code(code: &str) -> Result<Code, ApiError> {
    let code = match code {
        "KeyA" => Code::KeyA,
        "KeyB" => Code::KeyB,
        "KeyC" => Code::KeyC,
        "KeyD" => Code::KeyD,
        "KeyE" => Code::KeyE,
        "KeyF" => Code::KeyF,
        "KeyG" => Code::KeyG,
        "KeyH" => Code::KeyH,
        "KeyI" => Code::KeyI,
        "KeyJ" => Code::KeyJ,
        "KeyK" => Code::KeyK,
        "KeyL" => Code::KeyL,
        "KeyM" => Code::KeyM,
        "KeyN" => Code::KeyN,
        "KeyO" => Code::KeyO,
        "KeyP" => Code::KeyP,
        "KeyQ" => Code::KeyQ,
        "KeyR" => Code::KeyR,
        "KeyS" => Code::KeyS,
        "KeyT" => Code::KeyT,
        "KeyU" => Code::KeyU,
        "KeyV" => Code::KeyV,
        "KeyW" => Code::KeyW,
        "KeyX" => Code::KeyX,
        "KeyY" => Code::KeyY,
        "KeyZ" => Code::KeyZ,
        "Digit0" => Code::Digit0,
        "Digit1" => Code::Digit1,
        "Digit2" => Code::Digit2,
        "Digit3" => Code::Digit3,
        "Digit4" => Code::Digit4,
        "Digit5" => Code::Digit5,
        "Digit6" => Code::Digit6,
        "Digit7" => Code::Digit7,
        "Digit8" => Code::Digit8,
        "Digit9" => Code::Digit9,
        "F1" => Code::F1,
        "F2" => Code::F2,
        "F3" => Code::F3,
        "F4" => Code::F4,
        "F5" => Code::F5,
        "F6" => Code::F6,
        "F7" => Code::F7,
        "F8" => Code::F8,
        "F9" => Code::F9,
        "F10" => Code::F10,
        "F11" => Code::F11,
        "F12" => Code::F12,
        "F13" => Code::F13,
        "F14" => Code::F14,
        "F15" => Code::F15,
        "F16" => Code::F16,
        "F17" => Code::F17,
        "F18" => Code::F18,
        "F19" => Code::F19,
        "F20" => Code::F20,
        "F21" => Code::F21,
        "F22" => Code::F22,
        "F23" => Code::F23,
        "F24" => Code::F24,
        "ArrowDown" => Code::ArrowDown,
        "ArrowLeft" => Code::ArrowLeft,
        "ArrowRight" => Code::ArrowRight,
        "ArrowUp" => Code::ArrowUp,
        "Backquote" => Code::Backquote,
        "Backslash" => Code::Backslash,
        "Backspace" => Code::Backspace,
        "BracketLeft" => Code::BracketLeft,
        "BracketRight" => Code::BracketRight,
        "Comma" => Code::Comma,
        "Delete" => Code::Delete,
        "End" => Code::End,
        "Enter" => Code::Enter,
        "Equal" => Code::Equal,
        "Home" => Code::Home,
        "Insert" => Code::Insert,
        "Minus" => Code::Minus,
        "PageDown" => Code::PageDown,
        "PageUp" => Code::PageUp,
        "Period" => Code::Period,
        "Quote" => Code::Quote,
        "Semicolon" => Code::Semicolon,
        "Slash" => Code::Slash,
        "Space" => Code::Space,
        "Tab" => Code::Tab,
        _ => {
            return Err(ApiError::with_details(
                "SHORTCUT_INVALID",
                "Choose a supported key combination.",
                json!({ "reason": "That key cannot be registered consistently on this platform." }),
            ));
        }
    };
    Ok(code)
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::dto::Trigger;
    use crate::ports::{AudioMetadata, PlaybackRequest};

    #[derive(Default)]
    struct CountingAudio {
        plays: AtomicUsize,
    }

    impl AudioService for CountingAudio {
        fn is_available(&self) -> bool {
            true
        }

        fn probe(&self, _path: &Path) -> Result<AudioMetadata, ApiError> {
            unreachable!()
        }

        fn load(&self, _sound_id: &str, _path: &Path) -> Result<AudioMetadata, ApiError> {
            unreachable!()
        }

        fn unload(&self, _sound_id: &str) {}

        fn play(&self, _request: PlaybackRequest) -> Result<String, ApiError> {
            unreachable!()
        }

        fn try_play(&self, _request: PlaybackRequest) {
            self.plays.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn capture_mode_suppresses_global_shortcut_playback() {
        let router = ShortcutRouter::default();
        let audio = Arc::new(CountingAudio::default());
        router.set_audio(audio.clone());
        write(&router.targets).insert(
            7,
            HotkeyTarget {
                request: PlaybackRequest {
                    sound_id: "sound".to_owned(),
                    cell_id: "r0c0".to_owned(),
                    trigger: Trigger::GlobalShortcut,
                },
            },
        );

        router.set_capture_active(true);
        router.handle_pressed(7);
        assert_eq!(audio.plays.load(Ordering::Acquire), 0);

        router.set_capture_active(false);
        router.handle_pressed(7);
        assert_eq!(audio.plays.load(Ordering::Acquire), 1);
    }
}
