use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use futures::future::BoxFuture;

use super::step_failure;
use crate::api::categories::KickCategories;
use crate::vocabulary::variables;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::LOOKUP_CATEGORY;

const QUERY: TextField = TextField {
    key: config_keys::QUERY,
    label: "Search Query",
    editor: TextEditor::Line {
        placeholder: "just chatting",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&QUERY]);

pub(crate) struct LookupCategoryRunner {
    client: Arc<KickCategories>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl LookupCategoryRunner {
    pub(crate) fn new(
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let query = match QUERY.read(config, ctx.arg_stack) {
            Ok(query) => query,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let token = match (self.token_source)().await {
            Ok(token) => token,
            Err(e) => {
                return (timer.failed(step_failure::token_failed(&e)), None);
            }
        };
        match self.client.search(&token, &query).await {
            Ok(matches) => {
                let items: Vec<Variant> = matches
                    .into_iter()
                    .map(|m| {
                        Variant::Object(BTreeMap::from([
                            (
                                variables::category::MATCH_ID.to_owned(),
                                Variant::Int(i64::try_from(m.id).unwrap_or(i64::MAX)),
                            ),
                            (
                                variables::category::MATCH_NAME.to_owned(),
                                Variant::String(m.name),
                            ),
                        ]))
                    })
                    .collect();
                let stack = ctx.arg_stack.clone().set(
                    variables::category::MATCHES.to_owned(),
                    Variant::Array(items),
                );
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.failed(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{make_ctx, token_source};
    use forge_types::SubActionOutcome;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> LookupCategoryRunner {
        let categories = KickCategories::new(crate::endpoints_test_support::api_at(&server.uri()));
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                    "data": [
                        {"id": 15, "name": "Just Chatting"},
                        {"id": 21, "name": "Chess"}
                    ]
                })),
            )
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(serde_json::json!({"data": []})),
            )
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::UNAUTHORIZED)
                    .set_body_string("unauthorized"),
            )
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
