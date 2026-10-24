#![doc = "Outbound network building blocks: rate limiting, backoff, dedup and the public-address guard."]

pub mod address_guard;
pub mod backoff;
pub mod dedup;
pub mod rate_limit;
pub use address_guard::{
    HostRejection, PublicAddressResolver, ensure_public_ip_literal, host_rejection_cause,
    is_loopback_host, is_private_or_special,
};
pub use backoff::Backoff;
pub use dedup::DedupSet;
pub use rate_limit::{
    DEFAULT_RETRY_AFTER_SECS, MAX_ACQUIRE_ATTEMPTS, MAX_THROTTLE_WAIT, RateLimitError,
    RateLimitOutcome, RateLimitUsage, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
