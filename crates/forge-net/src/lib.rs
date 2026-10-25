#![doc = "Outbound network building blocks: the HTTP gateway, rate limiting, throttle hints, capped body reads, backoff, dedup and the public-address guard."]

pub mod address_guard;
pub mod backoff;
pub mod body_read;
pub mod cooldown;
pub mod dedup;
pub mod gateway;
pub mod header_lookup;
pub mod header_value;
pub mod rate_limit;
pub mod status;
pub mod status_reason;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod throttle_hint;
pub use address_guard::{
    HostRejection, PublicAddressResolver, ensure_public_ip_literal, host_rejection_cause,
    is_loopback_host, is_private_or_special,
};
pub use backoff::Backoff;
pub use body_read::{
    BodyProgress, BodySink, BodySource, CappedReadError, StreamReadError, TruncatedBody,
    read_capped, read_streaming, read_truncated,
};
pub use cooldown::{Cooldown, MAX_REMOTE_COOLDOWN};
pub use dedup::DedupSet;
pub use gateway::{
    BodyExcerpt, CallClass, Credential, DecodeCategory, EgressKind, ExpectedRefusal,
    FailureContext, FailureObserver, FormBody, Gateway, GatewayBuildError, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, InvalidRequest, Method, MultipartBody,
    RedirectRefusal, RequestBody, RequestEncodeError, ResolvedTarget, ResponseDecodeError,
    STREAMING_CONNECT_BUDGET, STREAMING_READ_IDLE_BUDGET, Secret, StreamFailure, StreamedResponse,
    TransportFailure, TransportKind,
};
pub use header_lookup::HeaderLookup;
pub use header_value::is_valid_header_value;
pub use rate_limit::{
    DEFAULT_RETRY_AFTER_SECS, MAX_ACQUIRE_ATTEMPTS, MAX_THROTTLE_WAIT, RateLimitError,
    RateLimitOutcome, RateLimitUsage, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
pub use status_reason::canonical_status_reason;
pub use throttle_hint::{
    ThrottleHint, ThrottleHintFamily, ThrottleScope, parse_retry_after, parse_throttle_hint,
};
