use std::sync::Arc;
use std::time::Duration;

use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, Gateway, GatewayProfile, GatewayRequest, RateLimiter,
    ThrottleHintFamily, TokenBucketRateLimiter,
};
use forge_types::Secret;
use serde::de::DeserializeOwned;

use crate::config::DonatelloConfig;
use crate::error::DonatelloError;
use crate::wire::{DonatesPageWire, MeWire};

const TOKEN_HEADER: &str = "X-Token";
const ME_PATH: &str = "/me";
const DONATES_PATH: &str = "/donates";
const PAGE_PARAM: &str = "page";
const SIZE_PARAM: &str = "size";
const NOT_FOUND_STATUS: u16 = 404;
const RATE_LIMIT_BURST: u32 = 20;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

pub(crate) const PAGE_SIZE: u64 = 20;

pub const DONATELLO_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn default_rate_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(TokenBucketRateLimiter::new(
        RATE_LIMIT_BURST,
        RATE_LIMIT_WINDOW,
    ))
}

pub fn donatello_gateway(
    config: &DonatelloConfig,
    limiter: Arc<dyn RateLimiter>,
) -> Result<Gateway, DonatelloError> {
    Gateway::new(&config.base_url, limiter, DONATELLO_GATEWAY_PROFILE, None).map_err(|error| {
        DonatelloError::ClientInit {
            reason: error.to_string(),
        }
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DonatelloAccount {
    pub nickname: Option<String>,
    pub page: Option<String>,
}

pub(crate) struct DonatelloApi {
    gateway: Gateway,
}

impl DonatelloApi {
    pub(crate) fn new(gateway: Gateway) -> Self {
        Self { gateway }
    }

    pub(crate) async fn account(&self, token: &Secret) -> Result<DonatelloAccount, DonatelloError> {
        let me: MeWire = match self.get(token, ME_PATH, &[]).await {
            Err(DonatelloError::Http { status }) if status == NOT_FOUND_STATUS => {
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
        token: &Secret,
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
        token: &Secret,
        path: &str,
        query: &[(&str, u64)],
    ) -> Result<T, DonatelloError> {
        let request = query.iter().fold(
            GatewayRequest::get(path)
                .api_key(TOKEN_HEADER, token.clone())
                .follow_redirects(false),
            |request, (name, value)| request.query(*name, value.to_string()),
        );
        let response = self.gateway.send(request).await?;
        response.json().map_err(DonatelloError::from_decode)
    }
}
