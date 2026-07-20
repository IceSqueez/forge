use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, QueueId};
use crate::sub_action_step::SubActionStep;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Sequential,
    RandomPick,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: ActionId,
    pub name: String,
    pub group: Option<String>,
    pub queue_id: QueueId,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    #[serde(default)]
    pub execution_mode: ExecutionMode,
    pub description: Option<String>,
    pub sub_actions: Vec<SubActionStep>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::sub_action_step::SubActionStep;
    use crate::variant::Variant;

    fn sample_action() -> Action {
        let mut send_config = BTreeMap::new();
        send_config.insert(
            "message".to_string(),
            Variant::String("Hello %user%!".to_string()),
        );
        send_config.insert("target".to_string(), Variant::String("twitch".to_string()));

        let mut log_config = BTreeMap::new();
        log_config.insert(
            "message".to_string(),
            Variant::String("greeted %user%".to_string()),
        );

        Action {
            id: ActionId::new(),
            name: "Greet".to_string(),
            group: Some("Chat".to_string()),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: Some("Sends a greeting".to_string()),
            sub_actions: vec![
                SubActionStep {
                    kind_id: "twitch.chat.send_message".to_string(),
                    config: send_config,
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                SubActionStep {
                    kind_id: "core.log.write".to_string(),
                    config: log_config,
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
            ],
        }
    }

    #[test]
    fn action_serde_roundtrip() {
        let a = sample_action();
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn action_no_group_serde_roundtrip() {
        let mut a = sample_action();
        a.group = None;
        a.description = None;
        a.sub_actions = vec![];
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
