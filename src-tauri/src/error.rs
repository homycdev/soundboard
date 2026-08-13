use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn internal() -> Self {
        Self::with_details(
            "INTERNAL",
            "An unexpected internal error occurred.",
            json!({ "correlationId": Uuid::new_v4().to_string() }),
        )
    }

    pub fn persistence() -> Self {
        Self::new(
            "PERSISTENCE_FAILED",
            "The change could not be saved. Your previous soundboard was kept.",
        )
    }

    pub fn audio_device() -> Self {
        Self::new(
            "AUDIO_DEVICE_UNAVAILABLE",
            "The default audio output device is unavailable.",
        )
    }

    pub fn decode() -> Self {
        Self::new(
            "AUDIO_DECODE_FAILED",
            "The selected audio file could not be decoded.",
        )
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}
