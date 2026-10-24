#![doc = "Outbound network building blocks: rate limiting, backoff, dedup and the public-address guard."]

pub mod address_guard;
pub mod backoff;
pub mod body_read;
pub mod cooldown;
pub mod dedup;
pub mod header_lookup;
pub mod rate_limit;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod throttle_hint;
pub use address_guard::{
    HostRejection, PublicAddressResolver, ensure_public_ip_literal, host_rejection_cause,
    is_loopback_host, is_private_or_special,
};
pub use backoff::Backoff;
pub use body_read::{
    BodyProgress, BodySink, BodySource, CappedReadError, StreamReadError, read_capped,
    read_streaming,
};
pub use cooldown::{Cooldown, MAX_REMOTE_COOLDOWN};
pub use dedup::DedupSet;
pub use header_lookup::HeaderLookup;
pub use rate_limit::{
    DEFAULT_RETRY_AFTER_SECS, MAX_ACQUIRE_ATTEMPTS, MAX_THROTTLE_WAIT, RateLimitError,
    RateLimitOutcome, RateLimitUsage, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
pub use throttle_hint::{
    ThrottleHint, ThrottleHintFamily, ThrottleScope, parse_retry_after, parse_throttle_hint,
};
