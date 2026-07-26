//! Wraps `obws` behind owned traits; no `obws` type may cross the crate boundary.

pub mod catalog;
pub mod client;
pub mod credentials;
pub mod error;
mod events;
pub mod health;
mod payload_fields;
pub mod probe;
pub mod quick_actions;
pub mod runners;
pub mod sink;
mod sink_impl;
pub mod source;
mod source_impl;
pub mod switchable_sink;
pub mod triggers;

pub use client::ObsClient;
pub use error::ObsError;
pub use health::HealthSnapshot;
pub use probe::{ObsProbeResult, probe_connection};
pub use runners::register_obs_sub_actions;
pub use sink::ObsSink;
pub use source::{ObsSource, SourceInfo};
pub use switchable_sink::SwitchableObsSink;
pub use triggers::register_obs_triggers;

fn _assert_object_safe(_: &dyn ObsSink, _: &dyn ObsSource) {}
