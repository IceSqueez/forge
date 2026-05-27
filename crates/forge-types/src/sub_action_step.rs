use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Variant;

pub type SubActionConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubActionStep {
    pub kind_id: String,
    pub config: SubActionConfig,
    pub enabled: bool,
    pub label: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sub_action_step_serde_roundtrip() {
        let mut config = BTreeMap::new();
        config.insert(
            "message".to_string(),
            Variant::String("Hello %user%!".to_string()),
        );
        let step = SubActionStep {
            kind_id: "twitch.chat.send_message".to_string(),
            config,
            enabled: true,
            label: Some("greet".to_string()),
        };
        let json = serde_json::to_string(&step).unwrap();
        let back: SubActionStep = serde_json::from_str(&json).unwrap();
        assert_eq!(step, back);
    }

    #[test]
    fn sub_action_step_disabled_no_label_serde_roundtrip() {
        let step = SubActionStep {
            kind_id: "core.logic.wait".to_string(),
            config: BTreeMap::new(),
            enabled: false,
            label: None,
        };
        let json = serde_json::to_string(&step).unwrap();
        let back: SubActionStep = serde_json::from_str(&json).unwrap();
        assert_eq!(step, back);
    }
}
