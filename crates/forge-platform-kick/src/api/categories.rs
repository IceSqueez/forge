use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;
use serde::Deserialize;

use crate::gateway::{RefusalReasons, authorized, malformed, platform_error};

const CATEGORIES_PATH: &str = "/categories";
const MAX_MATCHES: usize = 10;

const CATEGORIES_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "categories token rejected (401)",
    forbidden: None,
};

pub struct KickCategories {
    api: Gateway,
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
    id: Option<u64>,
    #[serde(default)]
    name: String,
}

impl KickCategories {
    pub fn new(api: Gateway) -> Self {
        Self { api }
    }

    pub async fn search(
        &self,
        token: &str,
        query: &str,
    ) -> Result<Vec<CategoryMatch>, PlatformError> {
        let request = authorized(GatewayRequest::get(CATEGORIES_PATH), token, CATEGORIES_PATH)
            .query("q", query);
        let response = self
            .api
            .send(request)
            .await
            .map_err(|failure| platform_error(failure, CATEGORIES_REFUSALS))?;
        let envelope: CategoriesEnvelope = response.json().map_err(malformed)?;

        Ok(envelope
            .data
            .into_iter()
            .filter_map(|c| c.id.map(|id| CategoryMatch { id, name: c.name }))
            .take(MAX_MATCHES)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn categories_on(server: &MockServer) -> KickCategories {
        KickCategories::new(crate::endpoints_test_support::api_at(&server.uri()))
    }

    async fn mount_categories(server: &MockServer, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/categories"))
            .respond_with(template)
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

        assert_eq!(
            matches
                .iter()
                .map(|m| (m.id, m.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(15, "Just Chatting")]
        );
    }

    #[tokio::test]
    async fn search_truncates_an_oversized_result_set() {
        let server = MockServer::start().await;
        let data: Vec<serde_json::Value> = (0..MAX_MATCHES + 5)
            .map(|i| serde_json::json!({"id": i, "name": format!("cat {i}")}))
            .collect();
        mount_categories(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data })),
        )
        .await;

        let matches = categories_on(&server).search("tok", "cat").await.unwrap();

        assert_eq!(matches.len(), MAX_MATCHES);
        assert_eq!(matches[MAX_MATCHES - 1].id, (MAX_MATCHES - 1) as u64);
    }

    #[tokio::test]
    async fn search_drops_rows_without_an_id_before_truncating() {
        let server = MockServer::start().await;
        let mut data = vec![
            serde_json::json!({"name": "no id"}),
            serde_json::json!({"id": null, "name": "null id"}),
        ];
        data.extend(
            (1..=MAX_MATCHES + 2).map(|i| serde_json::json!({"id": i, "name": format!("cat {i}")})),
        );
        mount_categories(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": data })),
        )
        .await;

        let matches = categories_on(&server).search("tok", "cat").await.unwrap();

        assert_eq!(
            matches.iter().map(|m| m.id).collect::<Vec<_>>(),
            (1..=MAX_MATCHES as u64).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn search_with_no_results_returns_an_empty_list_not_an_error() {
        let server = MockServer::start().await;
        mount_categories(
            &server,
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
        )
        .await;

        let matches = categories_on(&server)
            .search("tok", "nothing")
            .await
            .unwrap();

        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn search_reports_an_undecodable_success_body_as_malformed() {
        let server = MockServer::start().await;
        mount_categories(
            &server,
            ResponseTemplate::new(200).set_body_string("<html>"),
        )
        .await;

        let result = categories_on(&server).search("tok", "q").await;

        assert!(
            matches!(result, Err(PlatformError::MalformedResponse { .. })),
            "{:?}",
            result.map(|m| m.len())
        );
    }
}
