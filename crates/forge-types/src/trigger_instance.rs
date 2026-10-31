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
