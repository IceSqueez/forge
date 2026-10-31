use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigProblem, ConfigSpec, DurationBounds, DurationField, RunContext,
    SeparatedListField, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TypedField,
    ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::PREDICTION_START;
const MAX_TITLE_CHARS: i64 = 45;
const MIN_OUTCOMES: i64 = 2;
const MAX_OUTCOMES: i64 = 10;
const MAX_OUTCOME_TITLE_CHARS: i64 = 25;
const DEFAULT_WINDOW_SECS: i64 = 120;
const MIN_WINDOW_SECS: i64 = 30;
const MAX_WINDOW_SECS: i64 = 1800;

const TITLE: BoundedTextField = BoundedTextField {
    key: config_keys::TITLE,
    label: "Question",
    editor: TextEditor::Line {
        placeholder: "Will I win this game?",
    },
    default: "",
    required: true,
    length: ValueBounds {
        min: None,
        max: Some(MAX_TITLE_CHARS),
    },
};

const OUTCOMES: SeparatedListField = SeparatedListField {
    key: config_keys::OUTCOMES,
    label: "Outcomes (one per line or comma-separated, 2-10)",
    default: "",
    count: ValueBounds {
        min: Some(MIN_OUTCOMES),
        max: Some(MAX_OUTCOMES),
    },
    item_length: ValueBounds {
        min: None,
        max: Some(MAX_OUTCOME_TITLE_CHARS),
    },
};

pub(crate) const PREDICTION_WINDOW: DurationField = DurationField {
    key: config_keys::PREDICTION_WINDOW_SECONDS,
    label: "Prediction Window",
    default: DEFAULT_WINDOW_SECS,
    bounds: DurationBounds::in_seconds(MIN_WINDOW_SECS, MAX_WINDOW_SECS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE, &OUTCOMES, &PREDICTION_WINDOW]);

struct ResolvedConfig {
    title: String,
    outcomes: Vec<String>,
    prediction_window_seconds: u64,
}

impl ResolvedConfig {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            title: TITLE.read(config, args)?,
            outcomes: OUTCOMES.read(config, args)?,
            prediction_window_seconds: PREDICTION_WINDOW.read(config, args)?.as_secs(),
        })
    }
}

pub(crate) struct StartPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl StartPredictionRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn start(&self, cfg: &ResolvedConfig) -> Result<String, SubActionOutcome> {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => {
                return Err(step_failure::failed(&e));
            }
        };
        let outcomes: Vec<serde_json::Value> = cfg
            .outcomes
            .iter()
            .map(|o| serde_json::json!({ "title": o }))
            .collect();
        let mut body = serde_json::Map::new();
        body.insert("title".to_owned(), cfg.title.clone().into());
        body.insert("outcomes".to_owned(), outcomes.into());
        body.insert(
            "prediction_window".to_owned(),
            cfg.prediction_window_seconds.into(),
        );
        let request = HelixRequest::new(HelixMethod::Post, "/helix/predictions")
            .query("broadcaster_id", user_id)
            .body(serde_json::Value::Object(body));
        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| step_failure::failed(&e))?;
        resp["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| SubActionOutcome::Failed(messages::empty_response().into()))
    }
}

