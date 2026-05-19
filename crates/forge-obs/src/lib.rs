//! OBS Studio integration for forge. Wraps `obws` behind owned traits per the External Isolation
//! rule (CLAUDE.md §1): no `obws` type crosses a crate boundary; callers depend solely on
//! `ObsSink` and `ObsSource`.

pub mod catalog;
pub mod client;
pub mod error;
pub mod health;
pub mod quick_actions;
pub mod sink;
mod sink_impl;
pub mod source;

pub use client::ObsClient;
pub use error::ObsError;
pub use sink::ObsSink;
pub use source::{ObsSource, SourceInfo};

fn _assert_object_safe(_: &dyn ObsSink, _: &dyn ObsSource) {}
