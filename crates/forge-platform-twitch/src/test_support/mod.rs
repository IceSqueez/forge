pub(crate) mod bot_account;

use forge_events::test_support::NullPublisher;
use forge_storage::test_support::MemoryCredentials;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_net::{Gateway, RateLimiter, TokenBucketRateLimiter};
use forge_platform_core::{EndpointSurface, PlatformEndpoints};
use forge_registry::RunContext;

use forge_types::{ArgStack, EventId};

use crate::account::identity::SelfIdentity;
use crate::config::{HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW};
use crate::error::TwitchError;
use crate::helix::{HelixRequest, HelixTransport};
use crate::helix_gateway::HELIX_GATEWAY_PROFILE;
use crate::oauth_client::{
    TWITCH_OAUTH_BUDGET_CAPACITY, TWITCH_OAUTH_BUDGET_WINDOW, TwitchOAuthClient,
    twitch_oauth_gateway,
};

pub(crate) fn rejected_field(text: &forge_types::UserText) -> Option<&str> {
    let forge_types::UserText::Message(message) = text else {
        return None;
    };
    match message.arg("field") {
        Some(forge_types::MessageArg::StepFields { keys, .. }) if keys.len() == 1 => {
            Some(keys[0].as_str())
        }
        _ => None,
    }
}

pub(crate) fn runner_with<R>(
    responses: Vec<Result<serde_json::Value, TwitchError>>,
    build: impl FnOnce(Arc<dyn HelixTransport>, Arc<SelfIdentity>) -> R,
) -> (Arc<MockTransport>, R) {
    let transport = Arc::new(MockTransport::returning_sequence(responses));
    let runner = build(
        Arc::clone(&transport) as Arc<dyn HelixTransport>,
        self_identity(),
    );
    (transport, runner)
}

pub(crate) const TOKEN_SENTINEL: &str = "FAKE_TWITCH_TOKEN_SENTINEL_qq123";
pub(crate) const SELF_USER_ID: &str = "9876";

pub(crate) fn failed_message(outcome: &forge_types::SubActionOutcome) -> &forge_types::MessageRef {
    match outcome {
        forge_types::SubActionOutcome::Failed(forge_types::UserText::Message(message)) => message,
        other => panic!("expected a coded Failed outcome, got {other:?}"),
    }
}

pub(crate) fn assert_failed_with_code(outcome: &forge_types::SubActionOutcome, code: &str) {
    let message = failed_message(outcome);
    assert_eq!(message.code().to_string(), code);
    assert_no_secret_in(message);
}

pub(crate) fn assert_failed_with_detail(
    outcome: &forge_types::SubActionOutcome,
    code: &str,
    detail_fragment: &str,
) {
    let message = failed_message(outcome);
    assert_eq!(message.code().to_string(), code);
    let detail = message
        .detail()
        .expect("detail carries the upstream reason");
    assert!(detail.as_str().contains(detail_fragment), "{detail:?}");
}

pub(crate) fn assert_no_secret_in(message: &forge_types::MessageRef) {
    let wire = serde_json::to_string(message).unwrap();
    assert!(!wire.contains(TOKEN_SENTINEL), "token leaked: {wire}");
    assert!(!wire.contains("api.twitch.tv"), "URL leaked: {wire}");
}

pub(crate) fn assert_request_refused(outcome: &forge_types::SubActionOutcome, status: i64) {
    let message = failed_message(outcome);
    assert_eq!(message.code().to_string(), "twitch.request_refused");
    assert_eq!(
        message.arg("status"),
        Some(&forge_types::MessageArg::Int(status))
    );
    assert!(message.detail().is_none());
    assert_no_secret_in(message);
}

pub(crate) struct MockTransport {
    calls: Mutex<Vec<HelixRequest>>,
    responses: Mutex<VecDeque<Result<serde_json::Value, TwitchError>>>,
}

impl MockTransport {
    pub(crate) fn returning(response: Result<serde_json::Value, TwitchError>) -> Self {
        Self::returning_sequence(vec![response])
    }

    pub(crate) fn returning_sequence(
        responses: Vec<Result<serde_json::Value, TwitchError>>,
    ) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into()),
        }
    }

    pub(crate) fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub(crate) fn request(&self, index: usize) -> HelixRequest {
        self.calls.lock().unwrap()[index].clone()
    }

    pub(crate) fn last_request(&self) -> HelixRequest {
        self.calls.lock().unwrap().last().unwrap().clone()
    }
}

#[async_trait]
impl HelixTransport for MockTransport {
    async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, TwitchError> {
        self.calls.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(serde_json::Value::Null))
    }
}

pub(crate) fn users_fixture(id: &str) -> Result<serde_json::Value, TwitchError> {
    Ok(serde_json::json!({ "data": [{ "id": id, "login": "target" }] }))
}

pub(crate) struct MockCreds;

impl MockCreds {
    pub(crate) fn with_identity() -> MemoryCredentials {
        MemoryCredentials::holding(
            crate::credentials::TWITCH_CREDENTIAL_ID,
            &serde_json::json!({
                "access_token": TOKEN_SENTINEL,
                "user_id": SELF_USER_ID,
                "login": "streamer",
                "expires_at_unix": null,
            })
            .to_string(),
        )
    }

