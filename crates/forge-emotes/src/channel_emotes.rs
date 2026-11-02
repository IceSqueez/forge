use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use forge_net::status::NOT_FOUND;
use forge_net::{
    CallClass, ExpectedRefusal, Gateway, GatewayProfile, GatewayRequest, RateLimiter,
    ThrottleHintFamily, TokenBucketRateLimiter,
};
use forge_platform_core::{ChatEmoteBook, EndpointSurface, PlatformEndpoints};
use forge_types::RecoverRwLock;
use serde_json::Value;
use tokio::time::Instant;

use crate::catalog::EmoteCatalog;
use crate::endpoint_surfaces::{BETTER_TTV_API, FRANKER_FACE_Z_API, SEVEN_TV_API};
use crate::error::EmotesError;
use crate::provider::{EmoteSource, PRECEDENCE, ProviderEmote};
use crate::{better_ttv, franker_face_z, seven_tv};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
pub const FAILED_SOURCE_RETRY: Duration = Duration::from_secs(5 * 60);
const NO_HINT_WAIT: Duration = Duration::from_secs(30);
const CALLS_PER_WINDOW: u32 = 4;
const CALL_WINDOW: Duration = Duration::from_secs(60);
const TWITCH_USER_ID_LENGTH_CAP: usize = 32;

const PROVIDER_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Catalog,
    ThrottleHintFamily::RetryAfter,
    NO_HINT_WAIT,
);

type AnswerReader = fn(&Value) -> Vec<ProviderEmote>;

struct Providers {
    seven_tv: Gateway,
    better_ttv: Gateway,
    franker_face_z: Gateway,
}

impl Providers {
    fn new(endpoints: &PlatformEndpoints) -> Result<Self, EmotesError> {
        let gateway = |surface: EndpointSurface| -> Result<Gateway, EmotesError> {
            let limiter: Arc<dyn RateLimiter> =
                Arc::new(TokenBucketRateLimiter::new(CALLS_PER_WINDOW, CALL_WINDOW));
            Ok(Gateway::new(
                endpoints.base_url(surface),
                limiter,
                PROVIDER_PROFILE,
                None,
            )?)
        };
        Ok(Self {
            seven_tv: gateway(SEVEN_TV_API)?,
            better_ttv: gateway(BETTER_TTV_API)?,
            franker_face_z: gateway(FRANKER_FACE_Z_API)?,
        })
    }

    fn route(&self, source: EmoteSource, broadcaster_id: &str) -> (&Gateway, String, AnswerReader) {
        match source {
            EmoteSource::SevenTvChannel => (
                &self.seven_tv,
                seven_tv::channel_path(broadcaster_id),
                seven_tv::channel_emotes,
            ),
            EmoteSource::BetterTtvChannel => (
                &self.better_ttv,
                better_ttv::channel_path(broadcaster_id),
                better_ttv::channel_emotes,
            ),
            EmoteSource::FrankerFaceZChannel => (
                &self.franker_face_z,
                franker_face_z::channel_path(broadcaster_id),
                franker_face_z::channel_emotes,
            ),
            EmoteSource::SevenTvGlobal => (
                &self.seven_tv,
                seven_tv::GLOBAL_PATH.to_owned(),
                seven_tv::global_emotes,
            ),
            EmoteSource::BetterTtvGlobal => (
                &self.better_ttv,
                better_ttv::GLOBAL_PATH.to_owned(),
                better_ttv::global_emotes,
            ),
            EmoteSource::FrankerFaceZGlobal => (
                &self.franker_face_z,
                franker_face_z::GLOBAL_PATH.to_owned(),
                franker_face_z::global_emotes,
            ),
        }
    }

    async fn fetch(
        &self,
        source: EmoteSource,
        broadcaster_id: &str,
    ) -> Result<Vec<ProviderEmote>, EmotesError> {
        let (gateway, path, read) = self.route(source, broadcaster_id);
        let channel_scoped = is_channel_scoped(source);
        let mut request = GatewayRequest::get(path).label(label_of(source));
        if channel_scoped {
            request = request.expect_refusal(ExpectedRefusal::status(NOT_FOUND));
        }
        match gateway.send(request).await {
            Ok(response) => response
                .json::<Value>()
                .map(|answer| read(&answer))
                .map_err(|_| EmotesError::Unreadable),
            Err(failure) if channel_scoped && failure.status() == Some(NOT_FOUND) => Ok(Vec::new()),
            Err(failure) => Err(failure.into()),
        }
    }
}

