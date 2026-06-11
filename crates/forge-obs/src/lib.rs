//! OBS Studio integration for forge. Wraps `obws` behind owned traits per the External Isolation
//! rule (CLAUDE.md §1): no `obws` type crosses a crate boundary; callers depend solely on
//! `ObsSink` and `ObsSource`.

pub mod catalog;
pub mod client;
pub mod credentials;
pub mod error;
mod events;
pub mod health;
pub mod quick_actions;
pub mod runners;
pub mod sink;
mod sink_impl;
pub mod source;
mod source_impl;
pub mod switchable_sink;
pub mod test_connect;
pub mod triggers;

pub use client::ObsClient;
pub use error::ObsError;
pub use health::HealthSnapshot;
pub use runners::register_obs_sub_actions;
pub use sink::ObsSink;
pub use source::{ObsSource, SourceInfo};
pub use switchable_sink::SwitchableObsSink;
pub use test_connect::{ObsServerInfo, test_connect};
pub use triggers::register_obs_triggers;

fn _assert_object_safe(_: &dyn ObsSink, _: &dyn ObsSource) {}