    pub(crate) fn empty() -> MemoryCredentials {
        MemoryCredentials::default()
    }
}

pub(crate) fn chat_trigger_args(message_id: &str) -> ArgStack {
    let event = Event::new(
        forge_events::EventSource::Twitch,
        crate::vocabulary::event_kinds::CHANNEL_CHAT_MESSAGE,
        serde_json::json!({
            (forge_types::ChatPayload::KEY): {
                "platform_msg_id": message_id,
                "author": "loyalfan",
                "author_color": null,
                "segments": [],
                "badges": [],
                "is_event": false,
                "event_detail": null,
            },
        }),
    );
    let mut registry = forge_registry::TriggerRegistry::new();
    crate::triggers::register_twitch_triggers(&mut registry).unwrap();
    registry
        .get(crate::vocabulary::trigger_ids::CHAT_MESSAGE)
        .unwrap()
        .build_arg_stack(&event)
}

pub(crate) fn self_broadcaster() -> crate::account::broadcaster::BroadcasterIdentity {
    crate::account::broadcaster::BroadcasterIdentity::new(SELF_USER_ID, "streamer").unwrap()
}

pub(crate) fn self_identity() -> std::sync::Arc<crate::account::identity::SelfIdentity> {
    identity_holding(Some(self_broadcaster())).1
}

pub(crate) fn signed_out_identity() -> std::sync::Arc<crate::account::identity::SelfIdentity> {
    identity_holding(None).1
}

pub(crate) fn identity_holding(
    broadcaster: Option<crate::account::broadcaster::BroadcasterIdentity>,
) -> (
    std::sync::Arc<crate::account::broadcaster::BroadcasterSlot>,
    std::sync::Arc<crate::account::identity::SelfIdentity>,
) {
    let slot = std::sync::Arc::new(crate::account::broadcaster::BroadcasterSlot::holding(
        broadcaster,
    ));
    let identity = std::sync::Arc::new(crate::account::identity::SelfIdentity::new(
        std::sync::Arc::clone(&slot),
    ));
    (slot, identity)
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
}

pub(crate) fn endpoints_with(overrides: &[(EndpointSurface, &str)]) -> PlatformEndpoints {
    PlatformEndpoints::resolve(crate::endpoint_surfaces::ENDPOINT_SURFACES, |variable| {
        overrides
            .iter()
            .find(|(surface, _)| surface.env_var() == variable)
            .map(|(_, url)| OsString::from(*url))
    })
    .unwrap()
}

pub(crate) fn helix_gateway_at(base_url: &str) -> Gateway {
    Gateway::new(
        base_url,
        std::sync::Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        )),
        HELIX_GATEWAY_PROFILE,
        None,
    )
    .unwrap()
}

pub(crate) const REQUEST_FAIL_KIND: &str = "request.fail";

pub(crate) fn observed_helix_at(
    base_url: &str,
) -> (
    std::sync::Arc<crate::helix::HelixHttpTransport>,
    std::sync::Arc<crate::test_support::bot_account::RecordingBus>,
) {
    let bus = std::sync::Arc::new(crate::test_support::bot_account::RecordingBus::default());
    let gateway = crate::helix_gateway::helix_gateway(
        &endpoints_with(&[(crate::endpoint_surfaces::TWITCH_API, base_url)]),
        std::sync::Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        )),
        std::sync::Arc::clone(&bus) as std::sync::Arc<dyn EventPublisher>,
    )
    .unwrap();
    let transport = crate::helix::HelixHttpTransport::new(
        gateway,
        "test-client".to_owned(),
        std::sync::Arc::new(crate::helix::FixedTokenSource::new(
            forge_types::OAuthToken::new(TOKEN_SENTINEL),
        )),
    );
    (std::sync::Arc::new(transport), bus)
}

pub(crate) fn unreachable_helix_gateway() -> Gateway {
    helix_gateway_at("http://127.0.0.1:0")
}

pub(crate) fn oauth_client_limited_by(
    base_url: &str,
    client_id: &str,
    limiter: std::sync::Arc<dyn RateLimiter>,
) -> TwitchOAuthClient {
    let endpoints = endpoints_with(&[(crate::endpoint_surfaces::TWITCH_OAUTH, base_url)]);
    TwitchOAuthClient::new(
        twitch_oauth_gateway(&endpoints, limiter).unwrap(),
        client_id.to_owned(),
    )
}

pub(crate) fn oauth_client_at(base_url: &str, client_id: &str) -> TwitchOAuthClient {
    oauth_client_limited_by(
        base_url,
        client_id,
        std::sync::Arc::new(TokenBucketRateLimiter::new(
            TWITCH_OAUTH_BUDGET_CAPACITY,
            TWITCH_OAUTH_BUDGET_WINDOW,
        )),
    )
}

pub(crate) fn unreachable_oauth_client(client_id: &str) -> TwitchOAuthClient {
    oauth_client_at("http://127.0.0.1:0", client_id)
}

pub(crate) fn unreachable_twitch_endpoints() -> PlatformEndpoints {
    endpoints_with(&[
        (crate::endpoint_surfaces::TWITCH_API, "http://127.0.0.1:0"),
        (
            crate::endpoint_surfaces::TWITCH_EVENTSUB_SOCKET,
            "ws://127.0.0.1:0",
        ),
    ])
}
