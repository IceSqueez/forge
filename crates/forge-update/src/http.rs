use std::sync::{Arc, OnceLock};
use std::time::Duration;

use forge_net::{RateLimiter, TokenBucketRateLimiter, acquire_or_wait};

use crate::UpdateError;

const USER_AGENT_VALUE: &str = concat!("forge/", env!("CARGO_PKG_VERSION"));

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const SMALL_FILE_TIMEOUT: Duration = Duration::from_secs(10);

const GITHUB_ANON_REQUEST_BUDGET: u32 = 60;
const GITHUB_ANON_WINDOW: Duration = Duration::from_secs(60 * 60);
const REQUEST_WEIGHT: u32 = 1;

#[derive(Clone)]
pub struct UpdateClient {
    http: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
}

impl UpdateClient {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Result<Self, UpdateError> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT_VALUE)
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(|error| UpdateError::Client(error.without_url().to_string()))?;
        Ok(Self { http, limiter })
    }

    pub fn for_github() -> Result<Self, UpdateError> {
        Self::new(github_limiter())
    }

    pub(crate) async fn fetch_small(&self, url: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
        let mut response = self.send(url, Some(SMALL_FILE_TIMEOUT)).await?;
        if response
            .content_length()
            .is_some_and(|length| length > limit)
        {
            return Err(UpdateError::ResponseTooLarge { limit });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(UpdateError::network)? {
            body.extend_from_slice(&chunk);
            if body.len() as u64 > limit {
                return Err(UpdateError::ResponseTooLarge { limit });
            }
        }
        Ok(body)
    }

    pub(crate) async fn open_stream(&self, url: &str) -> Result<reqwest::Response, UpdateError> {
        self.send(url, None).await
    }

    async fn send(
        &self,
        url: &str,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, UpdateError> {
        acquire_or_wait(self.limiter.as_ref(), REQUEST_WEIGHT)
            .await
            .map_err(|_| UpdateError::RateLimited)?;
        let mut request = self.http.get(url);
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await.map_err(UpdateError::network)?;
        let status = response.status();
        if !status.is_success() {
            return Err(UpdateError::Status(status.as_u16()));
        }
        Ok(response)
    }
}

fn github_limiter() -> Arc<dyn RateLimiter> {
    static LIMITER: OnceLock<Arc<TokenBucketRateLimiter>> = OnceLock::new();
    let limiter = LIMITER.get_or_init(|| {
        Arc::new(TokenBucketRateLimiter::new(
            GITHUB_ANON_REQUEST_BUDGET,
            GITHUB_ANON_WINDOW,
        ))
    });
    Arc::clone(limiter) as Arc<dyn RateLimiter>
}
