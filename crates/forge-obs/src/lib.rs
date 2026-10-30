mod catalog;
mod client;
mod connect_failure;
pub mod credentials;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
mod error;
mod events;
mod health;
mod integration;
mod item_ids;
mod option_source;
mod probe;
mod quick_actions;
mod runners;
mod session;
mod sink;
mod sink_impl;
mod source;
mod source_impl;
mod stream_output;
mod switchable_sink;
mod triggers;
pub mod vocabulary;

pub use client::{ObsClient, parse_endpoint};
pub use error::ObsError;
pub use integration::OBS_INTEGRATION;
pub use probe::{ObsProbeResult, probe_connection};
pub use runners::register_obs_sub_actions;
pub use sink::ObsSink;
pub use source::{ObsSource, SourceInfo};
pub use stream_output::StreamOutputActive;
pub use switchable_sink::SwitchableObsSink;
pub use triggers::register_obs_triggers;

fn _assert_object_safe(_: &dyn ObsSink, _: &dyn ObsSource) {}
