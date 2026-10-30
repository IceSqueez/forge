pub mod event_kinds;
mod events;
pub mod fields;
pub mod keys;
pub mod messages;
pub mod trigger_ids;
pub(crate) mod variables;

pub use events::{HotkeyEnableFailed, HotkeyPressed};

pub const INTEGRATION_ID: &str = "hotkey";
