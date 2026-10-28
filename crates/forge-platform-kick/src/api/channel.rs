use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::{NON_HTTP_STATUS, PlatformError};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::gateway::{RefusalReasons, authorized, json_body, malformed, platform_error};

const CHANNELS_PATH: &str = "/channels";

const CHANNEL_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "channel token rejected (401)",
    forbidden: Some("channel forbidden (403); check channel scope"),
};

pub struct KickChannel {
    api: Gateway,
}

pub struct ChannelSnapshot {
    pub broadcaster_user_id: u64,
    pub slug: String,
    pub is_live: bool,
    pub stream_title: String,
    pub category_id: u64,
    pub category_name: String,
    pub viewer_count: u64,
    pub started_at: Option<OffsetDateTime>,
}

#[derive(Serialize)]
struct UpdateBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct ChannelsEnvelope {
    #[serde(default)]
    data: Vec<ChannelData>,
}

#[derive(Deserialize, Default)]
struct ChannelData {
    #[serde(default)]
    broadcaster_user_id: u64,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    stream_title: String,
    #[serde(default)]
    category: CategoryData,
    #[serde(default)]
    stream: StreamData,
}

#[derive(Deserialize, Default)]
struct CategoryData {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Default)]
struct StreamData {
    #[serde(default)]
    is_live: bool,
    #[serde(default)]
    viewer_count: u64,
    #[serde(default)]
    start_time: Option<String>,
}

impl KickChannel {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn update_info(
        &self,
        token: &str,
        title: Option<String>,
        category_id: Option<u64>,
        tags: Option<Vec<String>>,
    ) -> Result<(), PlatformError> {
        let body = UpdateBody {
            stream_title: title,
            category_id,
            custom_tags: tags,
        };
        let request = authorized(GatewayRequest::patch(CHANNELS_PATH), token, CHANNELS_PATH)
            .body(json_body(&body)?);
        self.api
            .send(request)
            .await
            .map(drop)
            .map_err(|failure| platform_error(failure, CHANNEL_REFUSALS))
    }

    pub async fn get_channel(&self, token: &str) -> Result<ChannelSnapshot, PlatformError> {
        self.fetch(token, None).await
    }

    pub async fn get_channel_by_slug(
        &self,
        token: &str,
        slug: &str,
    ) -> Result<ChannelSnapshot, PlatformError> {
        self.fetch(token, Some(slug)).await
    }

    async fn fetch(
        &self,
        token: &str,
        slug: Option<&str>,
    ) -> Result<ChannelSnapshot, PlatformError> {
        let mut request = authorized(GatewayRequest::get(CHANNELS_PATH), token, CHANNELS_PATH);
        if let Some(slug) = slug {
            request = request.query("slug", slug);
        }
        let response = self
            .api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, CHANNEL_REFUSALS))?;
        let envelope: ChannelsEnvelope = response.json().map_err(malformed)?;

        let channel = envelope
            .data
            .into_iter()
            .next()
            .ok_or_else(|| PlatformError::Http {
                status: NON_HTTP_STATUS,
                body: "channels GET returned no data".to_owned(),
            })?;

        Ok(ChannelSnapshot {
            broadcaster_user_id: channel.broadcaster_user_id,
            slug: channel.slug,
            is_live: channel.stream.is_live,
            stream_title: channel.stream_title,
            category_id: channel.category.id,
            category_name: channel.category.name,
            viewer_count: channel.stream.viewer_count,
            started_at: channel
                .stream
                .start_time
                .as_deref()
                .and_then(crate::timestamp::parse_logged),
        })
    }
}
