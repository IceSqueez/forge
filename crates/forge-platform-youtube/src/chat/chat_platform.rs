use forge_types::RecoverMutex;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventChannel, EventPublisher, EventSubscription};
use forge_platform_core::{
    ChatPlatform, ConnectionState, ConnectionTransition, PlatformError,
    connection_state_changed_event, reauth_required,
};
use forge_storage::BanLedgerRepo;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::account::channel_id_handle::ChannelIdHandle;
use crate::account::credentials_manager::YouTubeCredentialsManager;
use crate::api::data_api::YouTubeDataApi;
use crate::api::poll_health::{PollHealthReporter, PollLinkHealth};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use crate::chat::access_tokens::AccessTokens;
use crate::chat::avatar_book::AvatarBook;
use crate::chat::chat_poller::YouTubeChatPoller;
use crate::chat::send_chat::YouTubeSendChat;
use crate::chat::transport_mode::{ChatTransportMode, TransportModeSink};

use crate::vocabulary::INTEGRATION_ID as PLATFORM_ID;
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = forge_events::PLATFORM_EVENT_CHANNEL_CAPACITY;

pub struct YouTubePlatform {
    api: YouTubeDataApi,
    channel: ChannelIdHandle,
    events: Arc<EventChannel>,
    credentials_manager: Arc<YouTubeCredentialsManager>,
    sender: YouTubeSendChat,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    avatars: AvatarBook,
    state: Arc<Mutex<ConnectionState>>,
    state_tx: watch::Sender<ConnectionState>,
    transport_mode_tx: watch::Sender<ChatTransportMode>,
    cancel: Mutex<Option<CancellationToken>>,
}

impl YouTubePlatform {
    pub fn new(
        api: YouTubeDataApi,
        channel_id: String,
        credentials_manager: Arc<YouTubeCredentialsManager>,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        let sender = YouTubeSendChat::new(
            api.clone(),
            token_source(Arc::clone(&credentials_manager)).source(),
            live_chat_id.clone(),
        );
        let (state_tx, _) = watch::channel(ConnectionState::Disconnected);
        let (transport_mode_tx, _) = watch::channel(ChatTransportMode::Streaming);
        Self {
            api,
            channel: channel_id.into(),
            events: Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY)),
            credentials_manager,
            sender,
            live_chat_id,
            active_broadcast_id,
            ban_ledger,
            avatars: AvatarBook::default(),
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            state_tx,
            transport_mode_tx,
            cancel: Mutex::new(None),
        }
    }

    pub fn active_broadcast_id(&self) -> ActiveBroadcastIdHandle {
        self.active_broadcast_id.clone()
    }

    pub(crate) fn channel_id(&self) -> String {
        self.channel.get()
    }

    pub(crate) fn channel_handle(&self) -> &ChannelIdHandle {
        &self.channel
    }

    pub(crate) fn ban_ledger(&self) -> &Arc<dyn BanLedgerRepo> {
        &self.ban_ledger
    }

    pub(crate) fn with_avatar_book(mut self, avatars: AvatarBook) -> Self {
        self.avatars = avatars;
        self
    }

    pub(crate) fn api(&self) -> &YouTubeDataApi {
        &self.api
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.state_tx.subscribe()
    }

    pub(crate) fn transport_mode_receiver(&self) -> watch::Receiver<ChatTransportMode> {
        self.transport_mode_tx.subscribe()
    }
}

