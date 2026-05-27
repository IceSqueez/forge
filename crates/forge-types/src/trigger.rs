use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ActionId, TriggerId};
use crate::variant::Variant;

pub type TriggerConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub id: TriggerId,
    pub action_id: ActionId,
    pub kind_id: String,
    pub config: TriggerConfig,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn cheer_trigger() -> Trigger {
        let mut config = BTreeMap::new();
        config.insert("min_bits".to_string(), Variant::Int(100));
        Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "twitch.support.cheer".to_string(),
            config,
        }
    }

    #[test]
    fn trigger_serde_roundtrip() {
        let t = cheer_trigger();
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn trigger_empty_config_serde_roundtrip() {
        let t = Trigger {
            id: TriggerId::new(),
            action_id: ActionId::new(),
            kind_id: "twitch.chat.message".to_string(),
            config: BTreeMap::new(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}
