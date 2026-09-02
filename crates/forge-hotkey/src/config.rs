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
mod tests {
    use super::*;

    #[test]
    fn the_hold_ceiling_ships_switched_on() {
        // Why: shipping the ceiling off by default leaves a swallowed key-up holding a
        // push-to-talk open forever. Flipping this default is a product decision, not a tweak.
        assert_eq!(
            HotkeyConfig::default().hold_ceiling_secs,
            Some(DEFAULT_HOLD_CEILING_SECS)
        );
        assert_ne!(DEFAULT_HOLD_CEILING_SECS, 0);
    }
}
