use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource};
use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FailureContext, FailureObserver, Gateway,
    GatewayBuildError, GatewayFailure, GatewayProfile, GatewayRequest, RateLimiter, RequestBody,
    Secret, ThrottleHintFamily,
};
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
use serde::Serialize;

pub const KICK_API_BUDGET_CAPACITY: u32 = 60;
pub const KICK_API_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub const KICK_CHANNEL_INFO_BUDGET_CAPACITY: u32 = 1;
pub const KICK_CHANNEL_INFO_BUDGET_WINDOW: Duration = CHANNEL_INFO_REFRESH_INTERVAL;

pub(crate) const CHANNEL_INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);
const REQUEST_FAIL_KIND: &str = "request.fail";
const BODY_SNIPPET_MAX_CHARS: usize = 200;

pub const KICK_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn kick_api_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    kick_gateway(endpoints, EndpointSurface::KickPublicApi, limiter, bus)
}

pub fn kick_channel_info_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    kick_gateway(endpoints, EndpointSurface::KickChannelApi, limiter, bus)
}

fn kick_gateway(
    endpoints: &PlatformEndpoints,
    surface: EndpointSurface,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(surface),
        limiter,
        KICK_GATEWAY_PROFILE,
        Some(Arc::new(RequestFailPublisher { bus })),
    )
}

struct RequestFailPublisher {
    bus: Arc<dyn EventPublisher>,
}

impl FailureObserver for RequestFailPublisher {
    fn on_failure(&self, failure: &GatewayFailure, context: FailureContext) {
        let Some(status) = failure.status() else {
            return;
        };
        let retry_after_secs = match failure {
            GatewayFailure::RateLimited { retry_after, .. } => Some(retry_after.as_secs()),
            _ => None,
        };
        let body_snippet: String = failure
            .excerpt()
            .map(|excerpt| {
                excerpt
                    .as_str()
                    .chars()
                    .take(BODY_SNIPPET_MAX_CHARS)
                    .collect()
            })
            .unwrap_or_default();
        self.bus.publish(Event::new(
            EventSource::Kick,
            REQUEST_FAIL_KIND,
            serde_json::json!({
                "endpoint": context.label.unwrap_or_default(),
                "status_code": status,
                "body_snippet": body_snippet,
                "retry_after_secs": retry_after_secs,
            }),
        ));
    }
}

pub(crate) fn authorized(
    request: GatewayRequest,
    token: &str,
    label: &'static str,
) -> GatewayRequest {
    request.bearer(Secret::new(token)).label(label)
}

pub(crate) fn json_body<T: Serialize + ?Sized>(value: &T) -> Result<RequestBody, PlatformError> {
    RequestBody::json(value).map_err(|error| PlatformError::Network {
        reason: error.to_string(),
    })
}

pub(crate) fn malformed(error: impl std::fmt::Display) -> PlatformError {
    PlatformError::MalformedResponse {
        reason: error.to_string(),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RefusalReasons {
    pub(crate) unauthorized: &'static str,
    pub(crate) forbidden: Option<&'static str>,
}

pub(crate) fn platform_error(failure: GatewayFailure, reasons: RefusalReasons) -> PlatformError {
    match (failure, reasons.forbidden) {
        (GatewayFailure::Unauthorized { .. }, _) => PlatformError::Auth {
            reason: reasons.unauthorized.to_owned(),
        },
        (GatewayFailure::Forbidden { .. }, Some(forbidden)) => PlatformError::Auth {
            reason: forbidden.to_owned(),
        },
        (other, _) => gateway_error(other),
    }
}

pub(crate) fn gateway_error(failure: GatewayFailure) -> PlatformError {
    match failure {
        GatewayFailure::RateLimited { retry_after, .. } => PlatformError::RateLimited {
            retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
        },
        GatewayFailure::BudgetExhausted { .. } => PlatformError::RateLimitExhausted,
        other => match (other.status(), other.excerpt()) {
            (Some(status), Some(excerpt)) => PlatformError::Http {
                status,
                body: excerpt.as_str().to_owned(),
            },
            _ => PlatformError::Network {
                reason: other.to_string(),
            },
        },
    }
}
