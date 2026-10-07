#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_emulator::fixture::TwitchAccount;
use forge_emulator::twitch::{CredentialCheck, FakeTwitch, FakeTwitchConfig, Viewer, ViewerBadge};
use forge_events::{Event, EventPublisher, EventStream};
use forge_platform_core::{
    BuiltinControl, CONNECTION_STATE_CHANGED_KIND, ChatPlatform, ConnectionState, EndpointSurface,
    PlatformEndpoints, RateLimiter, TokenBucketRateLimiter,
};
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{
    ChatSessionConfig, CredentialsTokenSource, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW,
    HelixHttpTransport, SubscriptionTracker, TwitchChat, TwitchChatHandle,
    TwitchCredentialsManager, TwitchIntegrationBundle, TwitchLifecycle, TwitchPlatform, send_chat,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::{ChatPayload, OAuthToken, PermissionRung};
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(10);
const CHAT_MESSAGE_KIND: &str = "twitch.channel.chat.message";
const CHAT_SUBSCRIPTION: &str = "channel.chat.message";
const FOLLOW_SUBSCRIPTION: &str = "channel.follow";
const FOLLOW_KIND: &str = "twitch.channel.follow";
const STATE_POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
struct MemoryCredentials(Mutex<HashMap<String, String>>);

#[async_trait]
impl CredentialsRepo for MemoryCredentials {
    async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

struct Bus(mpsc::UnboundedSender<Event>);

impl EventPublisher for Bus {
    fn publish(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

struct Forge {
    creds: Arc<dyn CredentialsRepo>,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    events: mpsc::UnboundedReceiver<Event>,
}

impl Forge {
    async fn against(fake: &FakeTwitch, account: &TwitchAccount, access_token: &str) -> Self {
        let creds: Arc<dyn CredentialsRepo> = Arc::new(MemoryCredentials::default());
        store_credential(
            creds.as_ref(),
            &StoredCredential {
                access_token: OAuthToken::new(access_token.to_owned()),
                refresh_token: None,
                user_id: account.user_id.clone(),
                login: account.login.clone(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
            },
        )
        .await
        .unwrap();
        let overrides: HashMap<_, _> = fake.endpoint_overrides().into_iter().collect();
        let endpoints =
            PlatformEndpoints::resolve(|variable| overrides.get(variable).map(OsString::from))
                .unwrap();
        let (tx, events) = mpsc::unbounded_channel();
        Self {
            creds,
            config: ChatSessionConfig {
                client_id: account.client_id.clone(),
                broadcaster_id: account.user_id.clone(),
                user_id: account.user_id.clone(),
                endpoints,
            },
            bus: Arc::new(Bus(tx)),
            events,
        }
    }

    fn start_chat(
        &self,
        lifecycle: &TwitchLifecycle,
        tracker: &SubscriptionTracker,
    ) -> TwitchChatHandle {
        let manager = Arc::new(TwitchCredentialsManager::new(
            &self.config.endpoints,
            Arc::clone(&self.creds),
            self.config.client_id.clone(),
        ));
        TwitchChat::new(
            manager,
            self.config.clone(),
            Arc::clone(&self.bus),
            tracker.clone(),
            lifecycle.clone(),
        )
        .start()
    }

    fn platform(&self) -> Arc<TwitchPlatform> {
        let manager = Arc::new(TwitchCredentialsManager::new(
            &self.config.endpoints,
            Arc::clone(&self.creds),
            self.config.client_id.clone(),
        ));
        Arc::new(TwitchPlatform::new(
            self.config.clone(),
            Arc::clone(&self.creds),
            manager,
            SubscriptionTracker::default(),
            Self::rate_limiter(),
            TwitchLifecycle::new(),
        ))
    }

    fn rate_limiter() -> Arc<dyn RateLimiter> {
        Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        ))
    }

    async fn next_event_of(&mut self, kind: &str) -> Event {
        timeout(DEADLINE, async {
            loop {
                let event = self.events.recv().await.expect("bus open");
                if event.kind == kind {
                    return event;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("forge published no `{kind}` in time"))
    }
}

async fn chat_subscription_recorded(fake: &FakeTwitch) {
    fake.wait_for("forge's chat subscription", DEADLINE, |ledger| {
        ledger
            .subscriptions
            .iter()
            .any(|s| s.subscription_type == CHAT_SUBSCRIPTION)
            .then_some(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn fake_urls_pass_forge_endpoint_override_validation() {
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&TwitchAccount::default()))
        .await
        .unwrap();
    let overrides: HashMap<_, _> = fake.endpoint_overrides().into_iter().collect();

    let endpoints =
        PlatformEndpoints::resolve(|variable| overrides.get(variable).map(OsString::from)).unwrap();

    assert_eq!(
        endpoints.base_url(EndpointSurface::TwitchApi),
        fake.api_base_url()
    );
    assert_eq!(
        endpoints.base_url(EndpointSurface::TwitchEventSubSocket),
        fake.eventsub_ws_url()
    );
}

#[tokio::test]
async fn forge_chat_session_decodes_injected_messages_into_each_permission_rung() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let mut forge = Forge::against(&fake, &account, &account.access_token).await;
    let _chat = forge.start_chat(&TwitchLifecycle::new(), &SubscriptionTracker::default());
    chat_subscription_recorded(&fake).await;
    let rungs = [
        (
            Viewer::new("200000001", "everyone_viewer"),
            PermissionRung::Everyone,
        ),
        (
            Viewer::new("200000002", "sub_viewer")
                .with_badge(ViewerBadge::Subscriber { months: 3 }),
            PermissionRung::Subscriber,
        ),
        (
            Viewer::new("200000003", "vip_viewer").with_badge(ViewerBadge::Vip),
            PermissionRung::Vip,
        ),
        (
            Viewer::new("200000004", "mod_viewer")
                .with_badge(ViewerBadge::Subscriber { months: 12 })
                .with_badge(ViewerBadge::Moderator),
            PermissionRung::Moderator,
        ),
        (
            Viewer::new(account.user_id.clone(), account.login.clone())
                .with_badge(ViewerBadge::Broadcaster),
            PermissionRung::Broadcaster,
        ),
    ];

    for (viewer, rung) in &rungs {
        let text = format!("!ping from {}", viewer.login);
        let message_id = fake.inject_chat_message(viewer, &text).await.unwrap();

        let event = forge.next_event_of(CHAT_MESSAGE_KIND).await;
        assert_eq!(event.payload["user"]["login"], viewer.login.as_str());
        assert_eq!(event.payload["user"]["id"], viewer.user_id.as_str());
        assert_eq!(event.payload["message"], text.as_str());
        assert_eq!(event.payload["channel"], account.login.as_str());
        let chat: ChatPayload =
            serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
        assert_eq!(chat.platform_msg_id, message_id);
        assert_eq!(
            PermissionRung::from_badges(&chat.badges),
            *rung,
            "{}",
            viewer.login
        );
    }
}

#[tokio::test]
async fn forge_routes_a_refused_bearer_to_reauth_required() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let mut forge = Forge::against(&fake, &account, "a-token-the-fake-never-issued").await;

    let _chat = forge.start_chat(&TwitchLifecycle::new(), &SubscriptionTracker::default());

    let event = forge.next_event_of("platform.reauth_required").await;
    assert_eq!(event.payload["platform_id"], "twitch");
    let ledger = fake.ledger();
    assert!(ledger.subscriptions.is_empty());
    assert!(!ledger.requests.is_empty());
    assert!(
        ledger.requests.iter().all(
            |request| request.credentials == CredentialCheck::WrongBearer && request.status == 401
        ),
        "{:?}",
        ledger.requests
    );
}

#[tokio::test]
async fn forge_chat_session_follows_an_injected_reconnect_and_keeps_receiving_chat() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let mut forge = Forge::against(&fake, &account, &account.access_token).await;
    let _chat = forge.start_chat(&TwitchLifecycle::new(), &SubscriptionTracker::default());
    chat_subscription_recorded(&fake).await;

    fake.inject_session_reconnect().await.unwrap();
    fake.wait_for("forge to open the reconnect session", DEADLINE, |ledger| {
        ledger
            .live_sessions()
            .find(|session| session.reconnected_from.is_some())
            .map(|_| ())
    })
    .await
    .unwrap();
    fake.inject_chat_message(&Viewer::new("200000042", "alice"), "after reconnect")
        .await
        .unwrap();

    let event = forge.next_event_of(CHAT_MESSAGE_KIND).await;
    assert_eq!(event.payload["message"], "after reconnect");
}

#[tokio::test]
async fn forge_twitch_boot_calls_only_modeled_helix_endpoints() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let forge = Forge::against(&fake, &account, &account.access_token).await;
    let platform = forge.platform();
    let _bundle = TwitchIntegrationBundle::new(Some(account.login.clone()), Arc::clone(&platform));
    platform.connect().await.unwrap();

    let boot_paths = [
        "/helix/eventsub/subscriptions",
        "/helix/users",
        "/helix/streams",
        "/helix/polls",
        "/helix/predictions",
    ];
    let ledger = fake
        .wait_for("forge's boot-time Helix calls", DEADLINE, |ledger| {
            boot_paths
                .iter()
                .all(|path| ledger.requests.iter().any(|request| request.path == *path))
                .then(|| ledger.clone())
        })
        .await
        .unwrap();
    let unexpected: Vec<_> = ledger
        .unexpected_requests()
        .map(|request| format!("{} {}", request.method, request.path))
        .collect();
    assert!(
        unexpected.is_empty(),
        "unmodeled boot calls: {unexpected:?}"
    );
    let failed: Vec<_> = ledger
        .requests
        .iter()
        .filter(|request| request.status >= 300)
        .map(|request| format!("{} {} -> {}", request.method, request.path, request.status))
        .collect();
    assert!(failed.is_empty(), "refused boot calls: {failed:?}");
}

#[tokio::test]
async fn forge_chat_send_returns_the_message_id_the_fake_issued() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let forge = Forge::against(&fake, &account, &account.access_token).await;
    let transport = HelixHttpTransport::new(
        &forge.config.endpoints,
        Forge::rate_limiter(),
        Arc::clone(&forge.bus),
        account.client_id.clone(),
        Arc::new(CredentialsTokenSource::new(Arc::clone(&forge.creds))),
    );

    let sent = send_chat(&transport, &account.user_id, &account.user_id, "pong")
        .await
        .unwrap();

    let ledger = fake.ledger();
    let request = ledger
        .requests
        .iter()
        .find(|request| request.path == "/helix/chat/messages")
        .expect("chat send recorded");
    assert_eq!(request.body.as_ref().unwrap()["message"], "pong");
    assert_eq!(request.response["data"][0]["message_id"], sent.0.as_str());
}

async fn next_platform_event(events: &mut EventStream) -> Event {
    timeout(DEADLINE, events.recv())
        .await
        .expect("the platform published nothing in time")
        .expect("the platform event stream must stay open")
}

async fn platform_reaches_connected(platform: &TwitchPlatform) {
    timeout(DEADLINE, async {
        while platform.connection_state() != ConnectionState::Connected {
            tokio::time::sleep(STATE_POLL).await;
        }
    })
    .await
    .expect("the platform never reported Connected");
}

async fn sole_live_session_subscribed_to(fake: &FakeTwitch, types: &[&str]) {
    fake.wait_for(
        "exactly one live eventsub session holding forge's subscriptions",
        DEADLINE,
        |ledger| {
            let live: Vec<_> = ledger.live_sessions().collect();
            let [session] = live.as_slice() else {
                return None;
            };
            types
                .iter()
                .all(|subscription_type| {
                    ledger.subscriptions.iter().any(|subscription| {
                        subscription.session_id == session.id
                            && subscription.subscription_type == *subscription_type
                    })
                })
                .then_some(())
        },
    )
    .await
    .unwrap();
}

fn follow_event(account: &TwitchAccount) -> serde_json::Value {
    serde_json::json!({
        "user_id": "200000042",
        "user_login": "alice",
        "user_name": "alice",
        "broadcaster_user_id": account.user_id,
        "broadcaster_user_login": account.login,
        "broadcaster_user_name": account.login,
        "followed_at": "2026-10-07T12:00:00Z",
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn boot_connect_racing_a_reconnect_leaves_one_eventsub_session_delivering_each_event_once() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let forge = Forge::against(&fake, &account, &account.access_token).await;
    let platform = forge.platform();
    let bundle = TwitchIntegrationBundle::new(Some(account.login.clone()), Arc::clone(&platform));
    let mut events = platform.events();

    let boot = tokio::spawn({
        let platform = Arc::clone(&platform);
        async move { platform.connect().await }
    });
    let reconnect = tokio::spawn({
        let bundle = Arc::clone(&bundle);
        async move { bundle.reconnect().await }
    });
    boot.await.unwrap().unwrap();
    assert_eq!(reconnect.await.unwrap(), Ok(()));
    platform_reaches_connected(&platform).await;
    sole_live_session_subscribed_to(&fake, &[CHAT_SUBSCRIPTION, FOLLOW_SUBSCRIPTION]).await;

    let alice = Viewer::new("200000042", "alice");
    fake.inject_chat_message(&alice, "once").await.unwrap();
    let follow_reach = fake
        .inject_notification(FOLLOW_SUBSCRIPTION, follow_event(&account))
        .await
        .unwrap();
    fake.inject_chat_message(&alice, "marker").await.unwrap();

    let (mut chats, mut follows) = (0, 0);
    loop {
        let event = next_platform_event(&mut events).await;
        match event.kind.as_str() {
            CHAT_MESSAGE_KIND if event.payload["message"] == "marker" => break,
            CHAT_MESSAGE_KIND => chats += 1,
            FOLLOW_KIND => follows += 1,
            _ => {}
        }
    }
    assert_eq!(
        (follow_reach, chats, follows),
        (1, 1, 1),
        "(sockets the follow reached, chat deliveries, follow deliveries)"
    );
}

#[tokio::test]
async fn bundle_shutdown_publishes_a_final_disconnected_change_and_closes_the_eventsub_session() {
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let forge = Forge::against(&fake, &account, &account.access_token).await;
    let platform = forge.platform();
    let bundle = TwitchIntegrationBundle::new(Some(account.login.clone()), Arc::clone(&platform));
    let mut events = platform.events();
    platform.connect().await.unwrap();
    platform_reaches_connected(&platform).await;
    sole_live_session_subscribed_to(&fake, &[CHAT_SUBSCRIPTION]).await;

    bundle.shutdown().await;

    loop {
        let event = next_platform_event(&mut events).await;
        if event.kind == CONNECTION_STATE_CHANGED_KIND && event.payload["state"] == "disconnected" {
            break;
        }
    }
    fake.wait_for("forge to close its eventsub session", DEADLINE, |ledger| {
        (ledger.live_sessions().count() == 0).then_some(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_refused_chat_send_through_the_platform_publishes_request_fail_without_the_token() {
    const NEVER_ISSUED: &str = "a-token-the-fake-never-issued";
    let account = TwitchAccount::default();
    let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&account))
        .await
        .unwrap();
    let forge = Forge::against(&fake, &account, NEVER_ISSUED).await;
    let platform = forge.platform();
    let mut events = platform.events();

    platform
        .send_message(&account.login, "pong")
        .await
        .unwrap_err();

    let failure = loop {
        let event = next_platform_event(&mut events).await;
        if event.kind == "request.fail" {
            break event;
        }
    };
    assert_eq!(failure.payload["endpoint"], "/helix/chat/messages");
    assert!(
        !failure.payload.to_string().contains(NEVER_ISSUED),
        "token leaked into request.fail: {}",
        failure.payload
    );
}
