use serde_json::json;

use crate::domain::{Modifier, Shortcut};
use crate::error::ApiError;

const NAMED_CODES: &[&str] = &[
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "Backquote",
    "Backslash",
    "Backspace",
    "BracketLeft",
    "BracketRight",
    "Comma",
    "Delete",
    "End",
    "Enter",
    "Equal",
    "Home",
    "Insert",
    "Minus",
    "PageDown",
    "PageUp",
    "Period",
    "Quote",
    "Semicolon",
    "Slash",
    "Space",
    "Tab",
];

#[cfg_attr(not(test), allow(dead_code))]
pub fn normalize_modifier_name(value: &str) -> Option<Modifier> {
    match value.to_ascii_uppercase().as_str() {
        "CONTROL" | "CONTROLLEFT" | "CONTROLRIGHT" | "CTRL" => Some(Modifier::Control),
        "ALT" | "ALTLEFT" | "ALTRIGHT" | "OPTION" => Some(Modifier::Alt),
        "SHIFT" | "SHIFTLEFT" | "SHIFTRIGHT" => Some(Modifier::Shift),
        "META" | "METALEFT" | "METARIGHT" | "COMMAND" | "SUPER" | "WIN" => Some(Modifier::Meta),
        _ => None,
    }
}

pub fn normalize_shortcut(mut shortcut: Shortcut) -> Result<Shortcut, ApiError> {
    shortcut.modifiers.sort_by_key(modifier_rank);
    shortcut.modifiers.dedup();
    validate_shortcut(&shortcut)?;
    Ok(shortcut)
}

pub fn validate_shortcut(shortcut: &Shortcut) -> Result<(), ApiError> {
    if shortcut.modifiers.is_empty() && !is_function_key(&shortcut.code) {
        return Err(invalid(
            "Add at least one modifier such as Ctrl, Alt, Shift, or Meta.",
        ));
    }
    if !is_supported_code(&shortcut.code) {
        return Err(invalid("That key cannot be used for a global shortcut."));
    }

    let mut previous = None;
    for modifier in &shortcut.modifiers {
        let rank = modifier_rank(modifier);
        if previous.is_some_and(|value| value >= rank) {
            return Err(invalid(
                "Shortcut modifiers must be unique and in canonical order.",
            ));
        }
        previous = Some(rank);
    }
    Ok(())
}

pub fn format_shortcut(shortcut: &Shortcut) -> String {
    let mut parts = shortcut
        .modifiers
        .iter()
        .map(modifier_label)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    parts.push(code_label(&shortcut.code));
    parts.join(" + ")
}

pub fn is_supported_code(code: &str) -> bool {
    is_letter(code) || is_digit(code) || is_function_key(code) || NAMED_CODES.contains(&code)
}

fn is_letter(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 4 && bytes.starts_with(b"Key") && bytes[3].is_ascii_uppercase()
}

fn is_digit(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 6 && bytes.starts_with(b"Digit") && bytes[5].is_ascii_digit()
}

fn is_function_key(code: &str) -> bool {
    code.strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number))
}

fn modifier_rank(modifier: &Modifier) -> u8 {
    match modifier {
        Modifier::Control => 0,
        Modifier::Alt => 1,
        Modifier::Shift => 2,
        Modifier::Meta => 3,
    }
}

#[cfg(target_os = "macos")]
fn modifier_label(modifier: &Modifier) -> &'static str {
    match modifier {
        Modifier::Control => "Control",
        Modifier::Alt => "Option",
        Modifier::Shift => "Shift",
        Modifier::Meta => "Command",
    }
}

#[cfg(target_os = "windows")]
fn modifier_label(modifier: &Modifier) -> &'static str {
    match modifier {
        Modifier::Control => "Ctrl",
        Modifier::Alt => "Alt",
        Modifier::Shift => "Shift",
        Modifier::Meta => "Win",
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn modifier_label(modifier: &Modifier) -> &'static str {
    match modifier {
        Modifier::Control => "Ctrl",
        Modifier::Alt => "Alt",
        Modifier::Shift => "Shift",
        Modifier::Meta => "Super",
    }
}

fn code_label(code: &str) -> String {
    if is_letter(code) {
        return code[3..].to_owned();
    }
    if is_digit(code) {
        return code[5..].to_owned();
    }
    match code {
        "ArrowDown" => "Down".to_owned(),
        "ArrowLeft" => "Left".to_owned(),
        "ArrowRight" => "Right".to_owned(),
        "ArrowUp" => "Up".to_owned(),
        "Backquote" => "`".to_owned(),
        "Backslash" => "\\".to_owned(),
        "BracketLeft" => "[".to_owned(),
        "BracketRight" => "]".to_owned(),
        "Comma" => ",".to_owned(),
        "Equal" => "=".to_owned(),
        "Minus" => "-".to_owned(),
        "PageDown" => "Page Down".to_owned(),
        "PageUp" => "Page Up".to_owned(),
        "Period" => ".".to_owned(),
        "Quote" => "'".to_owned(),
        "Semicolon" => ";".to_owned(),
        "Slash" => "/".to_owned(),
        other => other.to_owned(),
    }
}

fn invalid(reason: &str) -> ApiError {
    ApiError::with_details(
        "SHORTCUT_INVALID",
        "Choose a supported key combination.",
        json!({ "reason": reason }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sided_modifiers_and_canonical_order() {
        assert_eq!(
            normalize_modifier_name("ControlRight"),
            Some(Modifier::Control)
        );
        assert_eq!(normalize_modifier_name("AltLeft"), Some(Modifier::Alt));
        assert_eq!(normalize_modifier_name("ShiftRight"), Some(Modifier::Shift));
        assert_eq!(normalize_modifier_name("MetaLeft"), Some(Modifier::Meta));

        let normalized = normalize_shortcut(Shortcut {
            modifiers: vec![
                Modifier::Shift,
                Modifier::Control,
                Modifier::Shift,
                Modifier::Alt,
            ],
            code: "KeyF".into(),
        })
        .unwrap();
        assert_eq!(
            normalized.modifiers,
            vec![Modifier::Control, Modifier::Alt, Modifier::Shift]
        );
    }

    #[test]
    fn validates_physical_codes_and_modifier_rules() {
        assert!(
            normalize_shortcut(Shortcut {
                modifiers: vec![],
                code: "F24".into()
            })
            .is_ok()
        );
        assert!(
            normalize_shortcut(Shortcut {
                modifiers: vec![],
                code: "KeyA".into()
            })
            .is_err()
        );
        assert!(
            normalize_shortcut(Shortcut {
                modifiers: vec![Modifier::Alt],
                code: "AudioVolumeUp".into()
            })
            .is_err()
        );
    }
}
