use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    DEFAULT_RETRY_AFTER_SECS, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
use reqwest::StatusCode;
use reqwest::header::RETRY_AFTER;
use serde::de::DeserializeOwned;

use crate::config::DonatelloConfig;
use crate::error::DonatelloError;
use crate::token::DonatelloToken;
use crate::wire::{DonatesPageWire, MeWire};

const TOKEN_HEADER: &str = "X-Token";
const ME_PATH: &str = "/me";
const DONATES_PATH: &str = "/donates";
const PAGE_PARAM: &str = "page";
const SIZE_PARAM: &str = "size";
const REQUEST_WEIGHT: u32 = 1;
const RATE_LIMIT_BURST: u32 = 20;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

pub(crate) const PAGE_SIZE: u64 = 20;

pub fn default_rate_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(TokenBucketRateLimiter::new(
        RATE_LIMIT_BURST,
        RATE_LIMIT_WINDOW,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DonatelloAccount {
    pub nickname: Option<String>,
    pub page: Option<String>,
}

pub(crate) struct DonatelloApi {
    http: reqwest::Client,
    base_url: String,
    limiter: Arc<dyn RateLimiter>,
}

impl DonatelloApi {
    pub(crate) fn new(
        config: &DonatelloConfig,
        limiter: Arc<dyn RateLimiter>,
    ) -> Result<Self, DonatelloError> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| DonatelloError::ClientInit {
                reason: error.without_url().to_string(),
            })?;
        Ok(Self {
            http,
            base_url: config.base_url.clone(),
            limiter,
        })
    }

    pub(crate) async fn account(
        &self,
        token: &DonatelloToken,
    ) -> Result<DonatelloAccount, DonatelloError> {
        let me: MeWire = match self.get(token, ME_PATH, &[]).await {
            Err(DonatelloError::Http { status }) if status == StatusCode::NOT_FOUND.as_u16() => {
                return Err(DonatelloError::ProfileIncomplete);
            }
            other => other?,
        };
        Ok(DonatelloAccount {
            nickname: me.nickname.filter(|name| !name.trim().is_empty()),
            page: me.page.filter(|page| !page.trim().is_empty()),
        })
    }

    pub(crate) async fn donates_page(
        &self,
        token: &DonatelloToken,
        page: u64,
    ) -> Result<DonatesPageWire, DonatelloError> {
        self.get(
            token,
            DONATES_PATH,
            &[(PAGE_PARAM, page), (SIZE_PARAM, PAGE_SIZE)],
        )
        .await
    }

    async fn get<T: DeserializeOwned>(
        &self,
        token: &DonatelloToken,
        path: &str,
        query: &[(&str, u64)],
    ) -> Result<T, DonatelloError> {
        acquire_or_wait(self.limiter.as_ref(), REQUEST_WEIGHT)
            .await
            .map_err(DonatelloError::from_limiter)?;
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .header(TOKEN_HEADER, token.header_value()?)
            .query(query)
            .send()
            .await
            .map_err(DonatelloError::from_transport)?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(DonatelloError::Unauthorized);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after_secs = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u32>().ok())
                .unwrap_or(DEFAULT_RETRY_AFTER_SECS);
            self.limiter
                .observe_remote_throttle(Duration::from_secs(u64::from(retry_after_secs)))
                .await;
            return Err(DonatelloError::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            return Err(DonatelloError::Http {
                status: status.as_u16(),
            });
        }
        let body = response
            .bytes()
            .await
            .map_err(DonatelloError::from_transport)?;
        serde_json::from_slice(&body).map_err(|error| DonatelloError::from_decode(&error))
    }
}
