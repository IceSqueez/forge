#![allow(clippy::unwrap_used, clippy::panic)]
mod support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use forge_platform_core::{ChatPlatform, QuickActionFieldValue, QuickActions};
use forge_platform_youtube::vocabulary::{config_keys, sub_action_ids};
use forge_platform_youtube::{YoutubeAssembly, YoutubeAuthBundle, YoutubeSession};
use forge_registry::{RunContext, SubActionConfig, SubActionRegistry};
use forge_storage::BanLedgerRepo;
use forge_storage::MockBanLedgerRepo;
use forge_types::{ArgStack, EventId, OAuthToken, Secret, SubActionOutcome, Variant};
use serde_json::{Value, json};
use support::{DATA_API_PREFIX, MemoryRepo, SilentBus, data_api_override};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const HANG_GUARD: Duration = Duration::from_secs(30);
const STATE_POLL: Duration = Duration::from_millis(5);
const SLOW_ANSWER: Duration = Duration::from_millis(300);
const ONE_HOUR: Duration = Duration::from_secs(3600);
const CHAT_POLL_MS: u64 = 3000;
const FIRST: LiveBroadcast = LiveBroadcast {
    id: "broadcast-first",
    chat: "chat-first",
};
const SECOND: LiveBroadcast = LiveBroadcast {
    id: "broadcast-second",
    chat: "chat-second",
};

#[derive(Clone, Copy)]
struct LiveBroadcast {
    id: &'static str,
    chat: &'static str,
}

#[derive(Clone)]
struct BroadcastScript(Arc<Mutex<(LiveBroadcast, Duration)>>);

impl BroadcastScript {
    fn serving(broadcast: LiveBroadcast, delay: Duration) -> Self {
        Self(Arc::new(Mutex::new((broadcast, delay))))
    }

    fn serve(&self, broadcast: LiveBroadcast) {
        *self.0.lock().unwrap() = (broadcast, Duration::ZERO);
    }
}

impl Respond for BroadcastScript {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let (broadcast, delay) = *self.0.lock().unwrap();
        ResponseTemplate::new(reqwest::StatusCode::OK)
            .set_delay(delay)
            .set_body_json(json!({
                "items": [{
                    "id": broadcast.id,
                    "snippet": { "liveChatId": broadcast.chat, "title": "Live" },
                }],
            }))
    }
}

fn data_api_path(route: &str) -> String {
    format!("{DATA_API_PREFIX}{route}")
}

async fn mount_data_api(server: &MockServer, broadcasts: BroadcastScript) {
    Mock::given(method("GET"))
        .and(path(data_api_path("/liveBroadcasts")))
        .respond_with(broadcasts)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(data_api_path("/liveChat/messages")))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({
                "pollingIntervalMillis": CHAT_POLL_MS,
                "items": [],
            })),
        )
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(data_api_path("/liveChat/messages")))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({ "kind": "sent" })),
        )
        .mount(server)
        .await;
}

fn signed_in_bundle() -> YoutubeAuthBundle {
    YoutubeAuthBundle {
        access_token: OAuthToken::new("access"),
        refresh_token: OAuthToken::new("refresh"),
        channel_id: "UCsession".to_owned(),
        channel_title: "Session".to_owned(),
        channel_handle: None,
        client_id: "client".to_owned(),
        expires_at: SystemTime::now() + ONE_HOUR,
    }
}

struct Assembled {
    registry: SubActionRegistry,
    session: YoutubeSession,
    ledger: Arc<dyn BanLedgerRepo>,
    _assembly: YoutubeAssembly,
}

async fn assembled_over(server: &MockServer) -> Assembled {
    let ledger: Arc<dyn BanLedgerRepo> = Arc::new(MockBanLedgerRepo::new());
    let assembly = YoutubeAssembly::new(
        &data_api_override(server),
        Arc::new(SilentBus),
        Arc::new(MemoryRepo::default()),
        Arc::clone(&ledger),
        ("client".to_owned(), Secret::new("secret")),
    )
    .unwrap();
    assembly
        .credentials_manager()
        .save_from_bundle(signed_in_bundle())
        .await
        .unwrap();
    let mut registry = SubActionRegistry::new();
    assembly.register_sub_actions(&mut registry);
    let session = assembly.start().await.unwrap().unwrap();
    Assembled {
        registry,
        session,
        ledger,
        _assembly: assembly,
    }
}

async fn settle_until(what: &str, mut done: impl FnMut() -> bool) {
    let settled = tokio::time::timeout(HANG_GUARD, async {
        while !done() {
            tokio::time::sleep(STATE_POLL).await;
        }
    })
    .await;
    assert!(settled.is_ok(), "never happened: {what}");
}

async fn resolved_broadcast(fixture: &Assembled, broadcast: LiveBroadcast) {
    let handle = fixture.session.platform.active_broadcast_id();
    settle_until("the poller published the broadcast", || {
        handle.get().as_deref() == Some(broadcast.id)
    })
    .await;
}

