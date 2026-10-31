use std::sync::Arc;
use std::time::Duration;

use forge_net::{
    CallClass, Gateway, GatewayProfile, GatewayRequest, RateLimiter, ThrottleHintFamily,
    TokenBucketRateLimiter,
};
use forge_types::Secret;
use serde::de::DeserializeOwned;

use crate::config::{MonobankConfig, STATEMENT_CALL_INTERVAL};
use crate::error::MonobankError;
use crate::jar::{JarId, MonobankJar};
use crate::wire::ClientInfoWire;

const TOKEN_HEADER: &str = "X-Token";
const CLIENT_INFO_PATH: &str = "/personal/client-info";
const STATEMENT_PATH: &str = "/personal/statement";
const CALLS_PER_INTERVAL: u32 = 1;

pub(crate) const MONOBANK_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::Interactive,
    ThrottleHintFamily::RetryAfter,
    STATEMENT_CALL_INTERVAL,
);

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

#[derive(Clone)]
pub struct MonobankGateways {
    client_info: Gateway,
    statement: Gateway,
}

pub fn monobank_gateways(
    config: &MonobankConfig,
    limits: MonobankRateLimits,
) -> Result<MonobankGateways, MonobankError> {
    let build = |limiter: Arc<dyn RateLimiter>| {
        Gateway::new(&config.base_url, limiter, MONOBANK_GATEWAY_PROFILE, None).map_err(|error| {
            MonobankError::ClientInit {
                reason: error.to_string(),
            }
        })
    };
    Ok(MonobankGateways {
        client_info: build(limits.client_info)?,
        statement: build(limits.statement)?,
    })
}

pub(crate) struct MonobankApi {
    gateways: MonobankGateways,
}

impl MonobankApi {
    pub(crate) fn new(gateways: MonobankGateways) -> Self {
        Self { gateways }
    }

    pub(crate) async fn jars(&self, token: &Secret) -> Result<Vec<MonobankJar>, MonobankError> {
        let info: ClientInfoWire = self
            .get(
                &self.gateways.client_info,
                token,
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
        token: &Secret,
        jar: &JarId,
        from_unix: i64,
        to_unix: i64,
    ) -> Result<Vec<serde_json::Value>, MonobankError> {
        self.get(
            &self.gateways.statement,
            token,
            format!("{STATEMENT_PATH}/{}/{from_unix}/{to_unix}", jar.as_str()),
        )
        .await
    }

    async fn get<T: DeserializeOwned>(
        &self,
        gateway: &Gateway,
        token: &Secret,
        path: String,
    ) -> Result<T, MonobankError> {
        let request = GatewayRequest::get(path)
            .api_key(TOKEN_HEADER, token.clone())
            .follow_redirects(false)
            .without_limiter_wait();
        let response = gateway.send(request).await?;
        response.json().map_err(MonobankError::from_decode)
    }
}

pub(crate) fn whole_seconds(duration: Duration) -> u32 {
    let rounded_up = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    u32::try_from(rounded_up).unwrap_or(u32::MAX)
}
