use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::{DedupSet, ExpectedRefusal, GatewayRequest};
use forge_platform_core::PlatformError;
use forge_platform_core::TokenSource;
use forge_storage::BanLedgerRepo;
use forge_types::Secret;
use forge_types::{
    ChatEventDetail, ChatPayload, ChatSegment, ChatViewer, CurrencyCode, ModerationMarks,
    MoneyAmount, UserBadge,
};
use time::OffsetDateTime;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::api::data_api::{DataApiError, YoutubeDataApi, malformed};
use crate::api::poll_health::{PollHealthReporter, PollHealthTracker};
use crate::api::quota_state::{CHAT_POLL_COST, today_pacific};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::broadcast_lookup::{LookupFailure, fetch_active_broadcast};
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use crate::chat::ban_ledger::{USER_BANNED_EVENT_TYPE, observed_ban};
use crate::vocabulary::event_kinds;
use crate::vocabulary::fields::{
    ban as ban_fields, chat as chat_fields, chat_mod as chat_mod_fields, entity,
    gift as gift_fields, member as member_fields, stream as stream_fields,
    support as support_fields,
};

mod events;
mod fetch;
mod wire;

use self::fetch::*;
use self::wire::*;

const POLL_FLOOR_MS: u64 = 3_000;
const LONG_INTERVAL_MS: u64 = 60_000;
const BROADCAST_CADENCE_SECS: u64 = 60;
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
}

