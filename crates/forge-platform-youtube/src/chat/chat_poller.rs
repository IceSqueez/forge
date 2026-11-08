use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::{DedupSet, ExpectedRefusal, GatewayRequest};
use forge_platform_core::TokenSource;
use forge_platform_core::{PlatformError, ViewerReport};
use forge_storage::BanLedgerRepo;
use forge_types::Secret;
use forge_types::{
    ChatEventDetail, ChatModerationAction, ChatModerationPayload, ChatPayload, ChatSegment,
    ChatViewer, CurrencyCode, ModerationMarks, MoneyAmount, UserBadge, UserRemoval,
};
use time::OffsetDateTime;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::api::data_api::{DataApiError, YouTubeDataApi, malformed};
use crate::api::poll_health::{
    CONSECUTIVE_POLL_FAILURES_FOR_LOSS, PollHealthReporter, PollHealthTracker,
};
use crate::api::quota_state::{CHAT_POLL_COST, today_pacific};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::broadcast_check::{CheckFailure, CheckedChat, check_broadcast};
use crate::broadcast::broadcast_lookup::{LookupFailure, fetch_active_broadcast};
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use crate::broadcast::lookup_wake::{BroadcastLookupWake, LookupWakeListener};
use crate::chat::access_tokens::AccessTokens;
use crate::chat::avatar_book::AvatarBook;
use crate::chat::ban_ledger::{USER_BANNED_EVENT_TYPE, observed_ban};
use crate::chat::receive_supervisor::{LiveTransports, ReceiveSupervisor, ReceiveTimings};
use crate::chat::transport_mode::TransportModeSink;
use crate::vocabulary::event_kinds;
use crate::vocabulary::fields::{
    ban as ban_fields, chat as chat_fields, chat_mod as chat_mod_fields, entity,
    gift as gift_fields, member as member_fields, stream as stream_fields,
    support as support_fields,
};

mod broadcast_watch;
mod events;
mod fetch;
mod offline_wait;
mod receive;
mod stream_transcode;
mod wire;

use self::broadcast_watch::BroadcastWatch;
pub(crate) use self::fetch::{ChatMessagesResponse, FetchFailure, fetch_chat_page, page_from_body};
use self::offline_wait::OfflineWait;
pub(crate) use self::receive::{ChatSession, PageEffect, PollingReceive, Receive, ReceiveExit};
pub(crate) use self::stream_transcode::transcode_response;
use self::wire::*;

const POLL_FLOOR_MS: u64 = 3_000;
const LONG_INTERVAL_MS: u64 = 60_000;
const BROADCAST_CADENCE_SECS: u64 = 60;
const OFFLINE_CADENCE_SECS: u64 = 300;
const WAKE_FOLLOW_UP_INTERVAL_SECS: u64 = 20;
const DEDUP_WINDOW_SIZE: usize = 500;
const CHAT_ENDED_EVENT_TYPE: &str = "chatEndedEvent";
const LIVE_CHAT_ENDED_REASON: &str = "liveChatEnded";
const LIVE_CHAT_ENDED_STATUS: u16 = forge_net::status::FORBIDDEN;
const CHAT_MESSAGES_PAGE_SIZE: &str = "200";
const DEFAULT_POLLING_INTERVAL_MS: u64 = 5_000;
const MESSAGES_PATH: &str = "/liveChat/messages";
pub(crate) const MISSING_SCOPE: &str = "chat read scope missing";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChatPollTimings {
    pub poll_floor: Duration,
    pub long_interval: Duration,
    pub broadcast_cadence: Duration,
    pub offline_cadence: Duration,
    pub wake_follow_up_interval: Duration,
}

impl ChatPollTimings {
    pub const DEFAULT: Self = Self {
        poll_floor: Duration::from_millis(POLL_FLOOR_MS),
        long_interval: Duration::from_millis(LONG_INTERVAL_MS),
        broadcast_cadence: Duration::from_secs(BROADCAST_CADENCE_SECS),
        offline_cadence: Duration::from_secs(OFFLINE_CADENCE_SECS),
        wake_follow_up_interval: Duration::from_secs(WAKE_FOLLOW_UP_INTERVAL_SECS),
    };
}

impl Default for ChatPollTimings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Default)]
pub struct ChatPollerStop(CancellationToken);

impl ChatPollerStop {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.cancel();
    }

    pub fn stop_on_drop(self) -> ChatPollerStopGuard {
        ChatPollerStopGuard {
            _cancels_on_drop: self.0.drop_guard(),
        }
    }
}

pub struct ChatPollerStopGuard {
    _cancels_on_drop: DropGuard,
}

struct KnownBroadcast {
    broadcast_id: String,
    live_chat_id: String,
}

pub struct YouTubeChatPoller {
    api: YouTubeDataApi,
    access_tokens: AccessTokens,
    bus_sender: UnboundedSender<Event>,
    channel_id: String,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    health_reporter: Option<PollHealthReporter>,
    transport_mode: Option<TransportModeSink>,
    viewer_reports: Option<tokio::sync::watch::Sender<ViewerReport>>,
    avatars: Option<AvatarBook>,
    lookup_wake: BroadcastLookupWake,
    timings: ChatPollTimings,
}

