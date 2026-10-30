mod auth;
mod catalog;
mod client;
mod connect_attempt;
mod content;
mod control;
pub mod credentials;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
mod error;
mod events;
mod health;
mod integration;
mod option_source;
mod probe;
mod quick_actions;
mod runners;
mod sink;
mod sink_impl;
mod status;
pub(crate) mod supervisor;
mod switchable_sink;
mod triggers;
pub mod vocabulary;
mod vts;

pub const PLUGIN_NAME: &str = "Forge";

pub use auth::AuthState;
pub use catalog::{VTubeCatalog, VTubeCatalogChanges, VTubeChoice};
pub use client::{VTubeClient, VTubeConfig};
pub use connect_attempt::{VTubeAttemptPhase, VTubeConnectAttempt};
pub use credentials::{VTUBE_CREDENTIAL_ID, VTubeConnectError, VTubeCredentials};
pub use error::VTubeError;
pub use integration::VTUBE_INTEGRATION;
pub use probe::{VTubeProbeResult, probe_connection};
pub use runners::register_vtube_sub_actions;
pub use sink::VTubeSink;
pub use switchable_sink::SwitchableVTubeSink;
pub use triggers::register_vtube_triggers;
