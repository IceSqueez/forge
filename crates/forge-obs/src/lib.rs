//! OBS Studio integration for forge. Wraps `obws` behind owned traits per the External Isolation
//! rule (CLAUDE.md §1): no `obws` type crosses a crate boundary; callers depend solely on
//! `ObsSink` and `ObsSource`.

pub mod error;

pub use error::ObsError;

pub struct ObsClient;