#[async_trait]
impl ChatPlatform for YouTubePlatform {
    fn connection_state(&self) -> ConnectionState {
        *self.state.lock_recover()
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let stored = self
            .credentials_manager
            .load()
            .await?
            .ok_or_else(|| reauth_required(PLATFORM_ID))?;
        let stored = if stored.identity_pending() {
            self.credentials_manager.resolve_identity().await?
        } else {
            stored
        };
        if self.channel.is_pending() {
            self.channel.adopt(&stored.channel_id);
        }
        let channel_id = self.channel.get();

        let previous = self.cancel.lock_recover().take();
        if let Some(previous) = previous {
            let _current = self.state.lock_recover();
            previous.cancel();
        }

        publish_transition(
            &self.state,
            &self.state_tx,
            &self.events,
            ConnectionState::Connecting,
            ConnectionTransition::Expected,
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
        let forward_events = Arc::clone(&self.events);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                forward_events.publish(event);
            }
        });

        let cancel = CancellationToken::new();
        let poller = YouTubeChatPoller::new(
            self.api.clone(),
            token_source(Arc::clone(&self.credentials_manager)).source(),
            tx,
            channel_id.clone(),
            self.live_chat_id.clone(),
            self.active_broadcast_id.clone(),
            Arc::clone(&self.ban_ledger),
        )
        .with_access_tokens(token_source(Arc::clone(&self.credentials_manager)))
        .with_health_reporter(health_reporter(
            Arc::clone(&self.state),
            self.state_tx.clone(),
            Arc::clone(&self.events),
            cancel.clone(),
        ))
        .with_transport_mode(TransportModeSink::new(self.transport_mode_tx.clone()))
        .with_avatar_book(self.avatars.clone());

        let exit_state = Arc::clone(&self.state);
        let exit_state_tx = self.state_tx.clone();
        let exit_events = Arc::clone(&self.events);
        let poller_cancel = cancel.clone();
        let poller_cancelled = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = poller.run_supervised(poller_cancel).await {
                tracing::warn!(
                    error = %crate::api::error_shape::redact_platform_error(&err),
                    "youtube chat poller exited"
                );
            }
            publish_unless_cancelled(
                &exit_state,
                &exit_state_tx,
                &exit_events,
                &poller_cancelled,
                ConnectionState::Disconnected,
                ConnectionTransition::Unexpected,
            );
            tracing::info!("youtube chat receive stopped");
        });

        *self.cancel.lock_recover() = Some(cancel);
        tracing::info!(channel_id = %channel_id, "youtube chat receive started");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let cancel = self.cancel.lock_recover().take();
        let mut current = self.state.lock_recover();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
        apply_transition(
            &mut current,
            &self.state_tx,
            &self.events,
            ConnectionState::Disconnected,
            ConnectionTransition::Expected,
        );
        Ok(())
    }

    async fn send_message(&self, _channel: &str, text: &str) -> Result<(), PlatformError> {
        self.credentials_manager
            .load()
            .await?
            .ok_or_else(|| PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            })?;
        self.sender.send(text).await
    }

    fn events(&self) -> EventSubscription {
        self.events.subscribe()
    }
}

