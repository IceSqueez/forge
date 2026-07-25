use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::categories::KickCategories;

const KIND_ID: &str = "kick.lookup.category";

pub struct LookupCategoryRunner {
    client: Arc<KickCategories>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl LookupCategoryRunner {
    pub fn new(
        client: Arc<KickCategories>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for LookupCategoryRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Lookup Category"
    }

    fn summary(&self) -> &str {
        "Searches Kick categories by name and returns the matching numeric ids."
    }

    fn search_text(&self) -> &str {
        "kick lookup category search game id"
    }

    fn icon_name(&self) -> &str {
        "search"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("query".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "query",
            label: "Search Query",
            placeholder: "just chatting",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("query") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'query' must be a non-empty string"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config.str("query").unwrap_or_default();
        let query = ctx.arg_stack.interpolate(template);

        if query.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "query is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        let result = match (self.token_source)().await {
            Err(e) => Err(format!("token error: {e}")),
            Ok(token) => match self.client.search(&token, &query).await {
                Ok(matches) => {
                    let items: Vec<Variant> = matches
                        .into_iter()
                        .map(|m| {
                            Variant::Object(BTreeMap::from([
                                ("id".to_owned(), Variant::Int(m.id as i64)),
                                ("name".to_owned(), Variant::String(m.name)),
                            ]))
                        })
                        .collect();
                    let stack = ctx
                        .arg_stack
                        .clone()
                        .set("kick.category.matches".to_owned(), Variant::Array(items));
                    Ok(stack)
                }
                Err(e) => Err(e.to_string()),
            },
        };

        match result {
            Ok(stack) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Success,
                    index: ctx.index,
                },
                Some(stack),
            ),
            Err(msg) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(msg),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{GrantLimiter, make_ctx, token_source};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> LookupCategoryRunner {
        let categories = KickCategories::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        LookupCategoryRunner::new(Arc::new(categories), token_source())
    }

    fn config(query: &str) -> SubActionConfig {
        BTreeMap::from([("query".to_owned(), Variant::String(query.to_owned()))])
    }

    #[tokio::test]
    async fn matches_are_published_as_an_array_of_id_and_name_objects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    {"id": 15, "name": "Just Chatting"},
                    {"id": 21, "name": "Chess"}
                ]
            })))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&config("chat"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let matches = produced
            .expect("a successful search must return an arg stack")
            .get("kick.category.matches")
            .cloned()
            .expect("matches variable must be set");
        assert_eq!(
            matches,
            Variant::Array(vec![
                Variant::Object(BTreeMap::from([
                    ("id".to_owned(), Variant::Int(15)),
                    (
                        "name".to_owned(),
                        Variant::String("Just Chatting".to_owned())
                    ),
                ])),
                Variant::Object(BTreeMap::from([
                    ("id".to_owned(), Variant::Int(21)),
                    ("name".to_owned(), Variant::String("Chess".to_owned())),
                ])),
            ])
        );
    }

    #[tokio::test]
    async fn a_search_with_no_matches_succeeds_with_an_empty_array() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/categories"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": []})))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&config("nothing"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            produced.unwrap().get("kick.category.matches"),
            Some(&Variant::Array(Vec::new()))
        );
    }

    #[tokio::test]
    async fn empty_query_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        let stack = ArgStack::new().set("q".to_owned(), Variant::String(String::new()));

        let (telemetry, produced) = runner_on(&server)
            .execute(&config("%q%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn upstream_failure_is_reported_without_an_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/categories"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&config("chess"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
    }
}
