use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::{DedupSet, PlatformError};
use forge_types::{ChatEventDetail, ChatPayload, ChatSegment, ModerationMarks, UserBadge};
use futures::future::BoxFuture;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::live_chat_id::LiveChatIdHandle;
use crate::payload_fields::{
    ban as ban_fields, chat as chat_fields, chat_mod as chat_mod_fields, entity,
    gift as gift_fields, member as member_fields, stream as stream_fields,
    support as support_fields,
};
use crate::quota_state::{BROADCAST_COST, CHAT_POLL_COST, QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const POLL_FLOOR_MS: u64 = 3_000;
const LONG_INTERVAL_MS: u64 = 60_000;
const BROADCAST_CADENCE_SECS: u64 = 60;
const DEDUP_WINDOW_SIZE: usize = 500;

struct ChatMessagesResponse {
    items: Vec<serde_json::Value>,
    next_page_token: Option<String>,
    polling_interval_millis: u64,
}

pub struct YoutubeChatPoller {
    client: reqwest::Client,
    access_token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    bus_sender: UnboundedSender<Event>,
    channel_id: String,
    api_base: String,
    quota_tracker: Arc<Mutex<QuotaState>>,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
}

impl YoutubeChatPoller {
    pub fn new(
        access_token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        bus_sender: UnboundedSender<Event>,
        channel_id: String,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota_tracker: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            bus_sender,
            channel_id,
            api_base: DEFAULT_API_BASE.to_owned(),
            quota_tracker,
            live_chat_id,
            active_broadcast_id,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn run(self, cancel: CancellationToken) -> Result<(), PlatformError> {
        let mut dedup = DedupSet::bounded(DEDUP_WINDOW_SIZE);
        let mut last_seen_title: Option<String> = None;
        let mut last_broadcast_id: Option<String> = None;
        let mut is_live = false;

        'outer: loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            {
                let today = today_pacific();
                let mut qt = self.quota_tracker.lock().await;
                if let Err(e) = qt.charge(BROADCAST_COST, today) {
                    tracing::warn!("quota exhausted before broadcast resolution: {e}");
                    drop(qt);
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
            }

            let token = match (self.access_token_source)().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("access token source failed during broadcast resolution: {e}");
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
            };

            let (live_chat_id, broadcast_id, current_title) = match self
                .fetch_live_chat_id(&token)
                .await
            {
                Ok(Some(ids)) => ids,
                Ok(None) => {
                    self.live_chat_id.set(None);
                    self.active_broadcast_id.set(None);
                    // Resetting here means a later go-live with a different title is a fresh
                    // session, not an edit.
                    last_seen_title = None;
                    if is_live {
                        is_live = false;
                        let event = Event::new(
                            EventSource::YouTube,
                            "youtube.stream.offline",
                            serde_json::json!({
                                (stream_fields::BROADCAST_ID): last_broadcast_id.clone().unwrap_or_default(),
                            }),
                        );
                        if self.bus_sender.send(event).is_err() {
                            return Ok(());
                        }
                    }
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
                Err(e) => {
                    self.live_chat_id.set(None);
                    self.active_broadcast_id.set(None);
                    last_seen_title = None;
                    tracing::warn!("broadcast resolution failed: {e}");
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
            };

            if !is_live {
                is_live = true;
                let event = Event::new(
                    EventSource::YouTube,
                    "youtube.stream.online",
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
                        "youtube.stream.title_changed",
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

            self.live_chat_id.set(Some(live_chat_id.clone()));
            self.active_broadcast_id.set(Some(broadcast_id));

            let mut next_page_token: Option<String> = None;
            let broadcast_resolved_at = tokio::time::Instant::now();

            'inner: loop {
                if cancel.is_cancelled() {
                    return Ok(());
                }

                if broadcast_resolved_at.elapsed() >= Duration::from_secs(BROADCAST_CADENCE_SECS) {
                    break 'inner;
                }

                let floor = {
                    let qt = self.quota_tracker.lock().await;
                    if qt.long_interval_mode {
                        Duration::from_millis(LONG_INTERVAL_MS)
                    } else {
                        Duration::from_millis(POLL_FLOOR_MS)
                    }
                };

                {
                    let today = today_pacific();
                    let mut qt = self.quota_tracker.lock().await;
                    if let Err(e) = qt.charge(CHAT_POLL_COST, today) {
                        tracing::warn!("quota exhausted before chat poll: {e}");
                        drop(qt);
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        break 'inner;
                    }
                }

                let token = match (self.access_token_source)().await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("access token fetch failed during chat poll: {e}");
                        tokio::select! {
                            () = tokio::time::sleep(floor) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        continue 'inner;
                    }
                };

                let response = match self
                    .fetch_chat_messages(&token, &live_chat_id, next_page_token.as_deref())
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("chat messages fetch failed: {e}");
                        tokio::select! {
                            () = tokio::time::sleep(floor) => {}
                            () = cancel.cancelled() => return Ok(()),
                        }
                        continue 'inner;
                    }
                };

                for item in &response.items {
                    if let Some(event) = self.build_event(item, &mut dedup) {
                        if event.kind == "youtube.stream.offline" {
                            is_live = false;
                        }
                        if self.bus_sender.send(event).is_err() {
                            return Ok(());
                        }
                    }
                }

                next_page_token = response.next_page_token;

                let computed_sleep =
                    sleep_duration(response.polling_interval_millis, floor.as_millis() as u64);

                tokio::select! {
                    () = tokio::time::sleep(computed_sleep) => {}
                    () = cancel.cancelled() => return Ok(()),
                }
            }

            self.live_chat_id.set(None);
            self.active_broadcast_id.set(None);
        }
    }

    /// Resolves `liveChatId`, the video id, and current title in one call - no extra quota.
    async fn fetch_live_chat_id(
        &self,
        token: &str,
    ) -> Result<Option<(String, String, String)>, PlatformError> {
        let url = format!("{}/liveBroadcasts", self.api_base);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .query(&[
                ("part", "snippet,contentDetails"),
                ("broadcastStatus", "active"),
                ("broadcastType", "all"),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body = resp.text().await.unwrap_or_default();
            return Err(PlatformError::Http { status, body });
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());

        let live_chat_id = item
            .and_then(|item| item.get("snippet"))
            .and_then(|snippet| snippet.get("liveChatId"))
            .and_then(|v| v.as_str());
        let broadcast_id = item
            .and_then(|item| item.get("id"))
            .and_then(|v| v.as_str());
        let title = item
            .and_then(|item| item.get("snippet"))
            .and_then(|snippet| snippet.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        Ok(match (live_chat_id, broadcast_id) {
            (Some(lc), Some(b)) => Some((lc.to_owned(), b.to_owned(), title.to_owned())),
            _ => None,
        })
    }

    async fn fetch_chat_messages(
        &self,
        token: &str,
        live_chat_id: &str,
        page_token: Option<&str>,
    ) -> Result<ChatMessagesResponse, PlatformError> {
        let url = format!("{}/liveChat/messages", self.api_base);

        let mut query: Vec<(&str, String)> = vec![
            ("liveChatId", live_chat_id.to_owned()),
            ("part", "snippet,authorDetails".to_owned()),
            ("maxResults", "200".to_owned()),
        ];
        if let Some(pt) = page_token {
            query.push(("pageToken", pt.to_owned()));
        }

        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .query(&query)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = resp.status().as_u16();
        if status != 200 {
            let body = resp.text().await.unwrap_or_default();
            return Err(PlatformError::Http { status, body });
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;

        let items = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let next_page_token = body
            .get("nextPageToken")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned);

        let polling_interval_millis = body
            .get("pollingIntervalMillis")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(5_000);

        Ok(ChatMessagesResponse {
            items,
            next_page_token,
            polling_interval_millis,
        })
    }

    fn build_event(&self, item: &serde_json::Value, dedup: &mut DedupSet) -> Option<Event> {
        let id = item
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)?;

        if !dedup.try_insert(id.clone()) {
            return None;
        }

        let snippet = item.get("snippet")?;
        let author_details = item.get("authorDetails");
        let msg_type = snippet.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "chatEndedEvent" => {
                let event = Event::new(
                    EventSource::YouTube,
                    "youtube.stream.offline",
                    serde_json::json!({ (stream_fields::BROADCAST_ID): id }),
                );
                Some(event)
            }

            "textMessageEvent" => {
                let text = snippet
                    .get("displayMessage")
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        snippet
                            .get("textMessageDetails")
                            .and_then(|d| d.get("messageText"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_owned();

                let is_command = text.starts_with('!');
                let kind = if is_command {
                    "youtube.chat.command"
                } else {
                    "youtube.chat.message"
                };

                let author = extract_author(author_details);
                let badges = extract_badges(author_details);

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: author.clone(),
                    author_color: None,
                    segments: vec![ChatSegment::Text { text: text.clone() }],
                    badges,
                    is_event: false,
                    event_detail: None,
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (chat_fields::MESSAGE_TEXT): text,
                    (chat_fields::AUTHOR): author_json(author_details),
                    (chat_fields::BROADCASTER_CHANNEL_ID): self.channel_id,
                });

                if is_command {
                    let (cmd_name, args) = parse_command(&text);
                    payload[chat_fields::COMMAND_NAME] = serde_json::Value::String(cmd_name);
                    payload[chat_fields::ARGS] =
                        serde_json::Value::Array(args.into_iter().map(Into::into).collect());
                }

                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(EventSource::YouTube, kind, payload))
            }

            "superChatEvent" => {
                let details = snippet.get("superChatDetails");
                let amount_micros = details
                    .and_then(|d| d.get("amountMicros"))
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                let currency = details
                    .and_then(|d| d.get("currency"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let message = details
                    .and_then(|d| d.get("userComment"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned);

                let author = extract_author(author_details);
                let badges = extract_badges(author_details);
                let event_detail = ChatEventDetail::SuperChat {
                    amount_micros,
                    currency: currency.clone(),
                    message: message.clone(),
                };

                let segments = message
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|t| vec![ChatSegment::Text { text: t.to_owned() }])
                    .unwrap_or_default();

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: author.clone(),
                    author_color: None,
                    segments,
                    badges,
                    is_event: true,
                    event_detail: Some(event_detail),
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (chat_fields::AUTHOR): author_json(author_details),
                    (support_fields::AMOUNT_MICROS): amount_micros,
                    (support_fields::CURRENCY): currency,
                    (chat_fields::MESSAGE_TEXT): message,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.chat.super_chat",
                    payload,
                ))
            }

            "superStickerEvent" => {
                let details = snippet.get("superStickerDetails");
                let amount_micros = details
                    .and_then(|d| d.get("amountMicros"))
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                let currency = details
                    .and_then(|d| d.get("currency"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let sticker_id = details
                    .and_then(|d| d.get("superStickerMetadata"))
                    .and_then(|m| m.get("stickerId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();

                let author = extract_author(author_details);
                let badges = extract_badges(author_details);
                let event_detail = ChatEventDetail::SuperChat {
                    amount_micros,
                    currency: currency.clone(),
                    message: None,
                };

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: author.clone(),
                    author_color: None,
                    segments: vec![],
                    badges,
                    is_event: true,
                    event_detail: Some(event_detail),
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (chat_fields::AUTHOR): author_json(author_details),
                    (support_fields::STICKER_ID): sticker_id,
                    (support_fields::AMOUNT_MICROS): amount_micros,
                    (support_fields::CURRENCY): currency,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.chat.super_sticker",
                    payload,
                ))
            }

            "newSponsorEvent" => {
                let level = snippet
                    .get("newSponsorDetails")
                    .and_then(|d| d.get("memberLevelName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();

                let author = extract_author(author_details);
                let badges = extract_badges(author_details);
                let event_detail = ChatEventDetail::NewMember {
                    level: level.clone(),
                };

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: author.clone(),
                    author_color: None,
                    segments: vec![],
                    badges,
                    is_event: true,
                    event_detail: Some(event_detail),
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (chat_fields::AUTHOR): author_json(author_details),
                    (member_fields::MEMBER_LEVEL_NAME): level,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.member",
                    payload,
                ))
            }

            "memberMilestoneChatEvent" => {
                let details = snippet.get("memberMilestoneChatDetails");
                let months = details
                    .and_then(|d| d.get("memberMonth"))
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0) as u32;
                let message = details
                    .and_then(|d| d.get("userComment"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned);

                let author = extract_author(author_details);
                let badges = extract_badges(author_details);
                let event_detail = ChatEventDetail::MemberMilestone {
                    months,
                    message: message.clone(),
                };

                let segments = message
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|t| vec![ChatSegment::Text { text: t.to_owned() }])
                    .unwrap_or_default();

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: author.clone(),
                    author_color: None,
                    segments,
                    badges,
                    is_event: true,
                    event_detail: Some(event_detail),
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (chat_fields::AUTHOR): author_json(author_details),
                    (member_fields::MEMBER_MONTH): months,
                    (chat_fields::MESSAGE_TEXT): message,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.member_milestone",
                    payload,
                ))
            }

            "userBannedEvent" => {
                let banned_details = snippet.get("userBannedDetails");
                let banned_user = banned_details.and_then(|d| d.get("bannedUserDetails"));
                let target_display_name = non_empty(
                    banned_user
                        .and_then(|u| u.get("displayName"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                );
                let target_channel_id = banned_user
                    .and_then(|u| u.get("channelId"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .filter(|s| !s.is_empty());

                let ban_type = banned_details
                    .and_then(|d| d.get("banType"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("permanent")
                    .to_owned();
                let is_temporary = ban_type.eq_ignore_ascii_case("temporary");
                let ban_duration_secs: Option<i64> = if is_temporary {
                    banned_details
                        .and_then(|d| d.get("banDurationSeconds"))
                        .and_then(|v| {
                            v.as_u64()
                                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                        })
                        .map(|v| v as i64)
                } else {
                    None
                };

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: target_display_name.clone().unwrap_or_default(),
                    author_color: None,
                    segments: vec![],
                    badges: vec![],
                    is_event: true,
                    event_detail: None,
                    moderation: ModerationMarks {
                        timed_out: is_temporary,
                        banned: !is_temporary,
                        deleted: false,
                    },
                };

                let mut payload = serde_json::json!({
                    (ban_fields::TARGET_USER): entity_json(target_channel_id, target_display_name),
                    (ban_fields::MODERATOR): author_json(author_details),
                    (ban_fields::TYPE): ban_type,
                    (ban_fields::DURATION_SECS): ban_duration_secs,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.user_banned",
                    payload,
                ))
            }

            "tombstone" => {
                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: String::new(),
                    author_color: None,
                    segments: vec![],
                    badges: vec![],
                    is_event: true,
                    event_detail: None,
                    moderation: ModerationMarks {
                        timed_out: false,
                        banned: false,
                        deleted: true,
                    },
                };

                let mut payload = serde_json::json!({
                    (chat_mod_fields::MESSAGE_ID): id,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.chat.message_deleted",
                    payload,
                ))
            }

            "membershipGiftingEvent" => {
                let details = snippet.get("membershipGiftingDetails");
                let count = details
                    .and_then(|d| d.get("giftMembershipsCount"))
                    .and_then(|v| {
                        v.as_i64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);
                let level_name = details
                    .and_then(|d| d.get("giftMembershipsLevelName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();

                let gifter_display_name = extract_author(author_details);

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: gifter_display_name.clone(),
                    author_color: None,
                    segments: vec![],
                    badges: extract_badges(author_details),
                    is_event: true,
                    event_detail: None,
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (gift_fields::COUNT): count,
                    (gift_fields::LEVEL_NAME): level_name,
                    (gift_fields::GIFTER): author_json(author_details),
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.member_gift",
                    payload,
                ))
            }

            "giftMembershipReceivedEvent" => {
                let details = snippet.get("giftMembershipReceivedDetails");
                let level_name = details
                    .and_then(|d| d.get("memberLevelName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let gifter_channel_id = details
                    .and_then(|d| d.get("gifterChannelId"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .filter(|s| !s.is_empty());

                let recipient_display_name = extract_author(author_details);

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: recipient_display_name.clone(),
                    author_color: None,
                    segments: vec![],
                    badges: extract_badges(author_details),
                    is_event: true,
                    event_detail: None,
                    moderation: ModerationMarks::default(),
                };

                let mut payload = serde_json::json!({
                    (gift_fields::LEVEL_NAME): level_name,
                    (gift_fields::GIFTER): entity_json(gifter_channel_id, None),
                    (gift_fields::RECIPIENT): author_json(author_details),
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.member_gift_received",
                    payload,
                ))
            }

            _ => None,
        }
    }
}

fn sleep_duration(polling_interval_millis: u64, floor_ms: u64) -> Duration {
    Duration::from_millis(polling_interval_millis).max(Duration::from_millis(floor_ms))
}

fn parse_command(text: &str) -> (String, Vec<String>) {
    let stripped = text.strip_prefix('!').unwrap_or(text);
    let mut parts = stripped.split_whitespace();
    let cmd = parts.next().unwrap_or("").to_owned();
    let args = parts.map(ToOwned::to_owned).collect();
    (cmd, args)
}

fn extract_author(author_details: Option<&serde_json::Value>) -> String {
    author_details
        .and_then(|ad| ad.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

fn extract_author_channel_id(author_details: Option<&serde_json::Value>) -> Option<String> {
    author_details
        .and_then(|ad| ad.get("channelId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn entity_json(channel_id: Option<String>, display_name: Option<String>) -> serde_json::Value {
    serde_json::json!({
        (entity::CHANNEL_ID): channel_id,
        (entity::DISPLAY_NAME): display_name,
    })
}

fn author_json(author_details: Option<&serde_json::Value>) -> serde_json::Value {
    entity_json(
        extract_author_channel_id(author_details),
        non_empty(extract_author(author_details)),
    )
}

fn extract_badges(author_details: Option<&serde_json::Value>) -> Vec<UserBadge> {
    let ad = match author_details {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut badges = Vec::new();
    if ad
        .get("isChatOwner")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Broadcaster);
    }
    if ad
        .get("isChatModerator")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Moderator);
    }
    if ad
        .get("isChatSponsor")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Member {
            level: "member".to_owned(),
        });
    }
    badges
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::QuotaState;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn broadcast_response(live_chat_id: &str) -> serde_json::Value {
        json!({
            "items": [{
                "id": "broadcast-1",
                "snippet": {
                    "liveChatId": live_chat_id,
                    "title": "Test Stream"
                }
            }]
        })
    }

    fn empty_broadcast_response() -> serde_json::Value {
        json!({ "items": [] })
    }

    fn chat_response(items: serde_json::Value, polling_ms: u64) -> serde_json::Value {
        json!({
            "pollingIntervalMillis": polling_ms,
            "items": items
        })
    }

    fn chat_response_with_page(
        items: serde_json::Value,
        polling_ms: u64,
        next_page_token: &str,
    ) -> serde_json::Value {
        json!({
            "pollingIntervalMillis": polling_ms,
            "nextPageToken": next_page_token,
            "items": items
        })
    }

    fn text_item(id: &str, text: &str, display_name: &str) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "type": "textMessageEvent",
                "displayMessage": text
            },
            "authorDetails": {
                "displayName": display_name,
                "channelId": "UCviewer",
                "isChatOwner": false,
                "isChatModerator": false,
                "isChatSponsor": false
            }
        })
    }

    async fn mount_broadcast_mock(server: &MockServer, response: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(server)
            .await;
    }

    async fn mount_chat_mock(server: &MockServer, response: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(server)
            .await;
    }

    fn make_quota() -> Arc<tokio::sync::Mutex<QuotaState>> {
        Arc::new(tokio::sync::Mutex::new(QuotaState::default()))
    }

    fn make_poller(server: &MockServer) -> (YoutubeChatPoller, UnboundedSender<Event>) {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx.clone(),
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            make_quota(),
        )
        .with_api_base(server.uri());
        (poller, tx)
    }

    fn make_poller_with_receiver(
        server: &MockServer,
    ) -> (
        YoutubeChatPoller,
        tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            make_quota(),
        )
        .with_api_base(server.uri());
        (poller, rx)
    }

    async fn assert_leading_online(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) {
        let online = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(online.kind, "youtube.stream.online");
    }

    async fn broadcast_poll_count(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .map(|reqs| {
                reqs.iter()
                    .filter(|r| r.url.path() == "/liveBroadcasts")
                    .count()
            })
            .unwrap_or(0)
    }

    async fn drain_events_over_broadcast_cycles(
        poller: YoutubeChatPoller,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
        server: &MockServer,
        min_broadcast_polls: usize,
    ) -> Vec<Event> {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        for _ in 0..5_000 {
            if broadcast_poll_count(server).await >= min_broadcast_polls {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        cancel.cancel();
        join.await.unwrap().unwrap();

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        events
    }

    #[tokio::test]
    async fn text_message_event_emits_youtube_chat_message() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-abc")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([text_item("msg-1", "hello world", "StreamFan")]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.chat.message");
        assert_eq!(event.source, EventSource::YouTube);

        let chat: ChatPayload =
            serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
        assert_eq!(chat.author, "StreamFan");
        assert_eq!(chat.platform_msg_id, "msg-1");
        assert!(!chat.is_event);
        assert_eq!(
            chat.segments,
            vec![ChatSegment::Text {
                text: "hello world".to_owned()
            }]
        );
    }

    #[tokio::test]
    async fn chat_message_payload_carries_author_entity_from_author_details() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-author")).await;
        mount_chat_mock(
            &server,
            chat_response(json!([text_item("msg-a", "hi", "StreamFan")]), 3000),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(
            event.payload["author"]["display_name"].as_str().unwrap(),
            "StreamFan"
        );
        assert_eq!(
            event.payload["author"]["channel_id"].as_str().unwrap(),
            "UCviewer"
        );
    }

    #[tokio::test]
    async fn text_message_command_emits_youtube_chat_command() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-cmd")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([text_item("cmd-1", "!shoutout user123", "Chatter")]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.chat.command");
        assert_eq!(event.payload["command_name"].as_str().unwrap(), "shoutout");
        let args = event.payload["args"].as_array().unwrap();
        assert_eq!(
            args.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
            vec!["user123"]
        );
    }

    #[tokio::test]
    async fn super_chat_event_emits_with_event_detail_super_chat() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-sc")).await;

        let item = json!({
            "id": "sc-1",
            "snippet": {
                "type": "superChatEvent",
                "superChatDetails": {
                    "amountMicros": "5000000",
                    "currency": "USD",
                    "userComment": "great stream!"
                }
            },
            "authorDetails": {
                "displayName": "SuperFan",
                "isChatOwner": false,
                "isChatModerator": false,
                "isChatSponsor": false
            }
        });
        mount_chat_mock(&server, chat_response(json!([item]), 3000)).await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.chat.super_chat");

        let chat: ChatPayload =
            serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
        assert!(chat.is_event);
        assert_eq!(chat.author, "SuperFan");

        match chat.event_detail.unwrap() {
            ChatEventDetail::SuperChat {
                amount_micros,
                currency,
                message,
            } => {
                assert_eq!(amount_micros, 5_000_000);
                assert_eq!(currency, "USD");
                assert_eq!(message, Some("great stream!".to_owned()));
            }
            other => panic!("expected SuperChat, got {other:?}"),
        }
    }

    fn banned_item(
        id: &str,
        display_name: &str,
        ban_type: &str,
        duration_secs: serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "type": "userBannedEvent",
                "userBannedDetails": {
                    "bannedUserDetails": {
                        "displayName": display_name,
                        "channelId": "UCbanned"
                    },
                    "banType": ban_type,
                    "banDurationSeconds": duration_secs
                }
            },
            "authorDetails": {
                "displayName": "ModAlice",
                "channelId": "UCmodalice"
            }
        })
    }

    #[tokio::test]
    async fn user_banned_with_duration_emits_temporary_ban() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-temp")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([banned_item("ban-1", "Troll", "temporary", json!(600))]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.channel.user_banned");
        assert_eq!(event.payload["type"].as_str().unwrap(), "temporary");
        assert_eq!(event.payload["duration_secs"].as_i64().unwrap(), 600);
        assert_eq!(
            event.payload["target_user"]["display_name"]
                .as_str()
                .unwrap(),
            "Troll"
        );
    }

    #[tokio::test]
    async fn permanent_ban_carries_null_duration_not_zero_placeholder() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-perm")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([banned_item("ban-2", "Spammer", "permanent", json!(null))]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.channel.user_banned");
        assert_eq!(event.payload["type"].as_str().unwrap(), "permanent");
        assert!(event.payload["duration_secs"].is_null());
    }

    #[tokio::test]
    async fn user_banned_populates_moderator_from_author_details() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-mod")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([banned_item("ban-3", "Troll", "permanent", json!(null))]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(
            event.payload["moderator"]["channel_id"].as_str().unwrap(),
            "UCmodalice"
        );
        assert_eq!(
            event.payload["moderator"]["display_name"].as_str().unwrap(),
            "ModAlice"
        );
    }

    #[test]
    fn polling_interval_millis_respected() {
        let s = sleep_duration(5000, POLL_FLOOR_MS);
        assert_eq!(
            s,
            Duration::from_millis(5000),
            "pollingIntervalMillis > floor must be honored"
        );
    }

    #[test]
    fn polling_interval_floor_3s() {
        let s = sleep_duration(1000, POLL_FLOOR_MS);
        assert_eq!(
            s,
            Duration::from_millis(POLL_FLOOR_MS),
            "pollingIntervalMillis below floor must use the 3s floor"
        );
    }

    #[tokio::test]
    async fn dedup_via_next_page_token() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(broadcast_response("chat-dedup")),
            )
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/liveChat/messages"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(chat_response_with_page(
                    json!([
                        text_item("id1", "hello", "User1"),
                        text_item("id2", "world", "User2")
                    ]),
                    0,
                    "ptX",
                )),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/liveChat/messages"))
            .and(query_param("pageToken", "ptX"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(
                json!([
                    text_item("id2", "world", "User2"),
                    text_item("id3", "again", "User3")
                ]),
                0,
            )))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response(json!([]), 0)))
            .mount(&server)
            .await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            make_quota(),
        )
        .with_api_base(server.uri());

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        assert_leading_online(&mut rx).await;

        let mut events = Vec::new();
        let timeout_result = tokio::time::timeout(Duration::from_secs(10), async {
            while events.len() < 3 {
                if let Some(e) = rx.recv().await {
                    events.push(e);
                } else {
                    break;
                }
            }
        })
        .await;

        cancel.cancel();
        handle.await.unwrap().unwrap();

        assert!(
            timeout_result.is_ok(),
            "timed out waiting for 3 unique events"
        );

        let ids: Vec<String> = events
            .iter()
            .filter_map(|e| {
                e.payload
                    .get(ChatPayload::KEY)
                    .and_then(|c| c.get("platform_msg_id"))
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned)
            })
            .collect();

        assert_eq!(
            ids.len(),
            3,
            "expected exactly 3 unique events, got: {ids:?}"
        );
        assert!(ids.contains(&"id1".to_owned()));
        assert!(ids.contains(&"id2".to_owned()));
        assert!(ids.contains(&"id3".to_owned()));
    }

    #[tokio::test]
    async fn cancel_token_breaks_loop() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-cancel")).await;
        mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

        let (poller, _rx) = make_poller(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_secs(3), handle)
            .await
            .expect("run() did not return within timeout after cancel")
            .unwrap();

        assert!(result.is_ok(), "run() must return Ok(()) on cancellation");
    }

    #[allow(clippy::type_complexity)]
    fn make_poller_with_both_handles(
        server: &MockServer,
    ) -> (
        YoutubeChatPoller,
        LiveChatIdHandle,
        ActiveBroadcastIdHandle,
        tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let live = LiveChatIdHandle::new();
        let broadcast = ActiveBroadcastIdHandle::new();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            live.clone(),
            broadcast.clone(),
            make_quota(),
        )
        .with_api_base(server.uri());
        (poller, live, broadcast, rx)
    }

    #[tokio::test]
    async fn active_broadcast_sets_broadcast_id_to_items_first_id_alongside_live_chat_id() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("lc-active-xyz")).await;
        mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

        let (poller, live, broadcast, _rx) = make_poller_with_both_handles(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        let (lc, bc) = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let (Some(lc), Some(bc)) = (live.get(), broadcast.get()) {
                    return (lc, bc);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("both handles were not set within timeout");

        cancel.cancel();
        join.await.unwrap().unwrap();

        assert_eq!(lc, "lc-active-xyz");
        assert_eq!(bc, "broadcast-1");
    }

    #[tokio::test]
    async fn absent_broadcast_clears_both_handles_and_emits_nothing_when_never_live() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, empty_broadcast_response()).await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let live = LiveChatIdHandle::new();
        live.set(Some("stale-lc".to_owned()));
        let broadcast = ActiveBroadcastIdHandle::new();
        broadcast.set(Some("stale-bc".to_owned()));

        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            live.clone(),
            broadcast.clone(),
            make_quota(),
        )
        .with_api_base(server.uri());

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if live.get().is_none() && broadcast.get().is_none() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("handles were not both cleared within timeout");

        cancel.cancel();
        join.await.unwrap().unwrap();

        assert!(
            rx.try_recv().is_err(),
            "a poller that was never live must emit no lifecycle event"
        );
    }

    fn tombstone_item(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": { "type": "tombstone" }
        })
    }

    fn legacy_message_deleted_item(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "type": "messageDeletedEvent",
                "messageDeletedDetails": {
                    "deletedMessageId": "removed-msg"
                }
            },
            "authorDetails": {
                "displayName": "ModName",
                "channelId": "UCmod",
                "isChatOwner": false,
                "isChatModerator": true,
                "isChatSponsor": false
            }
        })
    }

    fn membership_gifting_item(
        id: &str,
        count: serde_json::Value,
        level_name: &str,
        gifter_display_name: &str,
        gifter_channel_id: &str,
    ) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "type": "membershipGiftingEvent",
                "membershipGiftingDetails": {
                    "giftMembershipsCount": count,
                    "giftMembershipsLevelName": level_name
                }
            },
            "authorDetails": {
                "displayName": gifter_display_name,
                "channelId": gifter_channel_id,
                "isChatOwner": false,
                "isChatModerator": false,
                "isChatSponsor": true
            }
        })
    }

    fn gift_received_item(
        id: &str,
        level_name: &str,
        recipient_display_name: &str,
        recipient_channel_id: &str,
        gifter_channel_id: serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": {
                "type": "giftMembershipReceivedEvent",
                "giftMembershipReceivedDetails": {
                    "memberLevelName": level_name,
                    "gifterChannelId": gifter_channel_id
                }
            },
            "authorDetails": {
                "displayName": recipient_display_name,
                "channelId": recipient_channel_id,
                "isChatOwner": false,
                "isChatModerator": false,
                "isChatSponsor": true
            }
        })
    }

    async fn first_event_from(server: &MockServer) -> Event {
        let (poller, mut rx) = make_poller_with_receiver(server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        assert_leading_online(&mut rx).await;
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();

        cancel.cancel();
        handle.await.unwrap().unwrap();
        event
    }

    #[tokio::test]
    async fn tombstone_emits_message_deleted_with_message_id_and_deleted_mark() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-del")).await;
        mount_chat_mock(
            &server,
            chat_response(json!([tombstone_item("del-1")]), 3000),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.chat.message_deleted");
        assert_eq!(event.payload["message_id"].as_str().unwrap(), "del-1");

        let chat: ChatPayload =
            serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
        assert!(chat.moderation.deleted);
        assert!(!chat.moderation.banned);
        assert!(!chat.moderation.timed_out);
    }

    #[tokio::test]
    async fn legacy_message_deleted_wire_type_produces_no_event() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-del-legacy")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([
                    legacy_message_deleted_item("ignored-1"),
                    text_item("keep-1", "still here", "Bystander")
                ]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.chat.message");
        let chat: ChatPayload =
            serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
        assert_eq!(chat.platform_msg_id, "keep-1");
    }

    #[tokio::test]
    async fn membership_gifting_event_emits_count_level_and_gifter() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-gift")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([membership_gifting_item(
                    "gift-1",
                    json!(5),
                    "Diamond",
                    "Generous",
                    "UCgifter"
                )]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.channel.member_gift");
        assert_eq!(event.payload["count"].as_i64().unwrap(), 5);
        assert_eq!(event.payload["level_name"].as_str().unwrap(), "Diamond");
        assert_eq!(
            event.payload["gifter"]["channel_id"].as_str().unwrap(),
            "UCgifter"
        );
        assert_eq!(
            event.payload["gifter"]["display_name"].as_str().unwrap(),
            "Generous"
        );
    }

    #[tokio::test]
    async fn membership_gifting_event_parses_string_count_into_int() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-gift-str")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([membership_gifting_item(
                    "gift-2",
                    json!("12"),
                    "Gold",
                    "Patron",
                    "UCpatron"
                )]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.payload["count"].as_i64().unwrap(), 12);
    }

    #[tokio::test]
    async fn gift_membership_received_yields_null_gifter_when_wire_absent() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-gift-recv")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([gift_received_item(
                    "recv-1",
                    "Gold",
                    "LuckyViewer",
                    "UCrecipient",
                    json!(null)
                )]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(event.kind, "youtube.channel.member_gift_received");
        assert_eq!(event.payload["level_name"].as_str().unwrap(), "Gold");
        assert_eq!(
            event.payload["recipient"]["channel_id"].as_str().unwrap(),
            "UCrecipient"
        );
        assert_eq!(
            event.payload["recipient"]["display_name"].as_str().unwrap(),
            "LuckyViewer"
        );
        assert!(event.payload["gifter"]["channel_id"].is_null());
        assert!(event.payload["gifter"]["display_name"].is_null());
    }

    #[tokio::test]
    async fn gift_membership_received_reads_wire_gifter_channel_id() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-gift-recv-src")).await;
        mount_chat_mock(
            &server,
            chat_response(
                json!([gift_received_item(
                    "recv-2",
                    "Gold",
                    "LuckyViewer",
                    "UCrecipient",
                    json!("UCbenefactor")
                )]),
                3000,
            ),
        )
        .await;

        let event = first_event_from(&server).await;

        assert_eq!(
            event.payload["gifter"]["channel_id"].as_str().unwrap(),
            "UCbenefactor"
        );
    }

    fn broadcast_response_titled(live_chat_id: &str, title: &str) -> serde_json::Value {
        json!({
            "items": [{
                "id": "broadcast-1",
                "snippet": {
                    "liveChatId": live_chat_id,
                    "title": title
                }
            }]
        })
    }

    fn quota_for_two_resolutions() -> Arc<tokio::sync::Mutex<QuotaState>> {
        Arc::new(tokio::sync::Mutex::new(QuotaState {
            used_today: QUOTA_DAILY_LIMIT_FOR_TEST - 2 * BROADCAST_COST,
            peak_seen: QUOTA_DAILY_LIMIT_FOR_TEST - 2 * BROADCAST_COST,
            last_reset_date: today_pacific(),
            long_interval_mode: false,
        }))
    }

    const QUOTA_DAILY_LIMIT_FOR_TEST: u32 = 10_000;

    #[allow(clippy::type_complexity)]
    fn poller_with_quota(
        server: &MockServer,
        quota: Arc<tokio::sync::Mutex<QuotaState>>,
    ) -> (
        YoutubeChatPoller,
        LiveChatIdHandle,
        tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let live = LiveChatIdHandle::new();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            live.clone(),
            ActiveBroadcastIdHandle::new(),
            quota,
        )
        .with_api_base(server.uri());
        (poller, live, rx)
    }

    async fn mount_sequenced_broadcasts(
        server: &MockServer,
        first: serde_json::Value,
        second: serde_json::Value,
    ) {
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(first))
            .up_to_n_times(1)
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(second.clone()))
            .up_to_n_times(1)
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(second))
            .mount(server)
            .await;
    }

    async fn run_until_resolution_and_drain_titles(
        poller: YoutubeChatPoller,
        live: LiveChatIdHandle,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
        until_live_chat_id: &str,
    ) -> Vec<Event> {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        let mut reached = false;
        for _ in 0..5_000 {
            if live.get().as_deref() == Some(until_live_chat_id) {
                reached = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        cancel.cancel();
        join.await.unwrap().unwrap();
        assert!(
            reached,
            "poller never reached live chat id {until_live_chat_id}"
        );

        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if e.kind == "youtube.stream.title_changed" {
                events.push(e);
            }
        }
        events
    }

    #[tokio::test(start_paused = true)]
    async fn first_broadcast_resolution_records_title_without_emitting_event() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response_titled("lc-1", "Steady Title"),
            broadcast_response_titled("lc-2", "Steady Title"),
        )
        .await;

        let (poller, live, rx) = poller_with_quota(&server, quota_for_two_resolutions());
        let events = run_until_resolution_and_drain_titles(poller, live, rx, "lc-2").await;

        assert!(
            events.is_empty(),
            "first resolution must only record; unchanged title must not fire, got {} event(s)",
            events.len()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn changed_title_on_second_resolution_emits_one_title_changed_event() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response_titled("lc-1", "Old Title"),
            broadcast_response_titled("lc-2", "New Title"),
        )
        .await;

        let (poller, live, rx) = poller_with_quota(&server, quota_for_two_resolutions());
        let events = run_until_resolution_and_drain_titles(poller, live, rx, "lc-2").await;

        assert_eq!(
            events.len(),
            1,
            "a single title change must fire exactly one event"
        );
        let payload = &events[0].payload;
        assert_eq!(events[0].source, EventSource::YouTube);
        assert_eq!(payload["title"]["old"].as_str().unwrap(), "Old Title");
        assert_eq!(payload["title"]["new"].as_str().unwrap(), "New Title");
    }

    #[tokio::test(start_paused = true)]
    async fn unchanged_title_across_two_resolutions_emits_no_event() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response_titled("lc-1", "Same Title"),
            broadcast_response_titled("lc-2", "Same Title"),
        )
        .await;

        let (poller, live, rx) = poller_with_quota(&server, quota_for_two_resolutions());
        let events = run_until_resolution_and_drain_titles(poller, live, rx, "lc-2").await;

        assert!(
            events.is_empty(),
            "identical titles must not fire, got {} event(s)",
            events.len()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn title_diff_across_intervening_offline_clear_does_not_fire() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(broadcast_response_titled("lc-1", "Title A")),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(empty_broadcast_response()))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(broadcast_response_titled("lc-2", "Title B")),
            )
            .mount(&server)
            .await;

        let quota = Arc::new(tokio::sync::Mutex::new(QuotaState {
            used_today: QUOTA_DAILY_LIMIT_FOR_TEST - 3 * BROADCAST_COST,
            peak_seen: QUOTA_DAILY_LIMIT_FOR_TEST - 3 * BROADCAST_COST,
            last_reset_date: today_pacific(),
            long_interval_mode: false,
        }));

        let (poller, live, rx) = poller_with_quota(&server, quota);
        let events = run_until_resolution_and_drain_titles(poller, live, rx, "lc-2").await;

        assert!(
            events.is_empty(),
            "an offline clear between two live titles must reset last-seen; \
             go-live with a new title must not fire, got {} event(s)",
            events.len()
        );
    }

    fn chat_ended_item(id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "snippet": { "type": "chatEndedEvent" }
        })
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_offline_resolutions_emit_no_events() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, empty_broadcast_response()).await;

        let (poller, _live, rx) = poller_with_quota(&server, make_quota());
        let events = drain_events_over_broadcast_cycles(poller, rx, &server, 3).await;

        assert!(
            events.is_empty(),
            "repeated offline resolutions must stay silent, got {events:?}"
        );
    }

    #[tokio::test]
    async fn going_live_emits_single_online_with_title_and_broadcast_id() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("lc-live")).await;
        mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

        let (poller, rx) = make_poller_with_receiver(&server);
        let events = drain_events_over_broadcast_cycles(poller, rx, &server, 1).await;

        let online: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind == "youtube.stream.online")
            .collect();
        assert_eq!(online.len(), 1, "exactly one online, got {events:?}");
        let payload = &online[0].payload;
        assert_eq!(
            payload[stream_fields::BROADCAST_TITLE].as_str().unwrap(),
            "Test Stream"
        );
        assert_eq!(
            payload[stream_fields::BROADCAST_ID].as_str().unwrap(),
            "broadcast-1"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn going_offline_emits_single_offline_with_last_broadcast_id() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response("lc-live"),
            empty_broadcast_response(),
        )
        .await;

        let (poller, _live, rx) = poller_with_quota(&server, quota_for_two_resolutions());
        let events = drain_events_over_broadcast_cycles(poller, rx, &server, 2).await;

        let offline: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind == "youtube.stream.offline")
            .collect();
        assert_eq!(offline.len(), 1, "exactly one offline, got {events:?}");
        assert_eq!(
            offline[0].payload[stream_fields::BROADCAST_ID]
                .as_str()
                .unwrap(),
            "broadcast-1"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn chat_ended_then_absent_broadcast_emits_single_offline_total() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response("lc-live"),
            empty_broadcast_response(),
        )
        .await;
        mount_chat_mock(
            &server,
            chat_response(json!([chat_ended_item("ended-1")]), 0),
        )
        .await;

        let (poller, rx) = make_poller_with_receiver(&server);
        let events = drain_events_over_broadcast_cycles(poller, rx, &server, 2).await;

        let offline = events
            .iter()
            .filter(|e| e.kind == "youtube.stream.offline")
            .count();
        assert_eq!(
            offline, 1,
            "chatEndedEvent then absent broadcast must not double-emit offline, got {events:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn same_live_session_resolved_twice_emits_single_online() {
        let server = MockServer::start().await;
        mount_sequenced_broadcasts(
            &server,
            broadcast_response("lc-1"),
            broadcast_response("lc-2"),
        )
        .await;

        let (poller, _live, rx) = poller_with_quota(&server, quota_for_two_resolutions());
        let events = drain_events_over_broadcast_cycles(poller, rx, &server, 2).await;

        let online = events
            .iter()
            .filter(|e| e.kind == "youtube.stream.online")
            .count();
        assert_eq!(
            online, 1,
            "an uninterrupted live session must emit online once, got {events:?}"
        );
    }
}
