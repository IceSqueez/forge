use serde::{Deserialize, Serialize};

use crate::ids::TriggerInstanceId;
use crate::permission_rung::PermissionRung;
use crate::platform_scope::PlatformScope;
use crate::trigger_config::TriggerConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerInstance {
    pub id: TriggerInstanceId,
    pub kind_id: String,
    pub name: String,
    pub overrides: TriggerConfig,
    pub enabled: bool,
    pub user_defined: bool,
    pub platform_scope: PlatformScope,
    #[serde(default)]
    pub cooldown_secs: u32,
    #[serde(default = "default_cooldown_global")]
    pub cooldown_global: bool,
    #[serde(default)]
    pub permission_rung: PermissionRung,
}

fn default_cooldown_global() -> bool {
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use crate::variant::Variant;

    use super::*;

    #[test]
    fn serde_roundtrip_default_instance() {
        let instance = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "twitch.chat.message".to_string(),
            name: "Chat Message".to_string(),
            overrides: BTreeMap::new(),
            enabled: true,
            user_defined: false,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        };
        let json = serde_json::to_string(&instance).unwrap();
        let back: TriggerInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(instance, back);
    }

    #[test]
    fn serde_roundtrip_custom_instance() {
        let mut overrides = BTreeMap::new();
        overrides.insert("min_bits".to_string(), Variant::Int(500));
        overrides.insert(
            "reward_title".to_string(),
            Variant::String("VIP".to_string()),
        );
        let instance = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: "twitch.support.cheer".to_string(),
            name: "High Cheer".to_string(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        };
        let json = serde_json::to_string(&instance).unwrap();
        let back: TriggerInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(instance, back);
    }

    #[test]
    fn trigger_instance_id_new_is_unique() {
        let a = TriggerInstanceId::new();
        let b = TriggerInstanceId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn trigger_instance_id_display_fromstr_roundtrip() {
        let id = TriggerInstanceId::new();
        let s = id.to_string();
        assert_eq!(s.len(), 26);
        let back = TriggerInstanceId::from_str(&s).unwrap();
        assert_eq!(id, back);
    }
}
