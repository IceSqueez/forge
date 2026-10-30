use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::PREDICTION_LOCK;
const PREDICTION_ID_TEMPLATE: &str = "%prediction.id%";

pub(crate) const PREDICTION_ID: TextField = TextField {
    key: config_keys::PREDICTION_ID,
    label: "Prediction ID",
    editor: TextEditor::Line {
        placeholder: PREDICTION_ID_TEMPLATE,
    },
    default: PREDICTION_ID_TEMPLATE,
    required: true,
};

pub(crate) const PREDICTION_CONFIG: ConfigSpec = ConfigSpec::new(&[&PREDICTION_ID]);

pub struct LockPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl LockPredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) struct PredictionUpdate<'a> {
    pub(crate) kind_id: &'a str,
    pub(crate) status: &'a str,
    pub(crate) winning_outcome: Option<&'a TextField>,
}

struct ResolvedUpdate {
    prediction_id: String,
    winning_outcome_id: Option<String>,
}

impl PredictionUpdate<'_> {
    fn read(
        &self,
        config: &SubActionConfig,
        args: &ArgStack,
    ) -> Result<ResolvedUpdate, ConfigProblem> {
        Ok(ResolvedUpdate {
            prediction_id: PREDICTION_ID.read(config, args)?,
            winning_outcome_id: self
                .winning_outcome
                .map(|field| field.read(config, args))
                .transpose()?,
        })
    }

    async fn patch(
        &self,
        transport: &Arc<dyn HelixTransport>,
        identity: &Arc<SelfIdentity>,
        resolved: ResolvedUpdate,
    ) -> SubActionOutcome {
        if resolved.prediction_id.is_empty() {
            return SubActionOutcome::Failed(messages::field_required("prediction_id").into());
        }
        if resolved
            .winning_outcome_id
            .as_ref()
            .is_some_and(String::is_empty)
        {
            return SubActionOutcome::Failed(messages::field_required("winning_outcome_id").into());
        }
        let user_id = match identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let mut body = serde_json::Map::new();
        body.insert("id".to_owned(), resolved.prediction_id.into());
        body.insert("status".to_owned(), self.status.into());
        if let Some(winning_outcome_id) = resolved.winning_outcome_id {
            body.insert("winning_outcome_id".to_owned(), winning_outcome_id.into());
        }
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/predictions")
            .query("broadcaster_id", user_id)
            .body(serde_json::Value::Object(body));
        match transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => step_failure::failed(&e),
        }
    }

    pub(crate) async fn execute(
        &self,
        transport: &Arc<dyn HelixTransport>,
        identity: &Arc<SelfIdentity>,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.kind_id);
        let resolved = match self.read(config, ctx.arg_stack) {
            Ok(resolved) => resolved,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (
            timer.finish(self.patch(transport, identity, resolved).await),
            None,
        )
    }
}

#[async_trait]
impl SubActionRunner for LockPredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "Lock Prediction"
    }

    fn summary(&self) -> &str {
        "Locks an active prediction, preventing new votes. Viewers can no longer change their outcome."
    }

    fn search_text(&self) -> &str {
        "twitch prediction lock stop votes"
    }

    fn icon_name(&self) -> &str {
        "lock"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(PREDICTION_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        PredictionUpdate {
            kind_id: KIND_ID,
            status: "LOCKED",
            winning_outcome: None,
        }
        .execute(&self.transport, &self.identity, config, ctx)
        .await
    }
}

#[cfg(test)]
mod tests {
    use forge_types::SubActionOutcome;

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn lock_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, LockPredictionRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = LockPredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn prediction_stack() -> ArgStack {
        ArgStack::new().set(
            "prediction.id".to_owned(),
            Variant::String("pr42".to_owned()),
        )
    }

    #[tokio::test]
    async fn lock_patches_predictions_with_broadcaster_query_and_locked_body() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));

        let (telemetry, out) = runner
            .execute(
                &PREDICTION_CONFIG.default_config(),
                &make_ctx(&prediction_stack()),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "prediction runners never push an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/predictions");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster_id must be self in the query, not the body: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "id": "pr42", "status": "LOCKED" })),
            "lock body must be id+LOCKED with no winning_outcome_id key"
        );
    }

    #[tokio::test]
    async fn prediction_id_interpolates_from_template() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "prediction.id".to_owned(),
            Variant::String("xyz9".to_owned()),
        );

        let _ = runner
            .execute(&PREDICTION_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        let body = transport.request(0).body.unwrap();
        assert_eq!(
            body.get("id").and_then(|v| v.as_str()),
            Some("xyz9"),
            "id must interpolate from %prediction.id%, not be sent verbatim"
        );
    }

    #[tokio::test]
    async fn empty_prediction_id_fails_without_helix_call() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([("prediction_id".to_owned(), Variant::String(String::new()))]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&ArgStack::new())).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty prediction_id must short-circuit before PATCH"
        );
    }

    #[test]
    fn validate_config_requires_a_non_blank_prediction_id_and_defaults_a_missing_one() {
        let (_transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present", PREDICTION_CONFIG.default_config(), true),
            (
                "empty prediction_id",
                BTreeMap::from([("prediction_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            (
                "missing key reads the declared template",
                BTreeMap::new(),
                true,
            ),
            (
                "whitespace only",
                BTreeMap::from([("prediction_id".to_owned(), Variant::String("  ".to_owned()))]),
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

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = lock_runner_with(Err(HelixError::Http {
            status: 401,
            body: "unauthorized".to_owned(),
        }));

        let (telemetry, _) = runner
            .execute(
                &PREDICTION_CONFIG.default_config(),
                &make_ctx(&prediction_stack()),
            )
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if !msg.code_or_text().contains(TOKEN_SENTINEL)
        ));
    }
}
