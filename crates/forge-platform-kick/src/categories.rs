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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use forge_platform_core::RateLimitOutcome;
    use std::time::Duration;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    struct ExhaustedLimiter;
    #[async_trait]
    impl RateLimiter for ExhaustedLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Exhausted)
        }
        fn remaining(&self) -> u32 {
            0
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn categories_on(server: &MockServer) -> KickCategories {
        KickCategories::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
    }

    // CategoryMatch is not Debug, so `unwrap_err` is unavailable on a search result.
    fn expect_err(result: Result<Vec<CategoryMatch>, PlatformError>) -> PlatformError {
        match result {
            Ok(matches) => panic!("expected an error, got {} matches", matches.len()),
            Err(e) => e,
        }
    }

    async fn mount_categories(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn search_passes_the_query_and_maps_id_and_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/categories"))
            .and(query_param("q", "just chatting"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": 15, "name": "Just Chatting"}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let matches = categories_on(&server)
            .search("tok", "just chatting")
            .await
            .unwrap();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, 15);
        assert_eq!(matches[0].name, "Just Chatting");
    }

    #[tokio::test]
    async fn search_truncates_an_oversized_result_set() {
        let server = MockServer::start().await;
        let data: Vec<serde_json::Value> = (0..MAX_MATCHES + 5)
            .map(|i| serde_json::json!({"id": i, "name": format!("cat {i}")}))
            .collect();
        mount_categories(&server, serde_json::json!({ "data": data })).await;

        let matches = categories_on(&server).search("tok", "cat").await.unwrap();

        assert_eq!(matches.len(), MAX_MATCHES);
        assert_eq!(matches[MAX_MATCHES - 1].id, (MAX_MATCHES - 1) as u64);
    }

    #[tokio::test]
    async fn search_with_no_results_returns_an_empty_list_not_an_error() {
        let server = MockServer::start().await;
        mount_categories(&server, serde_json::json!({ "data": [] })).await;

        let matches = categories_on(&server)
            .search("tok", "nothing")
            .await
            .unwrap();

        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn search_maps_error_statuses_to_typed_errors() {
        type Check = fn(&PlatformError) -> bool;
        let cases: [(u16, Option<&str>, Check, &str); 3] = [
            (
                401,
                None,
                |e| matches!(e, PlatformError::Auth { .. }),
                "Auth",
            ),
            (
                429,
                Some("42"),
                |e| {
                    matches!(
                        e,
                        PlatformError::RateLimited {
                            retry_after_secs: 42
                        }
                    )
                },
                "RateLimited(42)",
            ),
            (
                500,
                None,
                |e| matches!(e, PlatformError::Http { status: 500, .. }),
                "Http(500)",
            ),
        ];

        for (status, retry_after, check, expected) in cases {
            let server = MockServer::start().await;
            let mut template = ResponseTemplate::new(status).set_body_string("nope");
            if let Some(retry_after) = retry_after {
                template = template.insert_header("retry-after", retry_after);
            }
            Mock::given(method("GET"))
                .and(path("/categories"))
                .respond_with(template)
                .mount(&server)
                .await;

            let err = expect_err(categories_on(&server).search("tok", "q").await);

            assert!(
                check(&err),
                "status {status} must map to {expected}: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn exhausted_limiter_short_circuits_before_any_request() {
        let server = MockServer::start().await;
        mount_categories(&server, serde_json::json!({ "data": [] })).await;

        let client = KickCategories::new(Arc::new(ExhaustedLimiter)).with_api_base(server.uri());
        let err = expect_err(client.search("tok", "q").await);

        assert!(matches!(err, PlatformError::RateLimitExhausted));
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