#[async_trait]
impl SubActionRunner for StartPredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "Start Prediction"
    }

    fn summary(&self) -> &str {
        "Creates a new Twitch prediction. Outputs prediction.id for chaining."
    }

    fn search_text(&self) -> &str {
        "twitch prediction vote blue orange start create"
    }

    fn icon_name(&self) -> &str {
        "chart-pie"
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
        let resolved = match ResolvedConfig::read(config, ctx.arg_stack) {
            Ok(resolved) => resolved,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        match self.start(&resolved).await {
            Ok(prediction_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("prediction.id".to_owned(), Variant::String(prediction_id));
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
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };
    use std::collections::BTreeMap;

    fn prediction_payload() -> serde_json::Value {
        serde_json::json!({ "data": [{ "id": "pred-xyz", "title": "ignored" }] })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, StartPredictionRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = StartPredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn full_cfg() -> SubActionConfig {
        BTreeMap::from([
            (
                "title".to_owned(),
                Variant::String("Will I win?".to_owned()),
            ),
            ("outcomes".to_owned(), Variant::String("Yes\nNo".to_owned())),
            ("prediction_window_seconds".to_owned(), Variant::Int(120)),
        ])
    }

    async fn body_for(config: SubActionConfig) -> serde_json::Value {
        let (transport, runner) = runner_with(Ok(prediction_payload()));
        let stack = ArgStack::new();
        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        transport.request(0).body.unwrap()
    }

    #[tokio::test]
    async fn execute_posts_to_predictions_with_broadcaster_in_query_and_pushes_prediction_id() {
        let (transport, runner) = runner_with(Ok(prediction_payload()));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/predictions");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.unwrap().get("broadcaster_id").is_none());
        assert_eq!(
            output.unwrap().get("prediction.id"),
            Some(&Variant::String("pred-xyz".to_owned()))
        );
    }

    #[tokio::test]
    async fn outcomes_are_title_objects_not_bare_strings() {
        let body = body_for(full_cfg()).await;
        assert_eq!(
            body.get("outcomes"),
            Some(&serde_json::json!([{ "title": "Yes" }, { "title": "No" }]))
        );
    }

    #[tokio::test]
    async fn body_uses_prediction_window_key_with_integer_seconds() {
        let mut cfg = full_cfg();
        cfg.insert("prediction_window_seconds".to_owned(), Variant::Int(300));
        let body = body_for(cfg).await;
        assert_eq!(body.get("prediction_window"), Some(&serde_json::json!(300)));
        assert!(body.get("prediction_window_seconds").is_none());
    }

    #[tokio::test]
    async fn title_and_outcomes_are_interpolated_from_arg_stack() {
        let (transport, runner) = runner_with(Ok(prediction_payload()));
        let stack = ArgStack::new()
            .set("q".to_owned(), Variant::String("Clutch?".to_owned()))
            .set("a".to_owned(), Variant::String("Win".to_owned()));
        let mut cfg = full_cfg();
        cfg.insert("title".to_owned(), Variant::String("%q%".to_owned()));
        cfg.insert(
            "outcomes".to_owned(),
            Variant::String("%a%\nLose".to_owned()),
        );

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = transport.request(0).body.unwrap();
        assert_eq!(body.get("title"), Some(&serde_json::json!("Clutch?")));
        assert_eq!(
            body.get("outcomes"),
            Some(&serde_json::json!([{ "title": "Win" }, { "title": "Lose" }]))
        );
    }

    #[tokio::test]
    async fn empty_data_array_yields_failed_and_no_output() {
        let (_, runner) = runner_with(Ok(serde_json::json!({ "data": [] })));
        let stack = ArgStack::new();
        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
    }

    #[tokio::test]
    async fn missing_identity_fails_without_calling_helix() {
        let transport = Arc::new(MockTransport::returning(Ok(prediction_payload())));
        let runner = StartPredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::empty()))),
        );
        let stack = ArgStack::new();
        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn http_failure_outcome_does_not_leak_token() {
        let (_, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid outcomes".to_owned(),
        }));
        let stack = ArgStack::new();
        let (telemetry, _) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;
        assert_request_refused(&telemetry.outcome, 400);
    }

    #[test]
    fn validate_config_enforces_field_constraints() {
        let runner = StartPredictionRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );

        let valid = full_cfg();
        let cfg_with = |key: &str, value: Variant| {
            let mut c = valid.clone();
            c.insert(key.to_owned(), value);
            c
        };

        let cases = [
            ("baseline valid", valid.clone(), true),
            (
                "empty title",
                cfg_with("title", Variant::String(String::new())),
                false,
            ),
            (
                "title over 45 chars",
                cfg_with("title", Variant::String("x".repeat(46))),
                false,
            ),
            (
                "title at 45 chars",
                cfg_with("title", Variant::String("x".repeat(45))),
                true,
            ),
            (
                "single outcome",
                cfg_with("outcomes", Variant::String("Only".to_owned())),
                false,
            ),
            (
                "eleven outcomes",
                cfg_with(
                    "outcomes",
                    Variant::String("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk".to_owned()),
                ),
                false,
            ),
            (
                "outcome over 25 chars",
                cfg_with(
                    "outcomes",
                    Variant::String(format!("{}\nNo", "y".repeat(26))),
                ),
                false,
            ),
            (
                "window below minimum",
                cfg_with("prediction_window_seconds", Variant::Int(29)),
                false,
            ),
            (
                "window at minimum",
                cfg_with("prediction_window_seconds", Variant::Int(30)),
                true,
            ),
            (
                "window above maximum",
                cfg_with("prediction_window_seconds", Variant::Int(1801)),
                false,
            ),
            (
                "window at maximum",
                cfg_with("prediction_window_seconds", Variant::Int(1800)),
                true,
            ),
        ];

        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
