use serde::{Deserialize, Serialize};

/// Inline template string passed through `ArgInterpolator` before execution.
pub type VariantTemplate = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SubActionSpec {
    SendChat {
        message: VariantTemplate,
        target: String,
    },
    SetGlobal {
        name: String,
        value: VariantTemplate,
    },
    Delay {
        ms: u64,
    },
    Log {
        level: LogLevel,
        message: VariantTemplate,
    },
}

impl SubActionSpec {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::SendChat { .. } => "SendChat",
            Self::SetGlobal { .. } => "SetGlobal",
            Self::Delay { .. } => "Delay",
            Self::Log { .. } => "Log",
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn send_chat_serde_roundtrip() {
        let spec = SubActionSpec::SendChat {
            message: "Hello %user%!".to_string(),
            target: "twitch".to_string(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: SubActionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn set_global_serde_roundtrip() {
        let spec = SubActionSpec::SetGlobal {
            name: "counter".to_string(),
            value: "42".to_string(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: SubActionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn delay_serde_roundtrip() {
        let spec = SubActionSpec::Delay { ms: 1_000 };
        let json = serde_json::to_string(&spec).unwrap();
        let back: SubActionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn log_serde_roundtrip() {
        let spec = SubActionSpec::Log {
            level: LogLevel::Info,
            message: "action started".to_string(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: SubActionSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn all_log_levels_serde_roundtrip() {
        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            let spec = SubActionSpec::Log {
                level: level.clone(),
                message: String::new(),
            };
            let json = serde_json::to_string(&spec).unwrap();
            let back: SubActionSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(spec, back);
        }
    }
}
