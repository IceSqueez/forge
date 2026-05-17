use crate::ids::{ActionId, TriggerId};
use crate::variant::Variant;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerKind {
    TwitchChatCommand,
    TwitchChatAnyMessage,
    TwitchSubscribe,
    TwitchResubscribe,
    TwitchGiftSub,
    TwitchCheer,
    TwitchRaid,
}

/// Kind-specific trigger parameters (cooldown_secs, perm, min_bits, etc.).
pub type TriggerConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub id: TriggerId,
    pub action_id: ActionId,
    pub kind: TriggerKind,
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
            kind: TriggerKind::TwitchCheer,
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
            kind: TriggerKind::TwitchChatAnyMessage,
            config: BTreeMap::new(),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Trigger = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn all_trigger_kinds_serde_roundtrip() {
        let kinds = [
            TriggerKind::TwitchChatCommand,
            TriggerKind::TwitchChatAnyMessage,
            TriggerKind::TwitchSubscribe,
            TriggerKind::TwitchResubscribe,
            TriggerKind::TwitchGiftSub,
            TriggerKind::TwitchCheer,
            TriggerKind::TwitchRaid,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: TriggerKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }
}
