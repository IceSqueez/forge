use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyPressed {
    pub combo: String,
    pub id: u32,
    pub timestamp_us: u64,
}

impl TypedEvent for HotkeyPressed {
    const KIND: &'static str = event_kinds::GLOBAL_PRESSED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyEnableFailed {
    pub combos: Vec<String>,
}

impl TypedEvent for HotkeyEnableFailed {
    const KIND: &'static str = event_kinds::ENGINE_ENABLE_FAILED;
}
