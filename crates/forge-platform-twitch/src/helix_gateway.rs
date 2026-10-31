use std::sync::Arc;
use std::time::Duration;

use forge_events::{EventPublisher, EventSource, RequestFailed, TypedEvent};
use forge_net::{
    CallClass, FailureContext, FailureObserver, Gateway, GatewayBuildError, GatewayFailure,
    GatewayProfile, RateLimiter, ThrottleHintFamily,
};
use forge_platform_core::{EndpointSurface, PlatformEndpoints};

const BODY_SNIPPET_MAX_CHARS: usize = 200;
const NO_RESET_HEADER_WAIT: Duration = Duration::from_secs(5);

pub(crate) const HELIX_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::ResetEpochSeconds,
    NO_RESET_HEADER_WAIT,
);

pub fn helix_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(EndpointSurface::TwitchApi),
        limiter,
        HELIX_GATEWAY_PROFILE,
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
        self.bus.publish(
            RequestFailed {
                endpoint: context.label.unwrap_or_default().to_owned(),
                status_code: status,
                body_snippet,
                retry_after_secs,
            }
            .into_event(EventSource::Twitch, None),
        );
    }
}
