use super::keys::{ACCENT, ACCENT_SPLIT_KEYS};
use crate::catalog::descriptor::OverlayConfig;

pub fn with_split_accent(stored: &OverlayConfig) -> OverlayConfig {
    let mut split = stored.clone();
    if let Some(accent) = split.remove(ACCENT) {
        for key in ACCENT_SPLIT_KEYS {
            split
                .entry(key.to_owned())
                .or_insert_with(|| accent.clone());
        }
    }
    split
}
