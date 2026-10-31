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
mod timing;
mod triggers;
pub mod vocabulary;

pub use client::{EVENT_BUFFER_CAPACITY, ObsClient, parse_endpoint};
pub use error::ObsError;
pub use integration::OBS_INTEGRATION;
pub use probe::probe_connection;
pub use runners::register_obs_sub_actions;
pub use sink::{InputSettings, ObsSink, RecordStatus, SceneList, StreamStatus};
pub use source::{ObsSource, SourceInfo};
pub use stream_output::{ObsStatsWatch, ObsStreamStats, StreamOutputActive};
pub use switchable_sink::SwitchableObsSink;
pub use timing::ObsTiming;
pub use triggers::register_obs_triggers;

fn _assert_object_safe(_: &dyn ObsSink, _: &dyn ObsSource) {}
pub use probe::ObsProbeResult;