impl ChatPollTimings {
    pub const DEFAULT: Self = Self {
        poll_floor: Duration::from_millis(POLL_FLOOR_MS),
        long_interval: Duration::from_millis(LONG_INTERVAL_MS),
        broadcast_cadence: Duration::from_secs(BROADCAST_CADENCE_SECS),
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

pub struct YoutubeChatPoller {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    bus_sender: UnboundedSender<Event>,
    channel_id: String,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    health_reporter: Option<PollHealthReporter>,
    timings: ChatPollTimings,
}

impl YoutubeChatPoller {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        bus_sender: UnboundedSender<Event>,
        channel_id: String,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        Self {
            api,
            access_token_source,
            bus_sender,
            channel_id,
            live_chat_id,
            active_broadcast_id,
            ban_ledger,
            health_reporter: None,
            timings: ChatPollTimings::DEFAULT,
        }
    }

    pub fn with_timings(mut self, timings: ChatPollTimings) -> Self {
        self.timings = timings;
        self
    }

    pub(crate) fn with_health_reporter(mut self, reporter: PollHealthReporter) -> Self {
        self.health_reporter = Some(reporter);
        self
    }

    pub async fn run_until_stopped(self, stop: ChatPollerStop) -> Result<(), PlatformError> {
        self.run(stop.0).await
    }

    pub(crate) async fn run(self, cancel: CancellationToken) -> Result<(), PlatformError> {
        let mut dedup = DedupSet::bounded(DEDUP_WINDOW_SIZE);
        let mut last_seen_title: Option<String> = None;
        let mut last_broadcast_id: Option<String> = None;
        let mut is_live = false;
        let mut health = PollHealthTracker::new(self.health_reporter.clone());

        'outer: loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            let fetched_token = tokio::select! {
                biased;
                () = cancel.cancelled() => return Ok(()),
                fetched = (self.access_token_source)() => fetched,
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
                .fetch_live_chat_id(&token, &cancel)
                .await
            {
                Ok(Some(ids)) => {
                    health.succeeded();
                    ids
                }
                Ok(None) | Err(FetchFailure::ChatEnded) => {
                    health.succeeded();
                    self.publish_ids(None, None, &cancel);
                    last_seen_title = None;
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
                    tokio::select! {
                        () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                        () = cancel.cancelled() => return Ok(()),
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

            if let Some(prev) = last_seen_title.as_deref()
                && prev != current_title
                && self
                    .bus_sender
                    .send(Event::new(
                        EventSource::YouTube,
                        event_kinds::STREAM_TITLE_CHANGED,
                        serde_json::json!({
                            (stream_fields::TITLE): {
                                (stream_fields::OLD): prev,
                                (stream_fields::NEW): current_title,
                            },
                        }),
                    ))
                    .is_err()
            {
                return Ok(());
            }
            last_seen_title = Some(current_title);

            self.publish_ids(
                Some(live_chat_id.clone()),
                Some(broadcast_id.clone()),
                &cancel,
            );

            let mut next_page_token: Option<String> = None;
            let broadcast_resolved_at = tokio::time::Instant::now();

            let mut broadcast_ended = false;
            'inner: loop {
                if cancel.is_cancelled() {
                    return Ok(());
                }

                if broadcast_resolved_at.elapsed() >= self.timings.broadcast_cadence {
                    break 'inner;
                }

                let fetched_token = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Ok(()),
                    fetched = (self.access_token_source)() => fetched,
                };
                let token = match fetched_token {
                    Ok(t) => t,
                    Err(e) => {
                        health.failed();
                        tracing::warn!(
                            "access token fetch failed during chat poll: {}",
                            crate::api::error_shape::redact_platform_error(&e)
                        );
                        tokio::select! {
                            () = tokio::time::sleep(self.poll_floor().await) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        continue 'inner;
                    }
                };

                let fetched = self
                    .fetch_chat_messages(&token, &live_chat_id, next_page_token.as_deref(), &cancel)
                    .await;
                let floor = self.poll_floor().await;
                let response = match fetched {
                    Ok(r) => {
                        health.succeeded();
                        r
                    }
                    Err(FetchFailure::Cancelled) => return Ok(()),
                    Err(FetchFailure::ChatEnded) => {
                        health.succeeded();
                        broadcast_ended = true;
                        break 'inner;
                    }
                    Err(FetchFailure::QuotaExhausted) => {
                        tracing::warn!(
                            "quota exhausted before chat poll: {}",
                            PlatformError::QuotaExhausted
                        );
                        tokio::select! {
                            () = tokio::time::sleep(self.timings.broadcast_cadence) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        break 'inner;
                    }
                    Err(FetchFailure::Failed(e)) => {
                        health.failed();
                        tracing::warn!(
                            "chat messages fetch failed: {}",
                            crate::api::error_shape::redact_platform_error(&e)
                        );
                        tokio::select! {
                            () = tokio::time::sleep(floor) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        continue 'inner;
                    }
                };

                let mut chat_ended = false;
                for item in &response.items {
                    if item_type(item) == CHAT_ENDED_EVENT_TYPE {
                        chat_ended = true;
                        if let Some(event) = self.chat_ended_event(item, &broadcast_id, &mut dedup)
                        {
                            is_live = false;
                            if self.bus_sender.send(event).is_err() {
                                return Ok(());
                            }
                        }
                        continue;
                    }
                    if let Some(event) = self.build_event(item, &mut dedup) {
                        if item_type(item) == USER_BANNED_EVENT_TYPE {
                            self.record_observed_ban(item).await;
                        }
                        if self.bus_sender.send(event).is_err() {
                            return Ok(());
                        }
                    }
                }

                if chat_ended {
                    broadcast_ended = true;
                    break 'inner;
                }

                next_page_token = response.next_page_token;

                let computed_sleep = sleep_duration(
                    response.polling_interval_millis,
                    u64::try_from(floor.as_millis()).unwrap_or(u64::MAX),
                );

                tokio::select! {
                    () = tokio::time::sleep(computed_sleep) => {}
                    () = cancel.cancelled() => return Ok(()),
                }
            }

            if broadcast_ended {
                self.publish_ids(None, None, &cancel);
            }

            tokio::select! {
                () = tokio::time::sleep_until(
                    broadcast_resolved_at + self.timings.broadcast_cadence
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
