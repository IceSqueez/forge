use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::PlatformError;
use forge_types::{ChatEventDetail, ChatPayload, ChatSegment, ModerationMarks, UserBadge};
use futures::future::BoxFuture;
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::dedup_window::{DEDUP_WINDOW_SIZE, DedupWindow};
use crate::live_chat_id::LiveChatIdHandle;
use crate::quota_state::{BROADCAST_COST, CHAT_POLL_COST, QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const POLL_FLOOR_MS: u64 = 3_000;
const LONG_INTERVAL_MS: u64 = 60_000;
const BROADCAST_CADENCE_SECS: u64 = 60;

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
}

impl YoutubeChatPoller {
    pub fn new(
        access_token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        bus_sender: UnboundedSender<Event>,
        channel_id: String,
        live_chat_id: LiveChatIdHandle,
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
        }
    }

    #[cfg(test)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn run(self, cancel: CancellationToken) -> Result<(), PlatformError> {
        let mut dedup = DedupWindow::new(DEDUP_WINDOW_SIZE);

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

            let live_chat_id = match self.fetch_live_chat_id(&token).await {
                Ok(Some(id)) => id,
                Ok(None) => {
                    self.live_chat_id.set(None);
                    let event = Event::new(
                        EventSource::YouTube,
                        "youtube.channel.no_active_broadcast",
                        serde_json::Value::Object(Default::default()),
                    );
                    if self.bus_sender.send(event).is_err() {
                        return Ok(());
                    }
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
                Err(e) => {
                    self.live_chat_id.set(None);
                    tracing::warn!("broadcast resolution failed: {e}");
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_secs(BROADCAST_CADENCE_SECS)) => {}
                        () = cancel.cancelled() => return Ok(()),
                    }
                    continue 'outer;
                }
            };

            self.live_chat_id.set(Some(live_chat_id.clone()));

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
                    if let Some(event) = self.build_event(item, &mut dedup)
                        && self.bus_sender.send(event).is_err()
                    {
                        return Ok(());
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
        }
    }

    async fn fetch_live_chat_id(&self, token: &str) -> Result<Option<String>, PlatformError> {
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

        let id = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("snippet"))
            .and_then(|snippet| snippet.get("liveChatId"))
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned);

        Ok(id)
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

    fn build_event(&self, item: &serde_json::Value, dedup: &mut DedupWindow) -> Option<Event> {
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
                    serde_json::json!({ "broadcast_id": id }),
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
                    "message_text": text,
                    "user_display_name": author,
                    "channel_id": self.channel_id,
                });

                if is_command {
                    let (cmd_name, args) = parse_command(&text);
                    payload["command_name"] = serde_json::Value::String(cmd_name);
                    payload["args"] = serde_json::Value::String(args);
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
                    "user_display_name": author,
                    "amount_micros": amount_micros,
                    "currency": currency,
                });
                if let Some(msg) = message {
                    payload["message_text"] = serde_json::Value::String(msg);
                }
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
                    message: Some(sticker_id.clone()),
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
                    "user_display_name": author,
                    "sticker_id": sticker_id,
                    "amount_micros": amount_micros,
                    "currency": currency,
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
                    "user_display_name": author,
                    "member_level_name": level,
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
                    "user_display_name": author,
                    "member_month": months,
                });
                if let Some(msg) = message {
                    payload["message_text"] = serde_json::Value::String(msg);
                }
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.member_milestone",
                    payload,
                ))
            }

            "userBannedEvent" => {
                let banned_details = snippet.get("userBannedDetails");
                let display_name = banned_details
                    .and_then(|d| d.get("bannedUserDetails"))
                    .and_then(|u| u.get("displayName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let channel_id = banned_details
                    .and_then(|d| d.get("bannedUserDetails"))
                    .and_then(|u| u.get("channelId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();
                let ban_duration_secs: u64 = banned_details
                    .and_then(|d| d.get("banDurationSeconds"))
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    })
                    .unwrap_or(0);

                let is_temporary = ban_duration_secs > 0;
                let ban_type = if is_temporary {
                    "temporary"
                } else {
                    "permanent"
                };

                let chat_payload = ChatPayload {
                    platform_msg_id: id.clone(),
                    author: display_name.clone(),
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
                    "ban.target.display_name": display_name,
                    "ban.target.channel_id": channel_id,
                    "ban.type": ban_type,
                    "ban.duration_seconds": ban_duration_secs as i64,
                });
                payload[ChatPayload::KEY] = serde_json::to_value(&chat_payload).ok()?;
                Some(Event::new(
                    EventSource::YouTube,
                    "youtube.channel.user_banned",
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

fn parse_command(text: &str) -> (String, String) {
    let stripped = text.strip_prefix('!').unwrap_or(text);
    let mut parts = stripped.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("").to_owned();
    let args = parts.next().unwrap_or("").to_owned();
    (cmd, args)
}

fn extract_author(author_details: Option<&serde_json::Value>) -> String {
    author_details
        .and_then(|ad| ad.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
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
            make_quota(),
        )
        .with_api_base(server.uri());
        (poller, rx)
    }

    fn make_poller_with_handle(
        server: &MockServer,
    ) -> (
        YoutubeChatPoller,
        LiveChatIdHandle,
        tokio::sync::mpsc::UnboundedReceiver<Event>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = LiveChatIdHandle::new();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            handle.clone(),
            make_quota(),
        )
        .with_api_base(server.uri());
        (poller, handle, rx)
    }

    #[tokio::test]
    async fn broadcast_list_empty_emits_no_chat_events() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, empty_broadcast_response()).await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            make_quota(),
        )
        .with_api_base(server.uri());

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
        handle.await.unwrap().unwrap();

        let mut chat_events = 0u32;
        while let Ok(event) = rx.try_recv() {
            if event.kind.contains(".chat.") {
                chat_events += 1;
            }
        }
        assert_eq!(
            chat_events, 0,
            "expected zero chat events when no broadcast"
        );
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

        let (poller, mut rx) = make_poller_with_receiver(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();

        cancel.cancel();
        handle.await.unwrap().unwrap();

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

        let (poller, mut rx) = make_poller_with_receiver(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();

        cancel.cancel();
        handle.await.unwrap().unwrap();

        assert_eq!(event.kind, "youtube.chat.command");
        assert_eq!(event.payload["command_name"].as_str().unwrap(), "shoutout");
        assert_eq!(event.payload["args"].as_str().unwrap(), "user123");
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

        let (poller, mut rx) = make_poller_with_receiver(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();

        cancel.cancel();
        handle.await.unwrap().unwrap();

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
                    "banDurationSeconds": duration_secs
                }
            }
        })
    }

    async fn first_banned_event(server: &MockServer) -> Event {
        let (poller, mut rx) = make_poller_with_receiver(server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();

        cancel.cancel();
        handle.await.unwrap().unwrap();
        event
    }

    #[tokio::test]
    async fn user_banned_with_duration_emits_temporary_ban() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-temp")).await;
        mount_chat_mock(
            &server,
            chat_response(json!([banned_item("ban-1", "Troll", json!(600))]), 3000),
        )
        .await;

        let event = first_banned_event(&server).await;

        assert_eq!(event.kind, "youtube.channel.user_banned");
        assert_eq!(event.payload["ban.type"].as_str().unwrap(), "temporary");
        assert_eq!(event.payload["ban.duration_seconds"].as_i64().unwrap(), 600);
        assert_eq!(
            event.payload["ban.target.display_name"].as_str().unwrap(),
            "Troll"
        );
    }

    #[tokio::test]
    async fn user_banned_without_duration_emits_permanent_ban() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-perm")).await;
        mount_chat_mock(
            &server,
            chat_response(json!([banned_item("ban-2", "Spammer", json!(0))]), 3000),
        )
        .await;

        let event = first_banned_event(&server).await;

        assert_eq!(event.kind, "youtube.channel.user_banned");
        assert_eq!(event.payload["ban.type"].as_str().unwrap(), "permanent");
        assert_eq!(event.payload["ban.duration_seconds"].as_i64().unwrap(), 0);
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
            make_quota(),
        )
        .with_api_base(server.uri());

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

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

    #[tokio::test]
    async fn poller_updates_live_chat_id_handle_when_broadcast_active() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("lc-active-123")).await;
        mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

        let (poller, handle, _rx) = make_poller_with_handle(&server);
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        let found = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(id) = handle.get() {
                    return id;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("handle was not set within timeout");

        cancel.cancel();
        join.await.unwrap().unwrap();

        assert_eq!(found, "lc-active-123");
    }

    #[tokio::test]
    async fn poller_clears_live_chat_id_handle_when_no_broadcast() {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, empty_broadcast_response()).await;

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let handle = LiveChatIdHandle::new();
        handle.set(Some("stale-id".to_owned()));

        let poller = YoutubeChatPoller::new(
            token_source(),
            tx,
            "UCtest".to_owned(),
            handle.clone(),
            make_quota(),
        )
        .with_api_base(server.uri());

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let join = tokio::spawn(async move { poller.run(cancel_clone).await });

        let cleared = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if handle.get().is_none() {
                    return true;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("handle was not cleared within timeout");

        cancel.cancel();
        join.await.unwrap().unwrap();

        assert!(cleared);
    }
}
