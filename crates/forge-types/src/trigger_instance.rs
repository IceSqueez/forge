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

impl TriggerInstance {
    pub fn new_user_defined(
        kind_id: impl Into<String>,
        name: impl Into<String>,
        overrides: TriggerConfig,
    ) -> Self {
        Self {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.into(),
            name: name.into(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: default_cooldown_global(),
            permission_rung: PermissionRung::default(),
        }
    }
}

fn default_cooldown_global() -> bool {
    true
}
