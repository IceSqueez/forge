use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_UPDATE_TITLE;
const MAX_TITLE_CHARS: i64 = 140;

const TITLE: BoundedTextField = BoundedTextField {
    key: config_keys::TITLE,
    label: "Title",
    editor: TextEditor::Area,
    default: "",
    required: true,
    length: ValueBounds {
        min: None,
        max: Some(MAX_TITLE_CHARS),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE]);

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
            return SubActionOutcome::Failed(UserText::unlocalized(
                "title is empty after interpolation",
            ));
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
        };
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "title": title }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let title = match TITLE.read(config, ctx.arg_stack) {
            Ok(title) => title,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(&title).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
            SubActionOutcome::Failed(msg) if msg.code_or_text().contains("401") && !msg.code_or_text().contains(TOKEN_SENTINEL)
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
