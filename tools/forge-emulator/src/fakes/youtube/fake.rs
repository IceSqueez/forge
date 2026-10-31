use forge_platform_core::EndpointSurface;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::sync::watch;

use super::config::FakeYouTubeConfig;
use super::ledger::YouTubeLedger;
use super::rest::{self, DATA_API_PREFIX, OAUTH_PREFIX, UPLOAD_API_PREFIX};
use super::state::Shared;
use crate::EmulatorError;
use crate::fakes::support::{ServerTasks, bind_loopback, local_addr, serve_http};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YouTubeChatter {
    pub channel_id: String,
    pub display_name: String,
    #[serde(default)]
    pub sponsor: bool,
    #[serde(default)]
    pub moderator: bool,
}

impl YouTubeChatter {
    fn author_details(&self) -> Value {
        json!({
            "channelId": self.channel_id,
            "channelUrl": format!("http://www.youtube.com/channel/{}", self.channel_id),
            "displayName": self.display_name,
            "profileImageUrl": "",
            "isVerified": false,
            "isChatOwner": false,
            "isChatSponsor": self.sponsor,
            "isChatModerator": self.moderator
        })
    }
}

pub struct FakeYouTube {
    shared: Arc<Shared>,
    http_base: String,
    server: ServerTasks,
}

impl FakeYouTube {
    pub async fn start(config: FakeYouTubeConfig) -> Result<Self, EmulatorError> {
        for (field, value) in [
            ("broadcast_id", &config.broadcast_id),
            ("live_chat_id", &config.live_chat_id),
        ] {
            if value.trim().is_empty() {
                return Err(EmulatorError::InvalidFakeConfig {
                    reason: format!("the YouTube {field} must not be blank"),
                });
            }
        }
        let listener = bind_loopback().await?;
        let address: SocketAddr = local_addr(&listener)?;
        let shared = Arc::new(Shared::new(config));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let router = rest::router(Arc::clone(&shared));
        let task = serve_http(listener, router, shutdown_rx);
        Ok(Self {
            shared,
            http_base: format!("http://{address}"),
            server: ServerTasks::new(shutdown, vec![task]),
        })
    }

    pub fn http_base(&self) -> &str {
        &self.http_base
    }

    pub fn url_for(&self, surface: EndpointSurface) -> Option<String> {
        self.endpoint_overrides()
            .into_iter()
            .find(|(variable, _)| *variable == surface.env_var())
            .map(|(_, url)| url)
    }

    pub fn endpoint_overrides(&self) -> [(&'static str, String); 3] {
        [
            (
                forge_platform_youtube::endpoint_surfaces::YOUTUBE_DATA_API.env_var(),
                format!("{}{DATA_API_PREFIX}", self.http_base),
            ),
            (
                forge_platform_youtube::endpoint_surfaces::YOUTUBE_UPLOAD_API.env_var(),
                format!("{}{UPLOAD_API_PREFIX}", self.http_base),
            ),
            (
                forge_platform_youtube::endpoint_surfaces::YOUTUBE_OAUTH.env_var(),
                format!("{}{OAUTH_PREFIX}", self.http_base),
            ),
        ]
    }

    pub fn config(&self) -> &FakeYouTubeConfig {
        self.shared.config()
    }

    pub fn ledger(&self) -> YouTubeLedger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    pub fn access_token(&self) -> String {
        self.shared.read(|inner| inner.tokens.access.clone())
    }

    pub fn is_live(&self) -> bool {
        self.shared.read(|inner| inner.broadcast.live)
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&YouTubeLedger) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        self.shared
            .wait_for(what, timeout, |inner| probe(&inner.ledger))
            .await
    }

    pub fn inject_chat_message(
        &self,
        author: &YouTubeChatter,
        text: &str,
    ) -> Result<String, EmulatorError> {
        let mut snippet = Map::new();
        snippet.insert("type".to_owned(), json!("textMessageEvent"));
        snippet.insert("displayMessage".to_owned(), json!(text));
        snippet.insert(
            "textMessageDetails".to_owned(),
            json!({ "messageText": text }),
        );
        self.inject_chat_event(author, snippet)
    }

    pub fn inject_chat_event(
        &self,
        author: &YouTubeChatter,
        snippet: Map<String, Value>,
    ) -> Result<String, EmulatorError> {
        let config = self.shared.config();
        self.shared.mutate(|inner| {
            if !inner.chat_open() {
                return Err(EmulatorError::YouTubeChatNotLive {
                    live_chat_id: config.live_chat_id.clone(),
                });
            }
            Ok(inner.append_message(config, snippet, author.author_details()))
        })
    }

    pub fn set_live(&self, live: bool) {
        let config = self.shared.config();
        self.shared
            .mutate(|inner| match (inner.broadcast.live, live) {
                (false, true) => inner.go_live(),
                (true, false) => inner.end_broadcast(config),
                _ => {}
            });
    }

    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}
