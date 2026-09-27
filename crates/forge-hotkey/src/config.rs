use serde::{Deserialize, Serialize};

pub const DEFAULT_HOLD_CEILING_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub app_name: String,
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
mod tests {
    use super::*;

    #[test]
    fn the_hold_ceiling_ships_switched_on() {
        assert_eq!(
            HotkeyConfig::default().hold_ceiling_secs,
            Some(DEFAULT_HOLD_CEILING_SECS)
        );
        assert_ne!(DEFAULT_HOLD_CEILING_SECS, 0);
    }
}
