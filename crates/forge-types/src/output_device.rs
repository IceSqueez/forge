use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputDevice {
    Default,
    ByName { name: String },
    ById { id: String },
}

#[cfg(test)]
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
}
