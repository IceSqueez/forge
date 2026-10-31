#![doc = "Outbound network building blocks: the HTTP gateway, rate limiting, throttle hints, capped body reads, backoff, dedup and the public-address guard."]

mod address_guard;
mod backoff;
mod body_read;
mod cooldown;
mod dedup;
mod gateway;
mod header_lookup;
mod header_value;
mod rate_limit;
pub mod status;
mod status_reason;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
mod throttle_hint;
pub use address_guard::{HostRejection, ensure_public_ip_literal, is_loopback_host};
pub use backoff::{
    Backoff, DEFAULT_BASE as DEFAULT_BACKOFF_BASE, DEFAULT_CAP as DEFAULT_BACKOFF_CAP,
};
pub use body_read::{
    BodyProgress, BodySink, BodySource, CappedReadError, StreamReadError, TruncatedBody,
    read_capped, read_streaming, read_truncated,
};
pub use cooldown::{Cooldown, MAX_REMOTE_COOLDOWN};
pub use dedup::DedupSet;
pub use gateway::{
    BodyExcerpt, CallClass, Credential, DecodeCategory, ExpectedRefusal, FailureContext,
    FailureObserver, FormBody, Gateway, GatewayBuildError, GatewayFailure, GatewayProfile,
    GatewayRequest, GatewayResponse, GatewayTimings, InvalidRequest, Method, MultipartBody,
    RedirectRefusal, RequestBody, RequestEncodeError, ResolvedTarget, ResponseDecodeError,
    StreamFailure, StreamedResponse, TransportKind,
};
pub use gateway::{EgressKind, TransportFailure};
pub use header_lookup::HeaderLookup;
pub use header_value::is_valid_header_value;
pub use rate_limit::RateLimitUsage;
pub use rate_limit::{
    DEFAULT_RETRY_AFTER_SECS, RateLimitError, RateLimitOutcome, RateLimiter, TokenBucketRateLimiter,
};
pub use status_reason::{canonical_status_reason, status_class};
pub use throttle_hint::{
    ThrottleHint, ThrottleHintFamily, ThrottleScope, parse_retry_after, parse_throttle_hint,
};
