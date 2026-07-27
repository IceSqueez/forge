pub mod auth;
pub mod client;
pub mod content;
mod control;
pub mod credentials;
pub mod error;
pub mod events;
pub mod health;
mod payload_fields;
pub mod probe;
pub mod protocol;
pub mod quick_actions;
pub(crate) mod request;
pub mod runners;
pub mod sink;
mod sink_impl;
pub mod status;
pub(crate) mod supervisor;
pub mod switchable_sink;
pub mod triggers;

/// Must stay byte-identical to the string VTube Studio shows in its plugin-approval popup.
pub const PLUGIN_NAME: &str = "forge";

pub use auth::AuthState;
pub use client::{VTubeClient, VTubeConfig};
pub use credentials::{VTUBE_CREDENTIAL_ID, VTubeConnectError, VTubeCredentials};
pub use error::VTubeError;
pub use probe::{VTubeProbeResult, probe_connection};
pub use runners::register_vtube_sub_actions;
pub use sink::VTubeSink;
pub use switchable_sink::SwitchableVTubeSink;
pub use triggers::register_vtube_triggers;
