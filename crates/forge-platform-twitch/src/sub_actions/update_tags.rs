use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, SeparatedListField, StepTimer, SubActionCategory, SubActionRunner,
    TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_UPDATE_TAGS;
const MAX_TAGS: i64 = 10;
const MAX_TAG_CHARS: i64 = 25;

const TAGS: SeparatedListField = SeparatedListField {
    key: config_keys::TAGS,
    label: "Tags (one per line or comma-separated)",
    default: "",
    count: ValueBounds {
        min: None,
        max: Some(MAX_TAGS),
    },
    item_length: ValueBounds {
        min: None,
        max: Some(MAX_TAG_CHARS),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TAGS]);

pub struct UpdateTagsRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateTagsRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, tags: Vec<String>) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
        };
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "tags": tags }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for UpdateTagsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Tags"
    }

    fn summary(&self) -> &str {
        "Replaces the broadcaster's stream tags (max 10, each ≤25 chars). Empty clears all tags."
    }

    fn search_text(&self) -> &str {
        "twitch channel tags update broadcast stream"
    }

    fn icon_name(&self) -> &str {
        "tag"
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
        let tags = match TAGS.read(config, ctx.arg_stack) {
            Ok(tags) => tags,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(tags).await), None)
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
    ) -> (Arc<MockTransport>, UpdateTagsRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = UpdateTagsRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(tags: &str) -> SubActionConfig {
        BTreeMap::from([("tags".to_owned(), Variant::String(tags.to_owned()))])
    }

    #[tokio::test]
    async fn execute_sends_parsed_tags_array() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(&cfg("Speedrun\nEnglish, Chill"), &make_ctx(&stack))
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
            serde_json::json!({ "tags": ["Speedrun", "English", "Chill"] })
        );
    }

    #[tokio::test]
    async fn empty_input_clears_tags_with_empty_array() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg(""), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 1);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "tags": [] })
        );
    }

    #[tokio::test]
    async fn oversize_tag_and_too_many_tags_fail_without_helix_call() {
        for (label, raw) in [
            ("tag over 25 chars", "a".repeat(26)),
            (
                "11 tags",
                (1..=11)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&cfg(&raw), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "case: {label}"
            );
            assert_eq!(transport.call_count(), 0, "case: {label} must skip Helix");
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 403,
            body: "missing scope".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg("Chill"), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.code_or_text().contains("403") && !msg.code_or_text().contains(TOKEN_SENTINEL)
        ));
    }
}
