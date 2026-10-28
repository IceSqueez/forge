use std::sync::{Mutex, PoisonError};

use forge_net::{Gateway, GatewayFailure, GatewayRequest};
use serde::Deserialize;
use tokio::time::Instant;
use tracing::debug;

use crate::gateway::CHANNEL_INFO_REFRESH_INTERVAL;
use crate::pusher::error::KickError;

const CHANNELS_PATH: &str = "/channels";
const CHANNEL_LABEL: &str = "/channels/{slug}";
const ERROR_BODY_LIMIT: usize = 200;
const USER_AGENT_HEADER: &str = "User-Agent";
const USER_AGENT: &str = concat!(
    "forge/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/IceSqueez/forge)"
);

#[derive(Debug, Clone)]
pub struct KickChannelInfo {
    pub chatroom_id: u64,
}

struct CachedChannelInfo {
    slug: String,
    info: KickChannelInfo,
    fetched_at: Instant,
}

pub struct ChannelInfoFetcher {
    gateway: Gateway,
    cached: Mutex<Option<CachedChannelInfo>>,
}

impl ChannelInfoFetcher {
    pub fn new(gateway: Gateway) -> Self {
        Self {
            gateway,
            cached: Mutex::new(None),
        }
    }

    pub async fn fetch(&self, slug: &str) -> Result<KickChannelInfo, KickError> {
        if let Some(info) = self.fresh(slug) {
            return Ok(info);
        }
        let info = self.request(slug).await?;
        *self.cached.lock().unwrap_or_else(PoisonError::into_inner) = Some(CachedChannelInfo {
            slug: slug.to_owned(),
            info: info.clone(),
            fetched_at: Instant::now(),
        });
        Ok(info)
    }

    fn fresh(&self, slug: &str) -> Option<KickChannelInfo> {
        let cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        cached
            .as_ref()
            .filter(|entry| {
                entry.slug == slug && entry.fetched_at.elapsed() < CHANNEL_INFO_REFRESH_INTERVAL
            })
            .map(|entry| entry.info.clone())
    }

    async fn request(&self, slug: &str) -> Result<KickChannelInfo, KickError> {
        let request = GatewayRequest::get(format!("{CHANNELS_PATH}/{slug}"))
            .header(USER_AGENT_HEADER, USER_AGENT)
            .label(CHANNEL_LABEL);
        let response = self
            .gateway
            .send(request)
            .await
            .map_err(|failure| channel_info_error(slug, failure))?;

        let body: ChannelResponse =
            response
                .json()
                .map_err(|e| KickError::ChannelInfoUnavailable {
                    slug: slug.to_owned(),
                    reason: format!("failed to parse channel response: {e}"),
                })?;

        let chatroom_id = body.chatroom.as_ref().and_then(|c| c.id).ok_or_else(|| {
            KickError::ChatroomIdNotFound {
                slug: slug.to_owned(),
            }
        })?;

        debug!(chatroom_id, "channel info fetched");

        Ok(KickChannelInfo { chatroom_id })
    }
}

fn channel_info_error(slug: &str, failure: GatewayFailure) -> KickError {
    match (failure.status(), failure.excerpt()) {
        (Some(status), Some(excerpt)) => KickError::Http {
            status,
            body: bounded_body(excerpt.as_str()),
        },
        _ => KickError::ChannelInfoUnavailable {
            slug: slug.to_owned(),
            reason: failure.to_string(),
        },
    }
}

fn bounded_body(body: &str) -> String {
    match body.char_indices().nth(ERROR_BODY_LIMIT) {
        Some((end, _)) => body[..end].to_owned(),
        None => body.to_owned(),
    }
}

#[derive(Deserialize)]
struct ChannelResponse {
    chatroom: Option<ChatroomField>,
}

#[derive(Deserialize)]
struct ChatroomField {
    id: Option<u64>,
}
