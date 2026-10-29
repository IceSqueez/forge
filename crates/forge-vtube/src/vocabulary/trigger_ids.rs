use super::event_kinds;

pub const MODEL_LOADED: &str = event_kinds::MODEL_LOADED;
pub const MODEL_UNLOADED: &str = event_kinds::MODEL_UNLOADED;
pub const MODEL_CONFIG_CHANGED: &str = event_kinds::MODEL_CONFIG_CHANGED;
pub const HOTKEY_TRIGGERED: &str = event_kinds::HOTKEY_TRIGGERED;
pub const EXPRESSION_STATE_CHANGED: &str = event_kinds::EXPRESSION_STATE_CHANGED;
pub const TRACKING_FACE_FOUND: &str = event_kinds::TRACKING_FACE_FOUND;
pub const TRACKING_FACE_LOST: &str = event_kinds::TRACKING_FACE_LOST;
pub const ITEM_ADDED: &str = event_kinds::ITEM_ADDED;
pub const ITEM_REMOVED: &str = event_kinds::ITEM_REMOVED;
