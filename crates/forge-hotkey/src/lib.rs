mod backend;
mod bindings;
mod client;
mod combo;
mod config;
pub(crate) mod content;
pub(crate) mod control;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
mod error;
pub(crate) mod health;
pub(crate) mod hold;
mod hold_ceiling;
mod integration;
mod main_thread;
mod quick_actions;
mod registrations;
pub(crate) mod status;
pub(crate) mod supervisor;
#[cfg(feature = "testing")]
pub mod testing;
mod triggers;
mod view;
pub mod vocabulary;

pub use backend::{BackendTier, HotkeyId};
pub use bindings::clip::{
    CLIP_COMBO_FIELD, ClipBindings, ClipBindingsSource, ClipBindingsWatch, clear_clip_combo,
    clip_bindings_of, stored_combo,
};
pub use bindings::live::{conflict_count, registered_combos};
pub use bindings::reconciler::HotkeyReconciler;
pub use bindings::row::{BindingHalf, BindingRow, COMBO_FIELD, HotkeyEdge, combo_keys};
pub use bindings::store::{
    BindingUpdate, bound_hotkey_combos, cleanup_stale_combo_instances, delete_binding,
    delete_binding_half, do_bind, load_bindings, persisted_hotkey_combos, rebind_combo,
    set_binding_enabled, update_binding,
};
pub use bindings::synced_clips::HotkeySyncedClipsRepo;
pub use bindings::synced_triggers::HotkeySyncedTriggerRepo;
pub use client::EnableFailure;
pub use client::HotkeyClient;
pub use combo::HotkeyCombo;
pub use config::{DEFAULT_HOLD_CEILING_SECS, HotkeyConfig};
pub use error::HotkeyError;
pub use hold_ceiling::HoldCeilingCodec;
pub use hold_ceiling::{HOLD_CEILING, SETTING_SPECS, load_hold_ceiling, save_hold_ceiling};
pub use integration::HOTKEY_INTEGRATION;
pub use main_thread::MainThreadHost;
pub use main_thread::{MainThreadLink, main_thread_channel};
pub use triggers::register_hotkey_triggers;
