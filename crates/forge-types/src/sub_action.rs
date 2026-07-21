use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputDevice {
    Default,
    ByName { name: String },
    ById { id: String },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn output_device_variants_serde_roundtrip() {
        let variants = [
            OutputDevice::Default,
            OutputDevice::ByName {
                name: "Speakers".to_string(),
            },
            OutputDevice::ById {
                id: "alsa:hw:0,0".to_string(),
            },
        ];
        for dev in variants {
            let json = serde_json::to_string(&dev).unwrap();
            let back: OutputDevice = serde_json::from_str(&json).unwrap();
            assert_eq!(dev, back);
        }
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
            let json = serde_json::to_string(&level).unwrap();
            let back: LogLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(level, back);
        }
    }
}
