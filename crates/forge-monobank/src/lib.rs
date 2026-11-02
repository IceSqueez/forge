mod account_setup;
mod api;
mod config;
mod content;
mod credentials;
mod currency;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
pub mod endpoint_surfaces;
mod error;
mod health;
mod integration;
mod jar;
mod jar_progress;
mod messages;
mod normalize;
mod poller;
mod provider;
mod status;
mod token;
mod triggers;
mod window;
mod wire;

pub use api::MonobankGateways;
pub use api::{MonobankRateLimits, monobank_gateways};
pub use config::{
    CALL_INTERVAL, DEFAULT_HISTORY_LOOKBACK, DEFAULT_WINDOW_OVERLAP, MAX_STATEMENT_WINDOW,
    MonobankConfig, MonobankTiming,
};
pub use credentials::MONOBANK_CREDENTIAL_ID;
pub use error::MonobankError;
pub use integration::MONOBANK_INTEGRATION;
pub use jar::{JarId, MonobankJar};
pub use jar_progress::{JAR_PROGRESS_KIND, JarProgressed};
pub use messages::MONOBANK_MESSAGES;
pub use provider::MonobankProvider;
pub use status::{PollFailure, PollStatus, PollerPhase};
pub use triggers::register_monobank_triggers;
pub use window::STATEMENT_PAGE_LIMIT;