pub(crate) fn token_source(manager: Arc<YouTubeCredentialsManager>) -> AccessTokens {
    let current_manager = Arc::clone(&manager);
    AccessTokens::new(
        Arc::new(move || {
            let manager = Arc::clone(&current_manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        }),
        Arc::new(move |rejected| {
            let manager = Arc::clone(&manager);
            Box::pin(async move {
                manager
                    .refresh(&rejected)
                    .await
                    .map(|credentials| credentials.access_token.expose().to_owned())
            })
        }),
    )
}

fn health_reporter(
    state: Arc<Mutex<ConnectionState>>,
    state_tx: watch::Sender<ConnectionState>,
    events: Arc<EventChannel>,
    cancel: CancellationToken,
) -> PollHealthReporter {
    Arc::new(move |health| {
        let (new, transition) = match health {
            PollLinkHealth::Lost => (
                ConnectionState::Reconnecting,
                ConnectionTransition::Unexpected,
            ),
            PollLinkHealth::Healthy => (ConnectionState::Connected, ConnectionTransition::Expected),
        };
        publish_unless_cancelled(&state, &state_tx, &events, &cancel, new, transition);
    })
}

fn publish_unless_cancelled(
    state: &Mutex<ConnectionState>,
    state_tx: &watch::Sender<ConnectionState>,
    events: &EventChannel,
    cancel: &CancellationToken,
    new: ConnectionState,
    transition: ConnectionTransition,
) {
    let mut current = state.lock_recover();
    if !cancel.is_cancelled() {
        apply_transition(&mut current, state_tx, events, new, transition);
    }
}

fn publish_transition(
    state: &Mutex<ConnectionState>,
    state_tx: &watch::Sender<ConnectionState>,
    events: &EventChannel,
    new: ConnectionState,
    transition: ConnectionTransition,
) {
    let mut current = state.lock_recover();
    apply_transition(&mut current, state_tx, events, new, transition);
}

fn apply_transition(
    current: &mut ConnectionState,
    state_tx: &watch::Sender<ConnectionState>,
    events: &EventChannel,
    new: ConnectionState,
    transition: ConnectionTransition,
) {
    if *current == new {
        return;
    }
    *current = new;
    state_tx.send_replace(new);
    events.publish(connection_state_changed_event(PLATFORM_ID, new, transition));
}

#[cfg(test)]
mod tests {
    use forge_storage::test_support::MemoryCredentials;

    use forge_events::Event;
    use forge_events::{PlatformConnectionChanged, TypedEvent};

    use time::OffsetDateTime;

    use super::*;

    fn stored_credentials() -> String {
        serde_json::to_string(&crate::account::credentials::YouTubeCredentials {
            access_token: forge_types::OAuthToken::new("access"),
            refresh_token: forge_types::OAuthToken::new("refresh"),
            client_id: "client".to_owned(),
            channel_id: "UCtest".to_owned(),
            channel_title: "Test".to_owned(),
            channel_handle: None,
            expires_at: OffsetDateTime::now_utc() + time::Duration::days(1),
        })
        .unwrap()
    }

    fn platform_with(
        credentials: MemoryCredentials,
        api: crate::api::data_api::YouTubeDataApi,
    ) -> YouTubePlatform {
        let manager = Arc::new(YouTubeCredentialsManager::new(
            Arc::new(credentials),
            crate::endpoints_test_support::unreachable_oauth_client(),
            crate::endpoints_test_support::unreachable_data_api(),
        ));
        YouTubePlatform::new(
            api,
            "UCtest".to_owned(),
            manager,
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    fn signed_out_platform() -> YouTubePlatform {
        platform_with(
            MemoryCredentials::default(),
            crate::endpoints_test_support::unreachable_data_api(),
        )
    }

    fn platform() -> YouTubePlatform {
        platform_with(
            MemoryCredentials::holding(
                crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
                &stored_credentials(),
            ),
            crate::endpoints_test_support::unreachable_data_api(),
        )
    }

    fn state_of(event: &Event) -> Option<String> {
        if event.kind != PlatformConnectionChanged::KIND {
            return None;
        }
        Some(event.payload["state"].as_str().unwrap().to_owned())
    }

    #[test]
    fn connection_state_is_disconnected_before_connect() {
        assert_eq!(platform().connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn send_message_without_credentials_requires_reauth() {
        let p = signed_out_platform();
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "youtube"),
            "expected ReauthRequired {{ platform: youtube }}, got {err:?}"
        );
    }

    #[tokio::test]
    async fn connect_without_credentials_requires_reauth_and_stays_disconnected() {
        let p = signed_out_platform();
        let err = p.connect().await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "youtube"),
            "expected ReauthRequired {{ platform: youtube }}, got {err:?}"
        );
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn an_explicit_disconnect_before_any_poll_succeeds_announces_connecting_then_disconnected()
     {
        let p = platform();
        let mut stream = p.events();
        p.connect().await.unwrap();
        p.disconnect().await.unwrap();
        drop(p);

        let mut announcements = Vec::new();
        while let Ok(event) = stream.recv().await {
            if let Some(state) = state_of(&event) {
                announcements.push((state, event.payload["expected"].as_bool()));
            }
        }
        assert_eq!(
            announcements,
            [
                ("connecting".to_owned(), Some(true)),
                ("disconnected".to_owned(), Some(true)),
            ]
        );
    }

    fn platform_polling(server: &crate::endpoints_test_support::StubServer) -> YouTubePlatform {
        platform_with(
            MemoryCredentials::holding(
                crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
                &stored_credentials(),
            ),
            crate::endpoints_test_support::data_api_at(&server.uri()),
        )
    }

    #[tokio::test]
    async fn connect_stays_connecting_until_the_first_poll_succeeds() {
        let server = crate::endpoints_test_support::StubServer::start().await;
        server.reply_always(
            "/liveBroadcasts",
            crate::endpoints_test_support::StubReply::held(),
        );
        let p = platform_polling(&server);
        p.connect().await.unwrap();
        let reached_server = crate::endpoints_test_support::wait_until(|| {
            server.request_count("/liveBroadcasts") == 1
        })
        .await;

        assert_eq!(
            (reached_server, p.connection_state()),
            (true, ConnectionState::Connecting)
        );
    }

    #[tokio::test]
    async fn the_first_successful_poll_moves_the_link_to_connected() {
        let server = crate::endpoints_test_support::StubServer::start().await;
        server.reply_always(
            "/liveBroadcasts",
            crate::endpoints_test_support::StubReply::ok(&serde_json::json!({ "items": [] })),
        );
        let p = platform_polling(&server);
        let mut state = p.state_receiver();
        p.connect().await.unwrap();

        let reached = tokio::time::timeout(
            crate::endpoints_test_support::HANG_GUARD,
            state.wait_for(|s| *s == ConnectionState::Connected),
        )
        .await;

        assert!(reached.is_ok_and(|r| r.is_ok()));
        p.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn connect_receives_chat_over_the_stream_without_polling_for_messages() {
        use crate::endpoints_test_support::{
            GrpcStep, GrpcStub, HANG_GUARD, StubReply, StubServer, data_api_streaming, grpc_frame,
            grpc_head,
        };
        use crate::stream_list_proto::live_chat_message_snippet::type_wrapper::Type;
        use crate::stream_list_proto::{
            LiveChatMessage, LiveChatMessageAuthorDetails, LiveChatMessageListResponse,
            LiveChatMessageSnippet,
        };
        use prost::Message;

        let rest = StubServer::start().await;
        rest.reply_always(
            "/liveBroadcasts",
            StubReply::ok(&serde_json::json!({
                "items": [{ "id": "bc-1", "snippet": { "liveChatId": "lc-1", "title": "Live" } }]
            })),
        );
        let page = LiveChatMessageListResponse {
            next_page_token: Some("s-1".to_owned()),
            items: vec![LiveChatMessage {
                id: Some("m-1".to_owned()),
                snippet: Some(LiveChatMessageSnippet {
                    r#type: Some(Type::TextMessageEvent as i32),
                    display_message: Some("hi".to_owned()),
                    ..Default::default()
                }),
                author_details: Some(LiveChatMessageAuthorDetails {
                    channel_id: Some("UCviewer".to_owned()),
                    display_name: Some("Viewer".to_owned()),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let grpc = GrpcStub::serving(vec![
            grpc_head(),
            grpc_frame(&page.encode_to_vec()),
            GrpcStep::Hold,
        ])
        .await;
        let p = platform_with(
            MemoryCredentials::holding(
                crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
                &stored_credentials(),
            ),
            data_api_streaming(
                &rest.uri(),
                &grpc.uri(),
                crate::api::quota_state::SharedQuota::default(),
                Arc::new(forge_events::test_support::NullPublisher),
            ),
        );
        let mut events = p.events();
        p.connect().await.unwrap();

        let received = tokio::time::timeout(HANG_GUARD, async {
            while let Ok(event) = events.recv().await {
                if event.kind == crate::vocabulary::event_kinds::CHAT_MESSAGE {
                    return true;
                }
            }
            false
        })
        .await;
        p.disconnect().await.unwrap();

        assert_eq!(
            (received, rest.request_count("/liveChat/messages")),
            (Ok(true), 0)
        );
    }

    fn pending_credentials() -> MemoryCredentials {
        let mut creds: crate::account::credentials::YouTubeCredentials =
            serde_json::from_str(&stored_credentials()).unwrap();
        creds.channel_id = String::new();
        creds.channel_title = String::new();
        MemoryCredentials::holding(
            crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
            &serde_json::to_string(&creds).unwrap(),
        )
    }

    fn pending_platform(server: &crate::endpoints_test_support::StubServer) -> YouTubePlatform {
        let manager = Arc::new(YouTubeCredentialsManager::new(
            Arc::new(pending_credentials()),
            crate::endpoints_test_support::oauth_client_at(&server.uri()),
            crate::endpoints_test_support::data_api_at(&server.uri()),
        ));
        YouTubePlatform::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            String::new(),
            manager,
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    #[tokio::test]
    async fn connect_resolves_a_pending_identity_before_the_poller_starts() {
        let server = crate::endpoints_test_support::StubServer::start().await;
        server.reply_always(
            "/channels",
            crate::endpoints_test_support::StubReply::ok(&serde_json::json!({
                "items": [{ "id": "UCresolved", "snippet": { "title": "Resolved" } }]
            })),
        );
        server.reply_always(
            "/liveBroadcasts",
            crate::endpoints_test_support::StubReply::held(),
        );
        let p = pending_platform(&server);

        p.connect().await.unwrap();
        let polled = crate::endpoints_test_support::wait_until(|| {
            server.request_count("/liveBroadcasts") == 1
        })
        .await;

        assert_eq!(
            (
                p.channel_id(),
                polled,
                server.answered_paths().first().cloned()
            ),
            ("UCresolved".to_owned(), true, Some("/channels".to_owned()))
        );
    }

    #[tokio::test]
    async fn connect_with_a_pending_identity_under_quota_errors_and_starts_no_poller() {
        let server = crate::endpoints_test_support::StubServer::start().await;
        let status = reqwest::StatusCode::FORBIDDEN.as_u16();
        server.reply_always(
            "/channels",
            crate::endpoints_test_support::StubReply::json(
                status,
                &serde_json::json!({ "error": { "code": status, "errors": [{ "reason": "quotaExceeded" }] } }),
            ),
        );
        let p = pending_platform(&server);

        let outcome = p.connect().await;

        assert_eq!(
            (
                matches!(outcome, Err(PlatformError::QuotaExhausted)),
                p.cancel.lock().unwrap().is_none(),
                p.connection_state(),
                p.channel_handle().is_pending()
            ),
            (true, true, ConnectionState::Disconnected, true)
        );
    }

    #[tokio::test]
    async fn reconnecting_a_running_poller_does_not_announce_the_retired_poller_exit() {
        let p = platform();
        let mut stream = p.events();
        p.connect().await.unwrap();
        p.connect().await.unwrap();
        p.disconnect().await.unwrap();
        drop(p);

        let mut announcements = Vec::new();
        while let Ok(event) = stream.recv().await {
            if let Some(state) = state_of(&event) {
                announcements.push((state, event.payload["expected"].as_bool()));
            }
        }
        assert_eq!(
            announcements,
            [
                ("connecting".to_owned(), Some(true)),
                ("disconnected".to_owned(), Some(true)),
            ]
        );
    }

    fn info_lifecycle_lines(
        lines: &[forge_test_support::log_capture::CapturedLine],
    ) -> Vec<&forge_test_support::log_capture::CapturedLine> {
        forge_test_support::log_capture::forge_lines(lines)
            .into_iter()
            .filter(|line| line.level == tracing::Level::INFO)
            .collect()
    }

    #[test]
    fn connect_logs_a_started_line_identifying_the_channel_and_nothing_else() {
        let (result, lines) =
            forge_test_support::log_capture::capture_blocking(tracing::Level::INFO, async {
                platform().connect().await
            });
        result.unwrap();

        let captured = info_lifecycle_lines(&lines);
        assert_eq!(
            captured.len(),
            1,
            "connect must log exactly one INFO lifecycle line, got {captured:?}"
        );
        let started = captured.first().unwrap();
        assert_eq!(started.field("channel_id"), Some("UCtest"));
        assert_eq!(
            started.field_names(),
            ["channel_id", "message"],
            "the lifecycle line must carry the channel id and nothing else - any extra field \
             here is an unreviewed value on an INFO-level record: {started:?}"
        );
    }

    #[tokio::test]
    async fn a_connect_disconnect_cycle_logs_the_started_line_then_the_stopped_line() {
        let recording = forge_test_support::log_capture::LogRecording::start(tracing::Level::INFO);
        let p = platform();
        p.connect().await.unwrap();
        p.disconnect().await.unwrap();
        forge_test_support::settle_until("the retired poller logged its exit", || {
            info_lifecycle_lines(&recording.lines()).len() >= 2
        })
        .await;

        let lines = recording.lines();
        let messages: Vec<&str> = info_lifecycle_lines(&lines)
            .iter()
            .map(|line| line.message())
            .collect();
        assert_eq!(
            messages.len(),
            2,
            "expected a started/stopped pair, got {messages:?}"
        );
        assert!(
            messages[0].contains("started") && messages[1].contains("stopped"),
            "the pair must be ordered started-then-stopped, got {messages:?}"
        );
    }

    fn reporter_for(p: &YouTubePlatform, cancel: &CancellationToken) -> PollHealthReporter {
        health_reporter(
            Arc::clone(&p.state),
            p.state_tx.clone(),
            Arc::clone(&p.events),
            cancel.clone(),
        )
    }

    impl YouTubePlatform {
        pub(crate) fn transport_mode_sink(&self) -> TransportModeSink {
            TransportModeSink::new(self.transport_mode_tx.clone())
        }

        pub(crate) fn report_link_health(&self, health: PollLinkHealth) {
            reporter_for(self, &CancellationToken::new())(health);
        }
    }

    async fn announcements(mut stream: EventSubscription) -> Vec<(String, Option<bool>)> {
        let mut seen = Vec::new();
        while let Ok(event) = stream.recv().await {
            if let Some(state) = state_of(&event) {
                seen.push((state, event.payload["expected"].as_bool()));
            }
        }
        seen
    }

    #[tokio::test]
    async fn poll_loss_then_recovery_announces_unexpected_reconnecting_then_expected_connected() {
        let p = platform();
        let stream = p.events();
        let report = reporter_for(&p, &CancellationToken::new());
        report(PollLinkHealth::Lost);
        let lost_state = p.connection_state();
        report(PollLinkHealth::Healthy);
        let recovered_state = p.connection_state();
        drop((report, p));
        assert_eq!(
            (lost_state, recovered_state, announcements(stream).await),
            (
                ConnectionState::Reconnecting,
                ConnectionState::Connected,
                vec![
                    ("reconnecting".to_owned(), Some(false)),
                    ("connected".to_owned(), Some(true)),
                ]
            )
        );
    }

    #[tokio::test]
    async fn poll_health_reported_after_cancel_changes_nothing() {
        let p = platform();
        let stream = p.events();
        let cancel = CancellationToken::new();
        let report = reporter_for(&p, &cancel);
        cancel.cancel();
        report(PollLinkHealth::Lost);
        let state = p.connection_state();
        drop((report, p));
        assert_eq!(
            (state, announcements(stream).await),
            (ConnectionState::Disconnected, Vec::new())
        );
    }

    #[tokio::test]
    async fn poll_loss_reported_after_disconnect_leaves_the_link_disconnected() {
        let p = platform();
        let cancel = CancellationToken::new();
        *p.cancel.lock().unwrap() = Some(cancel.clone());
        let report = reporter_for(&p, &cancel);
        report(PollLinkHealth::Healthy);
        p.disconnect().await.unwrap();
        report(PollLinkHealth::Lost);
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }
}
