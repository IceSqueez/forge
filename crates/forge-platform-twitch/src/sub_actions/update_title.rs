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

const KIND_ID: &str = "twitch.channel.update_title";
const MAX_TITLE_CHARS: usize = 140;

pub struct UpdateTitleRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateTitleRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, title: &str) -> SubActionOutcome {
        if title.is_empty() {
            return SubActionOutcome::Failed("title is empty after interpolation".to_owned());
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return SubActionOutcome::Failed("title exceeds 140-character limit".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "title": title }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateTitleRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Title"
    }

    fn summary(&self) -> &str {
        "Updates the broadcaster's stream title."
    }

    fn search_text(&self) -> &str {
        "twitch channel title stream update broadcast"
    }

    fn icon_name(&self) -> &str {
        "pencil"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("title".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "title",
            label: "Title",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("title") {
            Some(Variant::String(s)) if !s.is_empty() && s.chars().count() <= MAX_TITLE_CHARS => {}
            Some(Variant::String(s)) if s.is_empty() => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must not be empty"
                )));
            }
            Some(Variant::String(_)) => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must be ≤{MAX_TITLE_CHARS} characters"
                )));
            }
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must be a non-empty string"
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
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let title = ctx.arg_stack.interpolate(template);

        let outcome = self.apply(&title).await;

        (
            SubActionTelemetry {
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
    ) -> (Arc<MockTransport>, UpdateTitleRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = UpdateTitleRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(title: &str) -> SubActionConfig {
        BTreeMap::from([("title".to_owned(), Variant::String(title.to_owned()))])
    }

    #[tokio::test]
    async fn execute_patches_channels_with_interpolated_title() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("game".to_owned(), Variant::String("chess".to_owned()));

        let (telemetry, output) = runner
            .execute(&cfg("Playing %game%"), &make_ctx(&stack))
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
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({ "title": "Playing chess" })
        );
    }

    #[tokio::test]
    async fn empty_title_after_interpolation_fails_without_helix_call() {
        // %missing% resolves to empty here because the template IS the whole value
        // and the stack has no binding; production must reject before any PATCH.
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg(""), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "must not call Helix on empty title"
        );
    }

    #[tokio::test]
    async fn title_boundary_at_140_chars_sends_over_140_fails() {
        for (label, len, expect_call) in [("exactly 140", 140, true), ("141 chars", 141, false)] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();
            let title = "a".repeat(len);

            let (telemetry, _) = runner.execute(&cfg(&title), &make_ctx(&stack)).await;

            match expect_call {
                true => {
                    assert_eq!(
                        telemetry.outcome,
                        SubActionOutcome::Success,
                        "case: {label}"
                    );
                    assert_eq!(transport.call_count(), 1, "case: {label}");
                }
                false => {
                    assert!(
                        matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                        "case: {label}"
                    );
                    assert_eq!(transport.call_count(), 0, "case: {label} must skip Helix");
                }
            }
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 401,
            body: "token expired".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg("Live now"), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("401") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[test]
    fn validate_config_rejects_empty_oversize_and_non_string() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid title", cfg("Just Chatting"), true),
            ("at 140 chars", cfg(&"x".repeat(140)), true),
            ("over 140 chars", cfg(&"x".repeat(141)), false),
            ("empty string", cfg(""), false),
            (
                "non-string",
                BTreeMap::from([("title".to_owned(), Variant::Int(7))]),
                false,
            ),
            ("missing key", BTreeMap::new(), false),
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
