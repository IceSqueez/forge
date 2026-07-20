use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Variant;

pub type SubActionConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubActionStep {
    pub kind_id: String,
    pub config: SubActionConfig,
    pub enabled: bool,
    #[serde(default)]
    pub continue_on_error: bool,
    #[serde(default)]
    pub condition: Option<String>,
    pub label: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn sub_action_step_round_trips_across_field_shapes() {
        let mut config = BTreeMap::new();
        config.insert(
            "message".to_string(),
            Variant::String("Hello %user%!".to_string()),
        );
        let cases = [
            SubActionStep {
                kind_id: "twitch.chat.send_message".to_string(),
                config,
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: Some("greet".to_string()),
            },
            SubActionStep {
                kind_id: "core.logic.wait".to_string(),
                config: BTreeMap::new(),
                enabled: false,
                continue_on_error: true,
                condition: None,
                label: None,
            },
        ];
        for step in cases {
            let json = serde_json::to_string(&step).unwrap();
            let back: SubActionStep = serde_json::from_str(&json).unwrap();
            assert_eq!(step, back);
        }
    }

    #[test]
    fn legacy_json_without_continue_on_error_defaults_to_false() {
        // Why: actions persisted before the field existed must keep loading;
        // #[serde(default)] pins the value to false rather than failing the parse.
        let legacy = r#"{"kind_id":"core.log.write","config":{},"enabled":true,"label":null}"#;
        let step: SubActionStep = serde_json::from_str(legacy).unwrap();
        assert!(!step.continue_on_error);
    }
}