async fn send_message(registry: &SubActionRegistry) -> SubActionOutcome {
    let runner = registry.get(sub_action_ids::CHAT_SEND_MESSAGE).unwrap();
    let config: SubActionConfig = BTreeMap::from([(
        config_keys::MESSAGE.to_owned(),
        Variant::String("hello".to_owned()),
    )]);
    let stack = ArgStack::new();
    let context = RunContext::leaf(&stack, 0, EventId::new(), &SilentBus);
    runner.execute(&config, &context).await.0.outcome
}

async fn chats_sent_to(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.method.as_str() == "POST")
        .map(|request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            body["snippet"]["liveChatId"].as_str().unwrap().to_owned()
        })
        .collect()
}

async fn broadcast_fetches(server: &MockServer) -> usize {
    let route = data_api_path("/liveBroadcasts");
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

#[tokio::test]
async fn a_sub_action_writes_to_whichever_broadcast_the_poller_resolved_last() {
    let server = MockServer::start().await;
    let broadcasts = BroadcastScript::serving(FIRST, Duration::ZERO);
    mount_data_api(&server, broadcasts.clone()).await;
    let fixture = assembled_over(&server).await;
    let platform = &fixture.session.platform;

    platform.connect().await.unwrap();
    resolved_broadcast(&fixture, FIRST).await;
    let before_resolution = send_message(&fixture.registry).await;

    broadcasts.serve(SECOND);
    platform.disconnect().await.unwrap();
    platform.connect().await.unwrap();
    resolved_broadcast(&fixture, SECOND).await;
    let after_resolution = send_message(&fixture.registry).await;
    platform.disconnect().await.unwrap();

    assert_eq!(before_resolution, SubActionOutcome::Success);
    assert_eq!(after_resolution, SubActionOutcome::Success);
    assert_eq!(
        chats_sent_to(&server).await,
        [FIRST.chat.to_owned(), SECOND.chat.to_owned()]
    );
}

#[tokio::test]
async fn a_sub_action_finds_the_broadcast_itself_while_chat_polling_is_down() {
    let server = MockServer::start().await;
    mount_data_api(&server, BroadcastScript::serving(FIRST, Duration::ZERO)).await;
    let fixture = assembled_over(&server).await;

    let outcome = send_message(&fixture.registry).await;

    assert_eq!(outcome, SubActionOutcome::Success);
    assert_eq!(chats_sent_to(&server).await, [FIRST.chat.to_owned()]);
}

#[tokio::test]
async fn a_fetch_pending_across_a_reconnect_never_overwrites_the_new_session() {
    let server = MockServer::start().await;
    let broadcasts = BroadcastScript::serving(FIRST, SLOW_ANSWER);
    mount_data_api(&server, broadcasts.clone()).await;
    let fixture = assembled_over(&server).await;
    let platform = &fixture.session.platform;

    platform.connect().await.unwrap();
    let holders_of_one_session = Arc::strong_count(&fixture.ledger);
    tokio::time::timeout(HANG_GUARD, async {
        while broadcast_fetches(&server).await == 0 {
            tokio::time::sleep(STATE_POLL).await;
        }
    })
    .await
    .expect("the first session never asked for the broadcast");

    broadcasts.serve(SECOND);
    platform.disconnect().await.unwrap();
    platform.connect().await.unwrap();
    resolved_broadcast(&fixture, SECOND).await;
    settle_until("the retired poller finished", || {
        Arc::strong_count(&fixture.ledger) == holders_of_one_session
    })
    .await;

    assert_eq!(
        platform.active_broadcast_id().get().as_deref(),
        Some(SECOND.id)
    );
    platform.disconnect().await.unwrap();
}

fn filled_values(
    action: &forge_platform_core::QuickAction,
) -> BTreeMap<String, QuickActionFieldValue> {
    action
        .fields
        .iter()
        .filter(|field| field.default.is_none())
        .map(|field| {
            let sample = match field.key.as_str() {
                "options" => "Pizza\nSushi",
                "channel_id" => "UC1",
                _ => "supplied",
            };
            (
                field.key.clone(),
                QuickActionFieldValue::Text(sample.to_owned()),
            )
        })
        .collect()
}

#[tokio::test]
async fn every_quick_action_of_a_session_runs_on_a_runner_the_assembly_registered() {
    let server = MockServer::start().await;
    let fixture = assembled_over(&server).await;

    for action in QuickActions::actions(fixture.session.bundle.as_ref()) {
        let step = action.merge_config(&filled_values(&action));
        let runner = fixture
            .registry
            .get(&step.kind_id)
            .unwrap_or_else(|| panic!("no runner for quick action {:?}", action.label));

        let verdict = runner.validate_config(&step.config);

        assert!(
            verdict.is_ok(),
            "{:?} merges a config its runner rejects: {verdict:?}",
            action.label
        );
    }
}
