pub(crate) mod config_keys;
pub mod event_kinds;
pub(crate) mod fields;
pub(crate) mod limits;
mod out_message_kind;
mod signal_kind;
pub mod sub_action_ids;
pub mod trigger_ids;
pub(crate) mod variables;

pub(crate) use out_message_kind::OutMessageKind;
pub use signal_kind::MidiSignalKind;

pub const INTEGRATION_ID: &str = "midi";
