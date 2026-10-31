use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_CREATE_MARKER;
const MAX_DESCRIPTION_CHARS: i64 = 140;

const DESCRIPTION: BoundedTextField = BoundedTextField {
    key: config_keys::DESCRIPTION,
    label: "Description (optional)",
    editor: TextEditor::Area,
    default: "",
    required: false,
    length: ValueBounds {
        min: None,
        max: Some(MAX_DESCRIPTION_CHARS),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&DESCRIPTION]);

struct Marker {
    id: String,
    position_seconds: i64,
    created_at: Option<Variant>,
}

pub(crate) struct CreateMarkerRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CreateMarkerRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn create(&self, description: &str) -> Result<Marker, SubActionOutcome> {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => {
                return Err(step_failure::failed(&e));
            }
        };
        let mut body = serde_json::Map::new();
        if !description.is_empty() {
            body.insert("description".to_owned(), description.into());
        }
        let request = HelixRequest::new(HelixMethod::Post, "/helix/streams/markers")
            .query("broadcaster_id", user_id)
            .body(serde_json::Value::Object(body));
        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| step_failure::failed(&e))?;
        let marker = resp["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| SubActionOutcome::Failed(messages::empty_response().into()))?;
        let id = marker["id"]
            .as_str()
            .ok_or_else(|| SubActionOutcome::Failed(messages::response_field_missing("id").into()))?
            .to_owned();
        let position_seconds = marker["position_seconds"].as_i64().ok_or_else(|| {
            SubActionOutcome::Failed(messages::response_field_missing("position_seconds").into())
        })?;
        let created_at = marker["created_at"]
            .as_str()
            .and_then(crate::timestamp::variant);
        Ok(Marker {
            id,
            position_seconds,
            created_at,
        })
    }
}

#[async_trait]
impl SubActionRunner for CreateMarkerRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Create Stream Marker"
    }

    fn summary(&self) -> &str {
        "Places a bookmark at the current live stream position."
    }

    fn search_text(&self) -> &str {
        "twitch stream marker bookmark position highlight timestamp"
    }

    fn icon_name(&self) -> &str {
        "flag"
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
        let description = match DESCRIPTION.read(config, ctx.arg_stack) {
            Ok(description) => description,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        match self.create(&description).await {
            Ok(marker) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("marker.id".to_owned(), Variant::String(marker.id))
                    .set(
                        "marker.position_seconds".to_owned(),
                        Variant::Int(marker.position_seconds),
                    );
                let output_stack =
                    crate::timestamp::set(output_stack, "marker.created_at", marker.created_at);
                (timer.success(), Some(output_stack))
            }
            Err(outcome) => (timer.finish(outcome), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };
    use crate::timestamp::tests::instant;
    use std::collections::BTreeMap;

    fn marker_payload() -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "id": "123",
                "position_seconds": 244,
                "created_at": "2026-06-13T12:00:00Z",
                "description": "ignored"
            }]
        })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, CreateMarkerRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = CreateMarkerRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        (transport, runner)
    }

    fn cfg(description: &str) -> SubActionConfig {
        BTreeMap::from([(
            "description".to_owned(),
            Variant::String(description.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_posts_marker_with_description_and_pushes_outputs() {
        let (transport, runner) = runner_with(Ok(marker_payload()));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(&cfg("clutch moment"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/streams/markers");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({ "description": "clutch moment" })
        );

        let out = output.unwrap();
        assert_eq!(
            out.get("marker.id"),
            Some(&Variant::String("123".to_owned()))
        );
        assert_eq!(out.get("marker.position_seconds"), Some(&Variant::Int(244)));
        assert_eq!(
            out.get("marker.created_at"),
            Some(&instant("2026-06-13T12:00:00Z"))
        );
    }

    #[tokio::test]
    async fn a_marker_without_a_readable_creation_moment_still_succeeds_without_it() {
        for created_at in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!("")),
            Some(serde_json::json!("yesterday")),
        ] {
            let mut payload = marker_payload();
            match &created_at {
                Some(value) => payload["data"][0]["created_at"] = value.clone(),
                None => {
                    payload["data"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("created_at");
                }
            }
            let (_transport, runner) = runner_with(Ok(payload));
            let stack = ArgStack::new();

            let (telemetry, output) = runner.execute(&cfg("x"), &make_ctx(&stack)).await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Success,
                "{created_at:?}"
            );
            let out = output.unwrap();
            assert_eq!(out.get("marker.created_at"), None, "{created_at:?}");
            assert_eq!(
                out.get("marker.id"),
                Some(&Variant::String("123".to_owned())),
                "{created_at:?}"
            );
        }
    }

    #[tokio::test]
    async fn empty_description_omits_body_field() {
        let (transport, runner) = runner_with(Ok(marker_payload()));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg(""), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.request(0).body.unwrap(), serde_json::json!({}));
    }

    #[tokio::test]
    async fn missing_data_array_maps_to_failed() {
        let (_transport, runner) = runner_with(Ok(serde_json::json!({ "data": [] })));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&cfg("x"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
    }

    #[tokio::test]
    async fn description_boundary_at_140_sends_over_140_fails() {
        for (label, len, expect_call) in [("exactly 140", 140, true), ("141 chars", 141, false)] {
            let (transport, runner) = runner_with(Ok(marker_payload()));
            let stack = ArgStack::new();
            let desc = "d".repeat(len);

            let (telemetry, _) = runner.execute(&cfg(&desc), &make_ctx(&stack)).await;

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
            status: 404,
            body: "stream offline".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&cfg("x"), &make_ctx(&stack)).await;

        assert!(output.is_none());
        assert_request_refused(&telemetry.outcome, 404);
    }

    #[test]
    fn validate_config_rejects_only_oversize_description() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("no description", BTreeMap::new(), true),
            ("empty description", cfg(""), true),
            ("at 140 chars", cfg(&"x".repeat(140)), true),
            ("over 140 chars", cfg(&"x".repeat(141)), false),
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