impl YouTubeChatPoller {
    pub fn new(
        api: YouTubeDataApi,
        access_token_source: TokenSource,
        bus_sender: UnboundedSender<Event>,
        channel_id: String,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        Self {
            api,
            access_tokens: access_token_source.into(),
            bus_sender,
            channel_id,
            live_chat_id,
            active_broadcast_id,
            ban_ledger,
            health_reporter: None,
            transport_mode: None,
            viewer_reports: None,
            avatars: None,
            lookup_wake: BroadcastLookupWake::new(),
            timings: ChatPollTimings::DEFAULT,
        }
    }

    pub(crate) fn with_access_tokens(mut self, access_tokens: AccessTokens) -> Self {
        self.access_tokens = access_tokens;
        self
    }

    pub fn with_timings(mut self, timings: ChatPollTimings) -> Self {
        self.timings = timings;
        self
    }

    pub(crate) fn with_health_reporter(mut self, reporter: PollHealthReporter) -> Self {
        self.health_reporter = Some(reporter);
        self
    }

    pub(crate) fn with_transport_mode(mut self, sink: TransportModeSink) -> Self {
        self.transport_mode = Some(sink);
        self
    }

    pub(crate) fn with_viewer_reports(
        mut self,
        reports: tokio::sync::watch::Sender<ViewerReport>,
    ) -> Self {
        self.viewer_reports = Some(reports);
        self
    }

    pub(crate) fn with_avatar_book(mut self, avatars: AvatarBook) -> Self {
        self.avatars = Some(avatars);
        self
    }

    pub fn with_lookup_wake(mut self, wake: BroadcastLookupWake) -> Self {
        self.lookup_wake = wake;
        self
    }

    pub async fn run_until_stopped(self, stop: ChatPollerStop) -> Result<(), PlatformError> {
        self.run(stop.0).await
    }

    pub(crate) async fn run(self, cancel: CancellationToken) -> Result<(), PlatformError> {
        self.run_with(cancel, PollingReceive).await
    }

    pub(crate) async fn run_supervised(
        self,
        cancel: CancellationToken,
    ) -> Result<(), PlatformError> {
        let mut supervisor = ReceiveSupervisor::new(
            LiveTransports::new(self.api.clone(), self.access_tokens.clone()),
            ReceiveTimings::DEFAULT,
        );
        if let Some(sink) = self.transport_mode.clone() {
            supervisor = supervisor.with_mode_sink(sink);
        }
        self.run_with(cancel, supervisor).await
    }

    pub(crate) async fn run_with(
        self,
        cancel: CancellationToken,
        mut receive: impl Receive,
    ) -> Result<(), PlatformError> {
        let mut wake = self.lookup_wake.listen();
        let outcome = self.run_sessions(&cancel, &mut receive, &mut wake).await;
        self.report_viewers(ViewerReport::Absent);
        outcome
    }

    async fn run_sessions(
        &self,
        cancel: &CancellationToken,
        receive: &mut impl Receive,
        wake: &mut LookupWakeListener,
    ) -> Result<(), PlatformError> {
        let mut offline = OfflineWait::default();
        let mut dedup = DedupSet::bounded(DEDUP_WINDOW_SIZE);
        let mut watch = BroadcastWatch::new();
        let mut last_broadcast_id: Option<String> = None;
        let mut known: Option<KnownBroadcast> = None;
        let mut is_live = false;
        let mut health = PollHealthTracker::new(self.health_reporter.clone());

        'outer: loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            let current = match known.take() {
                Some(current) => current,
                None => {
                    let fetched_token = tokio::select! {
                        biased;
                        () = cancel.cancelled() => return Ok(()),
                        fetched = self.access_tokens.current() => fetched,
                    };
                    let token = match fetched_token {
                        Ok(t) => t,
                        Err(e) => {
                            health.failed();
                            tracing::warn!(
                                "access token source failed during broadcast resolution: {}",
                                crate::api::error_shape::redact_platform_error(&e)
                            );
                            tokio::select! {
                                () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                                () = cancel.cancelled() => return Ok(()),
                            }
                            continue 'outer;
                        }
                    };

                    let (live_chat_id, broadcast_id, current_title) = match self
                        .fetch_live_chat_id(&token, cancel)
                        .await
                    {
                        Ok(Some(ids)) => {
                            health.succeeded();
                            ids
                        }
                        Ok(None) | Err(FetchFailure::ChatEnded) => {
                            health.succeeded();
                            self.publish_ids(None, None, cancel);
                            self.report_viewers(ViewerReport::Absent);
                            watch.last_seen_title = None;
                            if is_live {
                                is_live = false;
                                let event = Event::new(
                                    EventSource::YouTube,
                                    event_kinds::STREAM_OFFLINE,
                                    serde_json::json!({
                                        (stream_fields::BROADCAST_ID): last_broadcast_id.clone().unwrap_or_default(),
                                    }),
                                );
                                if self.bus_sender.send(event).is_err() {
                                    return Ok(());
                                }
                            }
                            if self
                                .wait_offline(&mut offline, wake, cancel)
                                .await
                                .is_break()
                            {
                                return Ok(());
                            }
                            continue 'outer;
                        }
                        Err(FetchFailure::Cancelled) => return Ok(()),
                        Err(FetchFailure::QuotaExhausted) => {
                            tracing::warn!(
                                "quota exhausted before broadcast resolution: {}",
                                PlatformError::QuotaExhausted
                            );
                            tokio::select! {
                                () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                                () = cancel.cancelled() => return Ok(()),
                            }
                            continue 'outer;
                        }
                        Err(FetchFailure::Failed(e)) => {
                            health.failed();
                            tracing::warn!(
                                "broadcast resolution failed: {}",
                                crate::api::error_shape::redact_platform_error(&e)
                            );
                            tokio::select! {
                                () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                                () = cancel.cancelled() => return Ok(()),
                            }
                            continue 'outer;
                        }
                    };

                    offline.settle();
                    if !is_live {
                        is_live = true;
                        let event = Event::new(
                            EventSource::YouTube,
                            event_kinds::STREAM_ONLINE,
                            serde_json::json!({
                                (stream_fields::BROADCAST_TITLE): current_title,
                                (stream_fields::BROADCAST_ID): broadcast_id,
                            }),
                        );
                        if self.bus_sender.send(event).is_err() {
                            return Ok(());
                        }
                    }
                    last_broadcast_id = Some(broadcast_id.clone());
                    if !self.title_changed(&mut watch, current_title) {
                        return Ok(());
                    }
                    watch.next_check = tokio::time::Instant::now() + self.timings.broadcast_cadence;
                    KnownBroadcast {
                        broadcast_id,
                        live_chat_id,
                    }
                }
            };

