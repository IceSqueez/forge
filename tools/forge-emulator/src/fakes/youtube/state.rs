use std::sync::{Mutex, MutexGuard};

use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::sync::watch;

use super::config::FakeYouTubeConfig;
use super::ledger::{YouTubeLedger, YouTubeRequest};

pub(crate) const CHAT_ENDED_EVENT: &str = "chatEndedEvent";

pub(crate) struct Tokens {
    pub(crate) access: String,
    pub(crate) refresh: String,
    pub(crate) rotations: u32,
}

pub(crate) struct Broadcast {
    pub(crate) live: bool,
    pub(crate) started_at: Option<String>,
    pub(crate) offline_at: Option<String>,
    pub(crate) ended_at_message: Option<usize>,
}

pub(crate) struct Inner {
    pub(crate) ledger: YouTubeLedger,
    pub(crate) tokens: Tokens,
    pub(crate) broadcast: Broadcast,
    pub(crate) chat: Vec<Value>,
    next_message: u64,
}

pub(crate) fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

impl Inner {
    fn new(config: &FakeYouTubeConfig) -> Self {
        Self {
            ledger: YouTubeLedger::default(),
            tokens: Tokens {
                access: config.access_token.clone(),
                refresh: config.refresh_token.clone(),
                rotations: 0,
            },
            broadcast: Broadcast {
                live: config.live,
                started_at: config.live.then(now_rfc3339),
                offline_at: None,
                ended_at_message: None,
            },
            chat: Vec::new(),
            next_message: 1,
        }
    }

    pub(crate) fn chat_open(&self) -> bool {
        self.broadcast.live
    }

    pub(crate) fn append_message(
        &mut self,
        config: &FakeYouTubeConfig,
        mut snippet: Map<String, Value>,
        author_details: Value,
    ) -> String {
        let sequence = self.next_message;
        self.next_message += 1;
        let id = format!("emulator-message-{sequence}");
        snippet.insert("liveChatId".to_owned(), json!(config.live_chat_id));
        snippet.insert("publishedAt".to_owned(), json!(now_rfc3339()));
        if let Some(channel_id) = author_details.get("channelId") {
            snippet.insert("authorChannelId".to_owned(), channel_id.clone());
        }
        snippet.entry("hasDisplayContent").or_insert(json!(true));
        self.chat.push(json!({
            "kind": "youtube#liveChatMessage",
            "etag": format!("emulator-etag-{sequence}"),
            "id": id,
            "snippet": snippet,
            "authorDetails": author_details,
        }));
        id
    }

    pub(crate) fn owner_details(config: &FakeYouTubeConfig) -> Value {
        json!({
            "channelId": config.channel_id,
            "channelUrl": format!("http://www.youtube.com/channel/{}", config.channel_id),
            "displayName": config.channel_title,
            "profileImageUrl": "",
            "isVerified": false,
            "isChatOwner": true,
            "isChatSponsor": false,
            "isChatModerator": false
        })
    }

    pub(crate) fn go_live(&mut self) {
        self.broadcast.live = true;
        self.broadcast.started_at = Some(now_rfc3339());
        self.broadcast.offline_at = None;
        self.broadcast.ended_at_message = None;
    }

    pub(crate) fn end_broadcast(&mut self, config: &FakeYouTubeConfig) {
        let mut snippet = Map::new();
        snippet.insert("type".to_owned(), json!(CHAT_ENDED_EVENT));
        snippet.insert("hasDisplayContent".to_owned(), json!(false));
        self.append_message(config, snippet, Self::owner_details(config));
        self.broadcast.live = false;
        self.broadcast.offline_at = Some(now_rfc3339());
        self.broadcast.ended_at_message = Some(self.chat.len());
    }

    pub(crate) fn record_request(&mut self, request: YouTubeRequest) {
        self.ledger.requests.push(request);
    }
}

pub(crate) struct Shared {
    config: FakeYouTubeConfig,
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
}

impl Shared {
    pub(crate) fn new(config: FakeYouTubeConfig) -> Self {
        let inner = Inner::new(&config);
        Self {
            config,
            inner: Mutex::new(inner),
            changes: watch::Sender::new(0),
        }
    }

    pub(crate) fn config(&self) -> &FakeYouTubeConfig {
        &self.config
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn read<R>(&self, f: impl FnOnce(&Inner) -> R) -> R {
        f(&self.lock())
    }

    pub(crate) fn mutate<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let result = f(&mut self.lock());
        self.changes.send_modify(|generation| *generation += 1);
        result
    }

    pub(crate) fn changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }
}
