use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource};
use forge_net::{Gateway, GatewayTimings, RateLimiter};
use forge_platform_core::{EndpointSurface, PlatformEndpoints};
use forge_storage::CredentialsRepo;
use forge_types::EventId;

use crate::auth::credentials_manager::KickCredentialsManager;
use crate::auth::oauth_client::{KickOAuthClient, kick_oauth_gateway};
use crate::gateway::{KICK_GATEWAY_PROFILE, kick_api_gateway, kick_channel_info_gateway};
use crate::pusher::chatroom_lookup::ChannelInfoFetcher;

const UNREACHABLE_HTTP: &str = "http://127.0.0.1:0";
const UNREACHABLE_WS: &str = "ws://127.0.0.1:0";
const WS_SURFACE_MARKER: &str = "_WS_";
pub(crate) const OAUTH_CLIENT_ID: &str = "kick-test-client-id";
pub(crate) const OAUTH_CLIENT_SECRET: &str = "kick-test-client-secret-7c1e";

pub(crate) fn unreachable_endpoints() -> PlatformEndpoints {
    endpoints_at(&[])
}

pub(crate) fn endpoints_at(overrides: &[(EndpointSurface, &str)]) -> PlatformEndpoints {
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        PlatformEndpoints::resolve(|variable| {
            EndpointSurface::ALL
                .into_iter()
                .find(|surface| surface.env_var() == variable)
                .map(|surface| {
                    let fallback = if variable.contains(WS_SURFACE_MARKER) {
                        UNREACHABLE_WS
                    } else {
                        UNREACHABLE_HTTP
                    };
                    let base = overrides
                        .iter()
                        .find(|(overridden, _)| *overridden == surface)
                        .map_or(fallback, |(_, base)| base);
                    std::ffi::OsString::from(base)
                })
        })
        .unwrap()
    })
}

pub(crate) fn api_with(
    base: &str,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> Gateway {
    kick_api_gateway(
        &endpoints_at(&[(EndpointSurface::KickPublicApi, base)]),
        limiter,
        bus,
    )
    .unwrap()
}

pub(crate) fn api_limited_by(base: &str, limiter: Arc<dyn RateLimiter>) -> Gateway {
    api_with(base, limiter, Arc::new(RecordingPublisher::default()))
}

pub(crate) fn gateway_waiting_at_most(
    base: &str,
    limiter: Arc<dyn RateLimiter>,
    max_throttle_wait: Duration,
) -> Gateway {
    Gateway::new(
        base,
        limiter,
        KICK_GATEWAY_PROFILE.with_timings(GatewayTimings {
            max_throttle_wait,
            ..GatewayTimings::DEFAULT
        }),
        None,
    )
    .unwrap()
}

pub(crate) fn api_at(base: &str) -> Gateway {
    api_limited_by(base, granting_limiter())
}

pub(crate) fn unreachable_api() -> Gateway {
    api_at(UNREACHABLE_HTTP)
}

pub(crate) fn channel_info_with(
    base: &str,
    limiter: Arc<dyn RateLimiter>,
    bus: Arc<dyn EventPublisher>,
) -> ChannelInfoFetcher {
    ChannelInfoFetcher::new(
        kick_channel_info_gateway(
            &endpoints_at(&[(EndpointSurface::KickChannelApi, base)]),
            limiter,
            bus,
        )
        .unwrap(),
    )
}

pub(crate) fn channel_info_at(base: &str) -> ChannelInfoFetcher {
    channel_info_with(
        base,
        granting_limiter(),
        Arc::new(RecordingPublisher::default()),
    )
}

pub(crate) fn unreachable_channel_info() -> Arc<ChannelInfoFetcher> {
    Arc::new(channel_info_at(UNREACHABLE_HTTP))
}

pub(crate) fn oauth_client_over(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
) -> KickOAuthClient {
    KickOAuthClient::new(
        endpoints,
        kick_oauth_gateway(endpoints, limiter).unwrap(),
        OAUTH_CLIENT_ID.to_owned(),
        forge_types::Secret::new(OAUTH_CLIENT_SECRET),
    )
}

pub(crate) fn oauth_client_limited_by(
    base: &str,
    limiter: Arc<dyn RateLimiter>,
) -> KickOAuthClient {
    oauth_client_over(
        &endpoints_at(&[(EndpointSurface::KickOAuth, base)]),
        limiter,
    )
}

pub(crate) fn oauth_client_at(base: &str) -> KickOAuthClient {
    oauth_client_limited_by(base, granting_limiter())
}

pub(crate) fn unreachable_oauth_client() -> KickOAuthClient {
    oauth_client_at(UNREACHABLE_HTTP)
}

pub(crate) fn offline_manager(repo: Arc<dyn CredentialsRepo>) -> KickCredentialsManager {
    KickCredentialsManager::new(repo, unreachable_oauth_client(), unreachable_api())
}

pub(crate) fn granting_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(forge_net::test_support::RecordingRateLimiter::granting())
}

pub(crate) fn exhausted_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(forge_net::test_support::RecordingRateLimiter::exhausted())
}

#[derive(Default)]
pub(crate) struct RecordingPublisher {
    events: Mutex<Vec<Event>>,
}

impl RecordingPublisher {
    pub(crate) fn published(
        &self,
    ) -> Vec<(EventSource, String, serde_json::Value, Option<EventId>)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.source, e.kind.clone(), e.payload.clone(), e.caused_by))
            .collect()
    }

    pub(crate) fn events_of_kind(&self, kind: &str) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}