            self.publish_ids(
                Some(current.live_chat_id.clone()),
                Some(current.broadcast_id.clone()),
                cancel,
            );

            let session_started_at = tokio::time::Instant::now();
            let (checked_tx, checked_rx) = tokio::sync::watch::channel(CheckedChat::Open {
                live_chat_id: current.live_chat_id.clone(),
            });

            let exit = tokio::select! {
                exit = receive.receive(
                    self,
                    ChatSession {
                        live_chat_id: &current.live_chat_id,
                        broadcast_id: &current.broadcast_id,
                        checked_chat: &checked_rx,
                        dedup: &mut dedup,
                        health: &mut health,
                        is_live: &mut is_live,
                        cancel,
                    },
                ) => exit,
                () = self.watch_broadcast_until_receive_stops(
                    &mut watch,
                    &current.broadcast_id,
                    &checked_tx,
                    cancel,
                ) => ReceiveExit::Stop,
            };
            let checked = checked_rx.borrow().clone();
            match (exit, checked) {
                (ReceiveExit::Stop, _) => return Ok(()),
                (ReceiveExit::Resolve, CheckedChat::Open { live_chat_id }) => {
                    known = Some(KnownBroadcast {
                        broadcast_id: current.broadcast_id,
                        live_chat_id,
                    });
                    continue 'outer;
                }
                (ReceiveExit::Resolve | ReceiveExit::BroadcastEnded, _) => {
                    wake.discard_pending();
                    self.publish_ids(None, None, cancel);
                    self.report_viewers(ViewerReport::Absent);
                }
            }

            tokio::select! {
                () = tokio::time::sleep_until(
                    session_started_at + self.timings.broadcast_cadence
                ) => {}
                () = cancel.cancelled() => return Ok(()),
            }
        }
    }

    fn publish_ids(
        &self,
        live_chat_id: Option<String>,
        broadcast_id: Option<String>,
        cancel: &CancellationToken,
    ) {
        self.live_chat_id.set_while_active(live_chat_id, cancel);
        self.active_broadcast_id
            .set_while_active(broadcast_id, cancel);
    }

    async fn record_observed_ban(&self, item: &serde_json::Value) {
        let now = OffsetDateTime::now_utc();
        let Some(entry) = observed_ban(item, &self.channel_id, now) else {
            return;
        };
        if let Err(error) = self.ban_ledger.upsert(&entry, now).await {
            tracing::warn!(%error, "observed youtube ban not recorded in the ban ledger");
        }
    }

    fn chat_ended_event(
        &self,
        item: &serde_json::Value,
        broadcast_id: &str,
        dedup: &mut DedupSet,
    ) -> Option<Event> {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())?;
        if !dedup.try_insert(id.to_owned()) {
            return None;
        }
        Some(Event::new(
            EventSource::YouTube,
            event_kinds::STREAM_OFFLINE,
            serde_json::json!({ (stream_fields::BROADCAST_ID): broadcast_id }),
        ))
    }

    async fn poll_floor(&self) -> Duration {
        if self
            .api
            .quota()
            .lock()
            .await
            .long_interval_mode(today_pacific())
        {
            self.timings.long_interval
        } else {
            self.timings.poll_floor
        }
    }
}

#[cfg(test)]
mod tests;
