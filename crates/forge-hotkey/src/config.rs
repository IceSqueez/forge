use serde::{Deserialize, Serialize};

pub const DEFAULT_HOLD_CEILING_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub app_name: String,
    /// `None` lets a hold stay open until a release edge arrives; `Some` closes it with a
    /// synthesized release once the ceiling is reached.
    pub hold_ceiling_secs: Option<u64>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            app_name: "forge".to_owned(),
            hold_ceiling_secs: Some(DEFAULT_HOLD_CEILING_SECS),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn config_serde_roundtrip() {
        let cfg = HotkeyConfig {
            app_name: "myapp".to_owned(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: HotkeyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.app_name, cfg.app_name);
    }
}
