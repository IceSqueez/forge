use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventChannel, EventPublisher, EventSubscription};
use forge_platform_core::{
    ChatPlatform, ConnectionState, ConnectionTransition, PlatformError,
    connection_state_changed_event,
};
use forge_storage::BanLedgerRepo;
use futures::future::BoxFuture;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::chat_poller::YoutubeChatPoller;
use crate::credentials_manager::YoutubeCredentialsManager;
use crate::data_api::YoutubeDataApi;
use crate::live_chat_id::LiveChatIdHandle;
use crate::poll_health::{PollHealthReporter, PollLinkHealth};
use crate::send_chat::YoutubeSendChat;

const PLATFORM_ID: &str = "youtube";
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 256;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubePlatform {
    api: YoutubeDataApi,
    channel_id: String,
    events: Arc<EventChannel>,
    credentials_manager: Arc<YoutubeCredentialsManager>,
    sender: YoutubeSendChat,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    state: Arc<Mutex<ConnectionState>>,
    state_tx: watch::Sender<ConnectionState>,
    cancel: Mutex<Option<CancellationToken>>,
}

impl YoutubePlatform {
    pub fn new(
        api: YoutubeDataApi,
        channel_id: String,
        credentials_manager: Arc<YoutubeCredentialsManager>,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        let sender = YoutubeSendChat::new(
            api.clone(),
            token_source(Arc::clone(&credentials_manager)),
            live_chat_id.clone(),
        );
        let (state_tx, _) = watch::channel(ConnectionState::Disconnected);
        Self {
            api,
            channel_id,
            events: Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY)),
            credentials_manager,
            sender,
            live_chat_id,
            active_broadcast_id,
            ban_ledger,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            state_tx,
            cancel: Mutex::new(None),
        }
    }

    pub fn active_broadcast_id(&self) -> ActiveBroadcastIdHandle {
        self.active_broadcast_id.clone()
    }

    pub(crate) fn channel_id(&self) -> &str {
        &self.channel_id
    }

    pub(crate) fn ban_ledger(&self) -> &Arc<dyn BanLedgerRepo> {
        &self.ban_ledger
    }

    pub(crate) fn api(&self) -> &YoutubeDataApi {
        &self.api
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.state_tx.subscribe()
    }
}

#[async_trait]
impl ChatPlatform for YoutubePlatform {
    fn connection_state(&self) -> ConnectionState {
        *self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let previous = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(previous) = previous {
            let _current = self.state.lock().unwrap_or_else(|p| p.into_inner());
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
        let poller = YoutubeChatPoller::new(
            self.api.clone(),
            token_source(Arc::clone(&self.credentials_manager)),
            tx,
            self.channel_id.clone(),
            self.live_chat_id.clone(),
            self.active_broadcast_id.clone(),
            Arc::clone(&self.ban_ledger),
        )
        .with_health_reporter(health_reporter(
            Arc::clone(&self.state),
            self.state_tx.clone(),
            Arc::clone(&self.events),
            cancel.clone(),
        ));

        let exit_state = Arc::clone(&self.state);
        let exit_state_tx = self.state_tx.clone();
        let exit_events = Arc::clone(&self.events);
        let poller_cancel = cancel.clone();
        let poller_cancelled = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = poller.run(poller_cancel).await {
                tracing::warn!(
                    error = %crate::error_shape::redact_platform_error(&err),
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
            tracing::info!("youtube chat polling stopped");
        });

        *self.cancel.lock().unwrap_or_else(|p| p.into_inner()) = Some(cancel);
        publish_transition(
            &self.state,
            &self.state_tx,
            &self.events,
            ConnectionState::Connected,
            ConnectionTransition::Expected,
        );
        tracing::info!(channel_id = %self.channel_id, "youtube chat polling started");
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let cancel = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).take();
        let mut current = self.state.lock().unwrap_or_else(|p| p.into_inner());
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

fn token_source(manager: Arc<YoutubeCredentialsManager>) -> TokenSource {
    Arc::new(move || {
        let manager = Arc::clone(&manager);
        Box::pin(async move { manager.get_valid_access_token().await })
    })
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
    let mut current = state.lock().unwrap_or_else(|p| p.into_inner());
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
    let mut current = state.lock().unwrap_or_else(|p| p.into_inner());
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
    use async_trait::async_trait;
    use forge_events::Event;
    use forge_platform_core::CONNECTION_STATE_CHANGED_KIND;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;

    use super::*;

    struct EmptyRepo;
    #[async_trait]
    impl CredentialsRepo for EmptyRepo {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }
        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn platform() -> YoutubePlatform {
        let manager = Arc::new(YoutubeCredentialsManager::new(
            Arc::new(EmptyRepo),
            crate::endpoints_test_support::unreachable_oauth_client(),
            crate::endpoints_test_support::unreachable_data_api(),
        ));
        YoutubePlatform::new(
            crate::endpoints_test_support::unreachable_data_api(),
            "UCtest".to_owned(),
            manager,
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    fn state_of(event: &Event) -> Option<String> {
        if event.kind != CONNECTION_STATE_CHANGED_KIND {
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
        let p = platform();
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "youtube"),
            "expected ReauthRequired {{ platform: youtube }}, got {err:?}"
        );
    }

    #[tokio::test]
    async fn an_explicit_disconnect_is_announced_once_and_flagged_expected() {
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
                ("connected".to_owned(), Some(true)),
                ("disconnected".to_owned(), Some(true)),
            ]
        );
    }

    #[tokio::test]
    async fn reconnecting_a_running_poller_stays_connected_after_the_retired_poller_exits() {
        let p = platform();
        p.connect().await.unwrap();
        p.connect().await.unwrap();

        for _ in 0..64 {
            tokio::task::yield_now().await;
        }

        assert_eq!(p.connection_state(), ConnectionState::Connected);
    }

    fn info_lifecycle_lines(
        lines: &[crate::log_capture::CapturedLine],
    ) -> Vec<&crate::log_capture::CapturedLine> {
        crate::log_capture::forge_lines(lines)
            .into_iter()
            .filter(|line| line.level == tracing::Level::INFO)
            .collect()
    }

    #[test]
    fn connect_logs_a_started_line_identifying_the_channel_and_nothing_else() {
        let (result, lines) = crate::log_capture::capture_blocking(tracing::Level::INFO, async {
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

    #[test]
    fn a_connect_disconnect_cycle_logs_the_started_line_then_the_stopped_line() {
        let (_, lines) = crate::log_capture::capture_blocking(tracing::Level::INFO, async {
            let p = platform();
            p.connect().await.unwrap();
            p.disconnect().await.unwrap();
            tokio::task::yield_now().await;
        });

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

    fn reporter_for(p: &YoutubePlatform, cancel: &CancellationToken) -> PollHealthReporter {
        health_reporter(
            Arc::clone(&p.state),
            p.state_tx.clone(),
            Arc::clone(&p.events),
            cancel.clone(),
        )
    }

    async fn announcements(mut stream: EventStream) -> Vec<(String, Option<bool>)> {
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
