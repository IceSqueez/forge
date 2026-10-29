use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, SubActionCategory, SubActionRunner, TextEditor, TextField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::lock_prediction::{PREDICTION_ID, PredictionUpdate};
use crate::account::identity::SelfIdentity;
use crate::helix::HelixTransport;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::PREDICTION_RESOLVE;

const WINNING_OUTCOME_ID: TextField = TextField {
    key: config_keys::WINNING_OUTCOME_ID,
    label: "Winning Outcome ID",
    editor: TextEditor::Line {
        placeholder: "%prediction.outcome.id%",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PREDICTION_ID, &WINNING_OUTCOME_ID]);

pub struct ResolvePredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ResolvePredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ResolvePredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "Resolve Prediction"
    }

    fn summary(&self) -> &str {
        "Resolves a prediction by declaring a winning outcome and distributing points to winners."
    }

    fn search_text(&self) -> &str {
        "twitch prediction resolve win outcome declare result"
    }

    fn icon_name(&self) -> &str {
        "trophy"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        PredictionUpdate {
            kind_id: KIND_ID,
            status: "RESOLVED",
            winning_outcome: Some(&WINNING_OUTCOME_ID),
        }
        .execute(&self.transport, &self.identity, config, ctx)
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome};

    use super::*;
    use crate::helix::HelixMethod;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, SELF_USER_ID, make_ctx};

    fn resolve_runner() -> (Arc<MockTransport>, ResolvePredictionRunner) {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = ResolvePredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(prediction_id: &str, winning_outcome_id: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "prediction_id".to_owned(),
                Variant::String(prediction_id.to_owned()),
            ),
            (
                "winning_outcome_id".to_owned(),
                Variant::String(winning_outcome_id.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn resolve_sends_resolved_body_with_winning_outcome_id() {
        let (transport, runner) = resolve_runner();
        let stack = ArgStack::new()
            .set(
                "prediction.id".to_owned(),
                Variant::String("pr42".to_owned()),
            )
            .set(
                "prediction.outcome.id".to_owned(),
                Variant::String("outA".to_owned()),
            );

        let (telemetry, _) = runner
            .execute(
                &cfg("%prediction.id%", "%prediction.outcome.id%"),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster_id must be self in the query: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({
                "id": "pr42",
                "status": "RESOLVED",
                "winning_outcome_id": "outA"
            })),
            "resolve must send RESOLVED with the interpolated winning_outcome_id"
        );
    }

    #[tokio::test]
    async fn winning_outcome_id_interpolates_from_its_own_template() {
        let (transport, runner) = resolve_runner();
        let stack = ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String("p1".to_owned()))
            .set("chosen".to_owned(), Variant::String("win7".to_owned()));

        let _ = runner
            .execute(&cfg("%prediction.id%", "%chosen%"), &make_ctx(&stack))
            .await;

        let body = transport.request(0).body.unwrap();
        assert_eq!(
            body.get("winning_outcome_id").and_then(|v| v.as_str()),
            Some("win7"),
            "winning_outcome_id must interpolate from its own template"
        );
    }

    #[tokio::test]
    async fn empty_winning_outcome_id_fails_without_helix_call() {
        let (transport, runner) = resolve_runner();

        let (telemetry, _) = runner
            .execute(&cfg("pr42", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty winning_outcome_id must short-circuit before PATCH"
        );
    }

    #[test]
    fn validate_config_requires_both_ids_non_empty() {
        let (_transport, runner) = resolve_runner();

        let cases = [
            ("both present", cfg("pred-1", "outcome-a"), true),
            ("empty prediction_id", cfg("", "outcome-a"), false),
            ("empty winning_outcome_id", cfg("pred-1", ""), false),
            ("both empty", cfg("", ""), false),
            (
                "missing winning_outcome_id",
                BTreeMap::from([(
                    "prediction_id".to_owned(),
                    Variant::String("pred-1".to_owned()),
                )]),
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
