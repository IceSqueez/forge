use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.update_category";
const CATEGORY_ID_KEY: &str = "category_id";
const CATEGORY_NAME_KEY: &str = "category_name";

pub struct UpdateCategoryRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateCategoryRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn resolve_category_id(&self, category_name: &str) -> Result<String, HelixError> {
        let request =
            HelixRequest::new(HelixMethod::Get, "/helix/games").query("name", category_name);
        let resp = self.transport.execute(request).await?;
        resp["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|game| game["id"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| HelixError::Http {
                status: forge_net::status::NOT_FOUND,
                body: format!("category not found: {category_name}"),
            })
    }

    async fn apply(&self, category_id: &str, category_name: &str) -> SubActionOutcome {
        let category_id = category_id.trim();
        let category_name = category_name.trim();
        if category_id.is_empty() && category_name.is_empty() {
            return SubActionOutcome::Failed(
                "category_id and category_name are empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let category_id = if category_id.is_empty() {
            match self.resolve_category_id(category_name).await {
                Ok(id) => id,
                Err(e) => return SubActionOutcome::Failed(e.to_string()),
            }
        } else {
            category_id.to_owned()
        };
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "game_id": category_id }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for UpdateCategoryRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Category"
    }

    fn summary(&self) -> &str {
        "Changes the broadcaster's game/category by its Helix game_id or exact category name."
    }

    fn search_text(&self) -> &str {
        "twitch channel category game update broadcast"
    }

    fn icon_name(&self) -> &str {
        "game-controller"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (CATEGORY_ID_KEY.to_owned(), Variant::String(String::new())),
            (CATEGORY_NAME_KEY.to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: CATEGORY_ID_KEY,
                label: "Category ID",
                placeholder: "e.g. 509658",
            },
            FormField::Text {
                key: CATEGORY_NAME_KEY,
                label: "Category Name",
                placeholder: "e.g. Just Chatting",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        if config.str_nonempty(CATEGORY_ID_KEY).is_some()
            || config.str_nonempty(CATEGORY_NAME_KEY).is_some()
        {
            return Ok(());
        }
        Err(RegistryError::invalid_field(
            CATEGORY_ID_KEY,
            ConfigReason::Required,
        ))
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let interpolate = |key: &str| {
            config
                .str(key)
                .map(|template| ctx.arg_stack.interpolate(template))
                .unwrap_or_default()
        };
        let category_id = interpolate(CATEGORY_ID_KEY);
        let category_name = interpolate(CATEGORY_NAME_KEY);

        let outcome = self.apply(&category_id, &category_name).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, UpdateCategoryRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = UpdateCategoryRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(pairs: &[(&str, &str)]) -> SubActionConfig {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), Variant::String((*v).to_owned())))
            .collect()
    }

    #[tokio::test]
    async fn execute_sends_game_id_and_omits_display_name() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(
                &cfg(&[
                    ("category_id", "509658"),
                    ("category_name", "Just Chatting"),
                ]),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        assert_eq!(transport.call_count(), 1, "id must skip the games lookup");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channels");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        let body = request.body.unwrap();
        assert_eq!(body, serde_json::json!({ "game_id": "509658" }));
        assert!(body.get("category_name").is_none());
        assert!(!body.to_string().contains("Just Chatting"));
    }

    #[tokio::test]
    async fn execute_interpolates_category_id_from_stack() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("gid".to_owned(), Variant::String("12345".to_owned()));

        let (telemetry, _) = runner
            .execute(&cfg(&[("category_id", "%gid%")]), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "game_id": "12345" })
        );
    }

    #[tokio::test]
    async fn blank_category_id_and_name_fail_without_helix_call() {
        for config in [
            cfg(&[("category_id", "")]),
            cfg(&[("category_id", "  "), ("category_name", " ")]),
        ] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

            assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
            assert_eq!(transport.call_count(), 0);
        }
    }

    #[tokio::test]
    async fn category_name_resolves_through_games_lookup_then_patches_returned_id() {
        let transport = Arc::new(MockTransport::returning_sequence(vec![
            Ok(serde_json::json!({ "data": [{ "id": "509658", "name": "Just Chatting" }] })),
            Ok(serde_json::Value::Null),
        ]));
        let runner = UpdateCategoryRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(
                &cfg(&[("category_name", "Just Chatting")]),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let lookup = transport.request(0);
        assert_eq!(
            (lookup.method, lookup.path, lookup.query),
            (
                HelixMethod::Get,
                "/helix/games",
                vec![("name".to_owned(), "Just Chatting".to_owned())]
            )
        );
        let patch = transport.request(1);
        assert_eq!(patch.path, "/helix/channels");
        assert_eq!(
            patch.body.unwrap(),
            serde_json::json!({ "game_id": "509658" })
        );
    }

    #[tokio::test]
    async fn unknown_category_name_fails_with_not_found_and_skips_the_patch() {
        let (transport, runner) = runner_with(Ok(serde_json::json!({ "data": [] })));
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(
                &cfg(&[("category_name", "No Such Game")]),
                &make_ctx(&stack),
            )
            .await;

        assert!(
            matches!(
                &telemetry.outcome,
                SubActionOutcome::Failed(msg) if msg.contains("404") && msg.contains("No Such Game")
            ),
            "{:?}",
            telemetry.outcome
        );
        assert_eq!(transport.call_count(), 1);
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid game_id".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&cfg(&[("category_id", "509658")]), &make_ctx(&stack))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("400") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[test]
    fn validate_config_requires_a_non_empty_category_id_or_name() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid id", cfg(&[("category_id", "509658")]), true),
            (
                "name only",
                cfg(&[("category_id", ""), ("category_name", "Just Chatting")]),
                true,
            ),
            ("empty id", cfg(&[("category_id", "")]), false),
            (
                "both empty",
                cfg(&[("category_id", ""), ("category_name", "")]),
                false,
            ),
            ("missing id", BTreeMap::new(), false),
            (
                "non-string id",
                BTreeMap::from([("category_id".to_owned(), Variant::Int(509658))]),
                false,
            ),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
