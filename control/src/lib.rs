//! Version 1 of the local automation protocol. No USB or capture implementation.
pub mod http;
pub mod session;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "command", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Color {
        value: String,
    },
    Routine {
        action: RoutineControl,
    },
    Status {},
    Power {
        state: Power,
        #[serde(default, rename = "allowCapture")]
        allow_capture: bool,
    },
    Brightness {
        value: u8,
    },
    Scenes {},
    Profiles {},
    ApplyScene {
        id: String,
        #[serde(default, rename = "allowCapture")]
        allow_capture: bool,
    },
    ApplyProfile {
        id: String,
        #[serde(default, rename = "allowCapture")]
        allow_capture: bool,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RoutineControl {
    Status,
    Start,
    Pause,
    Skip,
    Cancel,
    Cue,
    ResumeRules,
    PauseSchedules,
    ResumeSchedules,
}
impl RoutineControl {
    pub fn command(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Start => "start",
            Self::Pause => "pause",
            Self::Skip => "skip",
            Self::Cancel => "cancel",
            Self::Cue => "cue",
            Self::ResumeRules => "resumeRules",
            Self::PauseSchedules => "pauseSchedules",
            Self::ResumeSchedules => "resumeSchedules",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Power {
    On,
    Off,
    Toggle,
}

impl Command {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Color { value }
                if value.len() != 7
                    || !value.starts_with('#')
                    || !value[1..].bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                Err("Color must be #RRGGBB".into())
            }
            Self::Brightness { value } if *value > 100 => Err("Brightness must be 0–100".into()),
            Self::ApplyScene { id, .. } | Self::ApplyProfile { id, .. }
                if id.is_empty()
                    || id.len() > 64
                    || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
            {
                Err("Invalid scene/profile ID".into())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u8,
    pub command: Command,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("Unsupported API version; expected 1".into());
        }
        self.command.validate()
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub version: u8,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
impl Response {
    pub fn from_result(result: Result<Value, String>) -> Self {
        match result {
            Ok(data) => Self {
                version: VERSION,
                ok: true,
                data: Some(data),
                error: None,
            },
            Err(error) => Self {
                version: VERSION,
                ok: false,
                data: None,
                error: Some(error),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_versions_fields_commands_and_ranges() {
        assert!(Request {
            version: 2,
            command: Command::Status {}
        }
        .validate()
        .is_err());
        assert!(Command::Brightness { value: 101 }.validate().is_err());
        for json in [
            r#"{"version":1,"command":{"command":"shutdown"}}"#,
            r#"{"version":1,"command":{"command":"status","token":"x"}}"#,
            r#"{"version":1,"command":{"command":"brightness","value":-1}}"#,
        ] {
            assert!(serde_json::from_str::<Request>(json).is_err());
        }
        assert!(Command::ApplyScene {
            id: "../settings".into(),
            allow_capture: false
        }
        .validate()
        .is_err());
    }
}
