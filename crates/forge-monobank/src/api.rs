use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{RateLimitOutcome, RateLimiter, TokenBucketRateLimiter};
use reqwest::StatusCode;
use reqwest::header::RETRY_AFTER;
use serde::de::DeserializeOwned;

use crate::config::{MonobankConfig, STATEMENT_CALL_INTERVAL};
use crate::error::MonobankError;
use crate::jar::{JarId, MonobankJar};
use crate::token::MonobankToken;
use crate::wire::ClientInfoWire;

const TOKEN_HEADER: &str = "X-Token";
const CLIENT_INFO_PATH: &str = "/personal/client-info";
const STATEMENT_PATH: &str = "/personal/statement";
const REQUEST_WEIGHT: u32 = 1;
const CALLS_PER_INTERVAL: u32 = 1;

#[derive(Clone)]
pub struct MonobankRateLimits {
    pub client_info: Arc<dyn RateLimiter>,
    pub statement: Arc<dyn RateLimiter>,
}

impl MonobankRateLimits {
    pub fn official() -> Self {
        let per_call = || -> Arc<dyn RateLimiter> {
            Arc::new(TokenBucketRateLimiter::new(
                CALLS_PER_INTERVAL,
                STATEMENT_CALL_INTERVAL,
            ))
        };
        Self {
            client_info: per_call(),
            statement: per_call(),
        }
    }
}

pub(crate) struct MonobankApi {
    http: reqwest::Client,
    base_url: String,
    limits: MonobankRateLimits,
}

impl MonobankApi {
    pub(crate) fn new(
        config: &MonobankConfig,
        limits: MonobankRateLimits,
    ) -> Result<Self, MonobankError> {
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| MonobankError::ClientInit {
                reason: error.without_url().to_string(),
            })?;
        Ok(Self {
            http,
            base_url: config.base_url.clone(),
            limits,
        })
    }

    pub(crate) async fn jars(
        &self,
        token: &MonobankToken,
    ) -> Result<Vec<MonobankJar>, MonobankError> {
        let info: ClientInfoWire = self
            .get(
                token,
                self.limits.client_info.as_ref(),
                CLIENT_INFO_PATH.to_owned(),
            )
            .await?;
        Ok(info
            .jars
            .into_iter()
            .filter_map(MonobankJar::from_wire)
            .collect())
    }

    pub(crate) async fn statement(
        &self,
        token: &MonobankToken,
        jar: &JarId,
        from_unix: i64,
        to_unix: i64,
    ) -> Result<Vec<serde_json::Value>, MonobankError> {
        self.get(
            token,
            self.limits.statement.as_ref(),
            format!("{STATEMENT_PATH}/{}/{from_unix}/{to_unix}", jar.as_str()),
        )
        .await
    }

    async fn get<T: DeserializeOwned>(
        &self,
        token: &MonobankToken,
        limiter: &dyn RateLimiter,
        path: String,
    ) -> Result<T, MonobankError> {
        reserve_call(limiter).await?;
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .header(TOKEN_HEADER, token.header_value()?)
            .send()
            .await
            .map_err(MonobankError::from_transport)?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(MonobankError::Unauthorized);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(STATEMENT_CALL_INTERVAL)
                .max(STATEMENT_CALL_INTERVAL);
            limiter.observe_remote_throttle(retry_after).await;
            return Err(MonobankError::RateLimited {
                retry_after_secs: whole_seconds(retry_after),
            });
        }
        if !status.is_success() {
            return Err(MonobankError::Http {
                status: status.as_u16(),
            });
        }
        let body = response
            .bytes()
            .await
            .map_err(MonobankError::from_transport)?;
        serde_json::from_slice(&body).map_err(|error| MonobankError::from_decode(&error))
    }
}

async fn reserve_call(limiter: &dyn RateLimiter) -> Result<(), MonobankError> {
    match limiter
        .acquire(REQUEST_WEIGHT)
        .await
        .map_err(MonobankError::from_limiter)?
    {
        RateLimitOutcome::Granted => Ok(()),
        RateLimitOutcome::Throttled { wait_for } => Err(MonobankError::CoolingDown {
            retry_after_secs: whole_seconds(wait_for),
        }),
        RateLimitOutcome::Exhausted => Err(MonobankError::CoolingDown {
            retry_after_secs: whole_seconds(STATEMENT_CALL_INTERVAL),
        }),
    }
}

pub(crate) fn whole_seconds(duration: Duration) -> u32 {
    let rounded_up = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    u32::try_from(rounded_up).unwrap_or(u32::MAX)
}
