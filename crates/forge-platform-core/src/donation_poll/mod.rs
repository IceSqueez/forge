mod control;
mod health;
mod status;
mod surface;
mod worker;

use std::time::Duration;

use forge_net::is_valid_header_value;
use forge_types::Secret;

pub use control::DonationPoll;
pub use health::{PollVocabulary, poll_metrics};
pub use status::{
    PollError, PollMetrics, PollPhaseCodes, PollStatus, PollStatusBoard, PollerPhase, RunningPoller,
};
pub use surface::PolledDonationProvider;
pub use worker::{CycleError, PollScope, PollWorker};

pub const POLL_ERROR_BACKOFF_BASE: Duration = Duration::from_secs(5);
pub const POLL_ERROR_BACKOFF_CAP: Duration = Duration::from_secs(300);

pub fn parse_header_token<E>(raw: &str, missing: E, malformed: E) -> Result<Secret, E> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(missing);
    }
    if !is_valid_header_value(trimmed) {
        return Err(malformed);
    }
    Ok(Secret::new(trimmed))
}
