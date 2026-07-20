use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.update_category";

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

    async fn apply(&self, category_id: &str) -> SubActionOutcome {
        if category_id.is_empty() {
            return SubActionOutcome::Failed("category_id is empty after interpolation".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "game_id": category_id }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
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
        "Changes the broadcaster's game/category by its Helix game_id."
    }

    fn search_text(&self) -> &str {
        "twitch channel category game update broadcast"
    }

    fn icon_name(&self) -> &str {
        "game-controller"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("category_id".to_owned(), Variant::String(String::new())),
            // Display-only label; runtime sends category_id, not this string.
            ("category_name".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "category_id",
                label: "Category ID",
                placeholder: "e.g. 509658",
            },
            FormField::Text {
                key: "category_name",
                label: "Category Name (display only)",
                placeholder: "e.g. Just Chatting",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("category_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'category_id' must be a non-empty string"
                )));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config
            .get("category_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let category_id = ctx.arg_stack.interpolate(template);

        let outcome = self.apply(&category_id).await;

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
#[allow(clippy::unwrap_used)]
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
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channels");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        // Body carries game_id ONLY - category_name is display-only and must not leak.
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
    async fn empty_category_id_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&cfg(&[("category_id", "")]), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
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
    fn validate_config_requires_non_empty_category_id() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid id", cfg(&[("category_id", "509658")]), true),
            ("empty id", cfg(&[("category_id", "")]), false),
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
