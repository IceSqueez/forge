pub(crate) mod config_keys;
pub mod event_kinds;
mod events;
pub(crate) mod fields;
pub(crate) mod option_lists;
pub mod picker_keys;
pub mod sub_action_ids;
pub mod trigger_ids;
pub(crate) mod variables;

pub use events::VTubeConnectionChanged;

pub const INTEGRATION_ID: &str = "vtube";
