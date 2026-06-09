use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub app_name: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            app_name: "forge".to_owned(),
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
