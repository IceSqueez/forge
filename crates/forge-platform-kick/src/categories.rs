use std::sync::Arc;

use forge_platform_core::{PlatformError, RateLimiter, acquire_or_wait};
use serde::Deserialize;

const CATEGORIES_ENDPOINT: &str = "https://api.kick.com/public/v1/categories";
const MAX_MATCHES: usize = 10;

pub struct KickCategories {
    client: reqwest::Client,
    limiter: Arc<dyn RateLimiter>,
    categories_endpoint: String,
}

pub struct CategoryMatch {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct CategoriesEnvelope {
    #[serde(default)]
    data: Vec<CategoryData>,
}

#[derive(Deserialize, Default)]
struct CategoryData {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
}

impl KickCategories {
    pub fn new(limiter: Arc<dyn RateLimiter>) -> Self {
        Self {
            client: reqwest::Client::new(),
            limiter,
            categories_endpoint: CATEGORIES_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, base: String) -> Self {
        self.categories_endpoint = format!("{base}/categories");
        self
    }

    pub async fn search(
        &self,
        token: &str,
        query: &str,
    ) -> Result<Vec<CategoryMatch>, PlatformError> {
        self.acquire_slot().await?;

        let response = self
            .client
            .get(&self.categories_endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return map_categories_error(status, response).await;
        }

        let envelope: CategoriesEnvelope =
            response.json().await.map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        Ok(envelope
            .data
            .into_iter()
            .take(MAX_MATCHES)
            .map(|c| CategoryMatch {
                id: c.id,
                name: c.name,
            })
            .collect())
    }

    async fn acquire_slot(&self) -> Result<(), PlatformError> {
        acquire_or_wait(self.limiter.as_ref(), 1).await
    }
}

async fn map_categories_error<T>(
    status: u16,
    response: reqwest::Response,
) -> Result<T, PlatformError> {
    let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);

    let body = response.text().await.unwrap_or_default();

    match status {
        401 => Err(PlatformError::Auth {
            reason: "categories token rejected (401)".to_owned(),
        }),
        429 => Err(PlatformError::RateLimited { retry_after_secs }),
        _ => Err(PlatformError::Http { status, body }),
    }
}
