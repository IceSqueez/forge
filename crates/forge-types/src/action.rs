use crate::ids::{ActionId, QueueId};
use crate::sub_action::SubActionSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: ActionId,
    pub name: String,
    pub group: Option<String>,
    pub queue_id: QueueId,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub description: Option<String>,
    pub sub_actions: Vec<SubActionSpec>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_action::{LogLevel, SubActionSpec};

    fn sample_action() -> Action {
        Action {
            id: ActionId::new(),
            name: "Greet".to_string(),
            group: Some("Chat".to_string()),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            description: Some("Sends a greeting".to_string()),
            sub_actions: vec![
                SubActionSpec::SendChat {
                    message: "Hello %user%!".to_string(),
                    target: "twitch".to_string(),
                },
                SubActionSpec::Log {
                    level: LogLevel::Info,
                    message: "greeted %user%".to_string(),
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