fn is_channel_scoped(source: EmoteSource) -> bool {
    matches!(
        source,
        EmoteSource::SevenTvChannel
            | EmoteSource::BetterTtvChannel
            | EmoteSource::FrankerFaceZChannel
    )
}

fn label_of(source: EmoteSource) -> &'static str {
    match source {
        EmoteSource::SevenTvChannel => "seven_tv_channel_emotes",
        EmoteSource::BetterTtvChannel => "better_ttv_channel_emotes",
        EmoteSource::FrankerFaceZChannel => "franker_face_z_channel_emotes",
        EmoteSource::SevenTvGlobal => "seven_tv_global_emotes",
        EmoteSource::BetterTtvGlobal => "better_ttv_global_emotes",
        EmoteSource::FrankerFaceZGlobal => "franker_face_z_global_emotes",
    }
}

fn is_twitch_user_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= TWITCH_USER_ID_LENGTH_CAP
        && id.chars().all(|c| c.is_ascii_digit())
}

#[derive(Clone)]
pub struct ChannelEmotes {
    providers: Arc<Providers>,
    shown: Arc<RwLock<Option<Arc<EmoteCatalog>>>>,
}

impl ChannelEmotes {
    pub fn new(endpoints: &PlatformEndpoints) -> Result<Self, EmotesError> {
        Ok(Self {
            providers: Arc::new(Providers::new(endpoints)?),
            shown: Arc::default(),
        })
    }

    pub fn book(&self) -> Arc<dyn ChatEmoteBook> {
        Arc::new(self.clone())
    }

    pub fn shown(&self) -> Option<Arc<EmoteCatalog>> {
        self.shown.read_recover().clone()
    }

    pub fn forget(&self) {
        *self.shown.write_recover() = None;
    }

    pub fn follow(&self, broadcaster_id: String) -> impl Future<Output = ()> + Send + 'static {
        let emotes = self.clone();
        async move {
            if !is_twitch_user_id(&broadcaster_id) {
                tracing::debug!(
                    "channel emotes not loaded: the broadcaster id is not a twitch user id"
                );
                return;
            }
            let mut layers: HashMap<EmoteSource, Vec<ProviderEmote>> = HashMap::new();
            let mut next_full = Instant::now();
            loop {
                let full = Instant::now() >= next_full;
                if full {
                    next_full = Instant::now() + REFRESH_INTERVAL;
                }
                for source in PRECEDENCE {
                    if !full && layers.contains_key(&source) {
                        continue;
                    }
                    match emotes.providers.fetch(source, &broadcaster_id).await {
                        Ok(found) => {
                            layers.insert(source, found);
                        }
                        Err(error) => {
                            tracing::debug!(?source, %error, "emote source skipped; retried later");
                        }
                    }
                }
                emotes.show(&layers);
                let until_full = next_full.saturating_duration_since(Instant::now());
                let missing = PRECEDENCE.iter().any(|source| !layers.contains_key(source));
                let wait = if missing {
                    FAILED_SOURCE_RETRY.min(until_full)
                } else {
                    until_full
                };
                tokio::time::sleep(wait).await;
            }
        }
    }

    fn show(&self, layers: &HashMap<EmoteSource, Vec<ProviderEmote>>) {
        let catalog = EmoteCatalog::from_sources(|source| layers.get(&source).map(Vec::as_slice));
        tracing::debug!(emotes = catalog.len(), "channel emote catalog refreshed");
        *self.shown.write_recover() = Some(Arc::new(catalog));
    }
}

impl ChatEmoteBook for ChannelEmotes {
    fn emote_image(&self, code: &str) -> Option<String> {
        self.shown
            .read_recover()
            .as_ref()
            .and_then(|catalog| catalog.image(code).map(str::to_owned))
    }
}
