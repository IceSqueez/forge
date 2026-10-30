pub mod backend;
mod binding;
mod binding_store;
pub mod client;
mod clip_bindings;
pub mod combo;
pub mod config;
pub(crate) mod content;
pub(crate) mod control;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
pub mod error;
pub(crate) mod health;
pub(crate) mod hold;
mod hold_ceiling;
pub mod integration;
mod main_thread;
pub mod quick_actions;
mod reconciler;
pub(crate) mod status;
pub(crate) mod supervisor;
mod synced_clips;
mod synced_triggers;
#[cfg(feature = "testing")]
pub mod testing;
pub mod triggers;
pub mod vocabulary;

#[cfg(target_os = "linux")]
pub(crate) mod backend_evdev;
#[cfg(target_os = "linux")]
pub(crate) mod backend_portal;
#[cfg(target_os = "linux")]
mod evdev_watch;
#[cfg(target_os = "linux")]
mod owned_fd_readiness;
#[cfg(target_os = "linux")]
mod portal_request;
#[cfg(target_os = "linux")]
mod portal_trigger;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod backend_global;

pub use backend::HotkeyId;
pub use binding::{
    BindingHalf, BindingRow, COMBO_FIELD, HotkeyEdge, combo_keys, conflict_count, registered_combos,
};
pub use binding_store::{
    BindingUpdate, bound_hotkey_combos, cleanup_stale_combo_instances, delete_binding,
    delete_binding_half, do_bind, load_bindings, persisted_hotkey_combos, rebind_combo,
    set_binding_enabled, update_binding,
};
pub use client::{EnableFailure, HotkeyClient};
pub use clip_bindings::{
    CLIP_COMBO_FIELD, ClipBindings, canonical_clip, clear_clip_combo, clip_bindings_of,
    stored_combo,
};
pub use combo::HotkeyCombo;
pub use config::{DEFAULT_HOLD_CEILING_SECS, HotkeyConfig};
pub use error::HotkeyError;
pub use hold_ceiling::{
    HOLD_CEILING, HOTKEY_HOLD_CEILING_KEY, SETTING_SPECS, load_hold_ceiling, save_hold_ceiling,
};
pub use integration::HOTKEY_INTEGRATION;
pub use main_thread::{
    MAIN_THREAD_TIMEOUT, MainThreadHost, MainThreadLink, main_thread_channel,
    main_thread_channel_with_timeout,
};
pub use reconciler::HotkeyReconciler;
pub use synced_clips::HotkeySyncedClipsRepo;
pub use synced_triggers::HotkeySyncedTriggerRepo;
pub use triggers::register_hotkey_triggers;
