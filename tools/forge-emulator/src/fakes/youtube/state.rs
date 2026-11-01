use crate::fakes::support::Observed;
use std::ops::Deref;

use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::config::FakeYouTubeConfig;
use super::ledger::{YouTubeLedger, YouTubeRequest};

pub(crate) const CHAT_ENDED_EVENT: &str = "chatEndedEvent";

pub(crate) struct Tokens {
    pub(crate) access: String,
    pub(crate) refresh: String,
    pub(crate) rotations: u32,
}

pub(crate) enum Broadcast {
    NeverLive,
    Live {
        started_at: String,
    },
    Ended {
        started_at: String,
        offline_at: String,
        ended_at_message: usize,
    },
}

impl Broadcast {
    pub(crate) fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }

    pub(crate) fn never_live(&self) -> bool {
        matches!(self, Self::NeverLive)
    }

    pub(crate) fn started_at(&self) -> Option<&str> {
        match self {
            Self::NeverLive => None,
            Self::Live { started_at } | Self::Ended { started_at, .. } => Some(started_at),
        }
    }

    pub(crate) fn ended_at_message(&self) -> Option<usize> {
        match self {
            Self::Ended {
                ended_at_message, ..
            } => Some(*ended_at_message),
            Self::NeverLive | Self::Live { .. } => None,
        }
    }

    pub(crate) fn offline_at(&self) -> Option<&str> {
        match self {
            Self::Ended { offline_at, .. } => Some(offline_at),
            Self::NeverLive | Self::Live { .. } => None,
        }
    }
}

pub(crate) struct Inner {
    pub(crate) ledger: YouTubeLedger,
    pub(crate) tokens: Tokens,
    pub(crate) broadcast: Broadcast,
    pub(crate) chat: Vec<Value>,
    pub(crate) unmodeled_insert: bool,
    next_message: u64,
}

const UNIX_EPOCH_RFC3339: &str = "1970-01-01T00:00:00Z";

pub(crate) fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| UNIX_EPOCH_RFC3339.to_owned())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthorRoles {
    pub(crate) owner: bool,
    pub(crate) sponsor: bool,
    pub(crate) moderator: bool,
}

pub(crate) fn author_details(channel_id: &str, display_name: &str, roles: AuthorRoles) -> Value {
    json!({
        "channelId": channel_id,
        "channelUrl": format!("http://www.youtube.com/channel/{channel_id}"),
        "displayName": display_name,
        "profileImageUrl": "",
        "isVerified": false,
        "isChatOwner": roles.owner,
        "isChatSponsor": roles.sponsor,
        "isChatModerator": roles.moderator
    })
}

pub(crate) fn text_message_snippet(text: &str) -> Map<String, Value> {
    let mut snippet = Map::new();
    snippet.insert("type".to_owned(), json!("textMessageEvent"));
    snippet.insert("displayMessage".to_owned(), json!(text));
    snippet.insert(
        "textMessageDetails".to_owned(),
        json!({ "messageText": text }),
    );
    snippet
}

impl Inner {
    fn new(config: &FakeYouTubeConfig) -> Self {
        Self {
            ledger: YouTubeLedger::default(),
            unmodeled_insert: false,
            tokens: Tokens {
                access: config.access_token.clone(),
                refresh: config.refresh_token.clone(),
                rotations: 0,
            },
            broadcast: if config.live {
                Broadcast::Live {
                    started_at: now_rfc3339(),
                }
            } else {
                Broadcast::NeverLive
            },
            chat: Vec::new(),
            next_message: 1,
        }
    }

    pub(crate) fn chat_open(&self) -> bool {
        self.broadcast.is_live()
    }

    pub(crate) fn append_message(
        &mut self,
        config: &FakeYouTubeConfig,
        snippet: Map<String, Value>,
        author_details: Value,
    ) -> String {
        let message = self.store_message(config, snippet, author_details);
        message["id"].as_str().unwrap_or_default().to_owned()
    }

    pub(crate) fn store_message(
        &mut self,
        config: &FakeYouTubeConfig,
        mut snippet: Map<String, Value>,
        author_details: Value,
    ) -> Value {
        let sequence = self.next_message;
        self.next_message += 1;
        let id = format!("emulator-message-{sequence}");
        snippet.insert("liveChatId".to_owned(), json!(config.live_chat_id));
        snippet.insert("publishedAt".to_owned(), json!(now_rfc3339()));
        if let Some(channel_id) = author_details.get("channelId") {
            snippet.insert("authorChannelId".to_owned(), channel_id.clone());
        }
        snippet.entry("hasDisplayContent").or_insert(json!(true));
        let message = json!({
            "kind": "youtube#liveChatMessage",
            "etag": format!("emulator-etag-{sequence}"),
            "id": id,
            "snippet": snippet,
            "authorDetails": author_details,
        });
        self.chat.push(message.clone());
        message
    }

    pub(crate) fn owner_details(config: &FakeYouTubeConfig) -> Value {
        author_details(
            &config.channel_id,
            &config.channel_title,
            AuthorRoles {
                owner: true,
                sponsor: false,
                moderator: false,
            },
        )
    }

    pub(crate) fn go_live(&mut self) {
        self.broadcast = Broadcast::Live {
            started_at: now_rfc3339(),
        };
    }

    pub(crate) fn end_broadcast(&mut self, config: &FakeYouTubeConfig) {
        let mut snippet = Map::new();
        snippet.insert("type".to_owned(), json!(CHAT_ENDED_EVENT));
        snippet.insert("hasDisplayContent".to_owned(), json!(false));
        self.append_message(config, snippet, Self::owner_details(config));
        self.broadcast = Broadcast::Ended {
            started_at: self.broadcast.started_at().unwrap_or_default().to_owned(),
            offline_at: now_rfc3339(),
            ended_at_message: self.chat.len(),
        };
    }

    pub(crate) fn record_request(&mut self, request: YouTubeRequest) {
        self.ledger.requests.push(request);
    }
}

pub(crate) struct Shared {
    config: FakeYouTubeConfig,
    state: Observed<Inner>,
}

impl Shared {
    pub(crate) fn new(config: FakeYouTubeConfig) -> Self {
        let inner = Inner::new(&config);
        Self {
            config,
            state: Observed::new(inner),
        }
    }

    pub(crate) fn config(&self) -> &FakeYouTubeConfig {
        &self.config
    }
}

impl Deref for Shared {
    type Target = Observed<Inner>;

    fn deref(&self) -> &Observed<Inner> {
        &self.state
    }
}
