use std::sync::{Arc, OnceLock};
use std::time::Duration;

use forge_net::{
    BodyProgress, BodySink, CallClass, DEFAULT_RETRY_AFTER_SECS, Gateway, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, Method, RateLimiter, StreamFailure,
    ThrottleHintFamily, TokenBucketRateLimiter,
};
use tokio_util::sync::CancellationToken;

use crate::UpdateError;

const GITHUB_API_REQUEST_BUDGET: u32 = 60;
const GITHUB_API_WINDOW: Duration = Duration::from_secs(60 * 60);
const RELEASE_DOWNLOAD_REQUEST_BUDGET: u32 = 120;
const RELEASE_DOWNLOAD_WINDOW: Duration = Duration::from_secs(60 * 60);
const NO_HINT_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

const RELEASE_API_PROFILE: GatewayProfile = GatewayProfile::absolute_https(
    CallClass::Catalog,
    ThrottleHintFamily::ResetEpochSeconds,
    NO_HINT_WAIT,
);
const RELEASE_DOWNLOAD_PROFILE: GatewayProfile = GatewayProfile::absolute_https(
    CallClass::Media,
    ThrottleHintFamily::RetryAfter,
    NO_HINT_WAIT,
);

#[derive(Clone)]
pub struct UpdateClient {
    release_api: Gateway,
    release_downloads: Gateway,
}

pub(crate) enum StreamError {
    TooLarge,
    Update(UpdateError),
}

impl UpdateClient {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Result<Self, UpdateError> {
        Self::with_limiters(Arc::clone(&limiter), limiter)
    }

    pub fn for_github() -> Result<Self, UpdateError> {
        static CLIENT: OnceLock<UpdateClient> = OnceLock::new();
        if let Some(client) = CLIENT.get() {
            return Ok(client.clone());
        }
        let client = Self::with_limiters(
            Arc::new(TokenBucketRateLimiter::new(
                GITHUB_API_REQUEST_BUDGET,
                GITHUB_API_WINDOW,
            )),
            Arc::new(TokenBucketRateLimiter::new(
                RELEASE_DOWNLOAD_REQUEST_BUDGET,
                RELEASE_DOWNLOAD_WINDOW,
            )),
        )?;
        Ok(CLIENT.get_or_init(|| client).clone())
    }

    fn with_limiters(
        api_limiter: Arc<dyn RateLimiter>,
        download_limiter: Arc<dyn RateLimiter>,
    ) -> Result<Self, UpdateError> {
        let build = |limiter, profile| {
            Gateway::without_base(limiter, profile, None)
                .map_err(|error| UpdateError::Client(error.to_string()))
        };
        Ok(Self {
            release_api: build(api_limiter, RELEASE_API_PROFILE)?,
            release_downloads: build(download_limiter, RELEASE_DOWNLOAD_PROFILE)?,
        })
    }

    pub(crate) async fn fetch_release_metadata(
        &self,
        url: &str,
        limit: u64,
    ) -> Result<Vec<u8>, UpdateError> {
        fetch_small(&self.release_api, url, limit).await
    }

    pub(crate) async fn fetch_release_file(
        &self,
        url: &str,
        limit: u64,
    ) -> Result<Vec<u8>, UpdateError> {
        fetch_small(&self.release_downloads, url, limit).await
    }

    pub(crate) async fn stream_release_file<S, P>(
        &self,
        url: &str,
        cap: u64,
        sink: &mut S,
        on_progress: P,
        cancel: &CancellationToken,
    ) -> Result<u64, StreamError>
    where
        S: BodySink<Error = std::io::Error>,
        P: FnMut(BodyProgress) + Send,
    {
        let request = GatewayRequest::absolute(Method::Get, url).class(CallClass::Media);
        self.release_downloads
            .send_streaming(request, cap, sink, on_progress, cancel)
            .await
            .map(|response| response.received())
            .map_err(|failure| match failure {
                StreamFailure::Gateway(GatewayFailure::ResponseTooLarge { .. }) => {
                    StreamError::TooLarge
                }
                StreamFailure::Gateway(failure) => {
                    StreamError::Update(UpdateError::from_gateway(failure))
                }
                StreamFailure::Sink(error) => StreamError::Update(UpdateError::Io(error)),
            })
    }
}

async fn fetch_small(gateway: &Gateway, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
    let request = GatewayRequest::absolute(Method::Get, url)
        .class(CallClass::Catalog)
        .body_cap(limit);
    gateway
        .send(request)
        .await
        .map(GatewayResponse::into_body)
        .map_err(|failure| match failure {
            GatewayFailure::ResponseTooLarge { .. } => UpdateError::ResponseTooLarge { limit },
            failure => UpdateError::from_gateway(failure),
        })
}
