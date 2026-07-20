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

const KIND_ID: &str = "twitch.prediction.start";
const MAX_TITLE_CHARS: usize = 45;
const MIN_OUTCOMES: usize = 2;
const MAX_OUTCOMES: usize = 10;
const MAX_OUTCOME_TITLE_CHARS: usize = 25;
const MIN_WINDOW_SECS: i64 = 30;
const MAX_WINDOW_SECS: i64 = 1800;

pub struct StartPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl StartPredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn start(&self, cfg: &ResolvedConfig) -> Result<String, SubActionOutcome> {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return Err(SubActionOutcome::Failed(e.to_string())),
        };

        // Outcomes must be objects with a "title" key - Twitch rejects a flat string array.
        let outcomes: Vec<serde_json::Value> = cfg
            .outcomes
            .iter()
            .map(|o| serde_json::json!({ "title": o }))
            .collect();

        let mut body = serde_json::Map::new();
        body.insert("title".to_owned(), cfg.title.clone().into());
        body.insert("outcomes".to_owned(), outcomes.into());
        // Twitch's body key is `prediction_window` (not `prediction_window_seconds`).
        body.insert(
            "prediction_window".to_owned(),
            cfg.prediction_window_seconds.into(),
        );

        // POST /helix/predictions; broadcaster_id is a query param, not in the body.
        // Returns 200 with { "data": [{ "id", ... }] }.
        // Requires channel:manage:predictions scope.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/predictions")
            .query("broadcaster_id", user_id)
            .body(serde_json::Value::Object(body));

        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| SubActionOutcome::Failed(e.to_string()))?;

        resp["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["id"].as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| {
                SubActionOutcome::Failed("empty response from start_prediction".to_owned())
            })
    }
}

struct ResolvedConfig {
    title: String,
    outcomes: Vec<String>,
    prediction_window_seconds: i64,
}

/// Splits a textarea value on newlines and commas, trims each entry, drops blanks.
/// Returns Err if count is outside [2, 10] or any outcome title exceeds 25 chars.
fn parse_outcomes(raw: &str) -> Result<Vec<String>, String> {
    let outcomes: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if outcomes.len() < MIN_OUTCOMES {
        return Err(format!("at least {MIN_OUTCOMES} outcomes required"));
    }
    if outcomes.len() > MAX_OUTCOMES {
        return Err(format!(
            "too many outcomes: max {MAX_OUTCOMES}, got {}",
            outcomes.len()
        ));
    }
    for outcome in &outcomes {
        if outcome.chars().count() > MAX_OUTCOME_TITLE_CHARS {
            return Err(format!(
                "outcome '{outcome}' exceeds {MAX_OUTCOME_TITLE_CHARS}-character limit"
            ));
        }
    }
    Ok(outcomes)
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("outcomes".to_owned(), Variant::String(String::new())),
            ("prediction_window_seconds".to_owned(), Variant::Int(120)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Question",
                placeholder: "Will I win this game?",
            },
            FormField::TextArea {
                key: "outcomes",
                label: "Outcomes (one per line or comma-separated, 2-10)",
            },
            FormField::Integer {
                key: "prediction_window_seconds",
                label: "Prediction Window (seconds, 30-1800)",
                min: MIN_WINDOW_SECS,
                max: MAX_WINDOW_SECS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title = match config.get("title") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        if title.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'title' is required"
            )));
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'title' must be \u{2264}{MAX_TITLE_CHARS} characters"
            )));
        }

        let raw_outcomes = match config.get("outcomes") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        parse_outcomes(raw_outcomes)
            .map_err(|msg| RegistryError::UnknownKindId(format!("{KIND_ID}: {msg}")))?;

        match config.get("prediction_window_seconds") {
            Some(Variant::Int(n)) if *n >= MIN_WINDOW_SECS && *n <= MAX_WINDOW_SECS => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'prediction_window_seconds' must be {MIN_WINDOW_SECS}..={MAX_WINDOW_SECS}"
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

        let title_template = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let title = ctx.arg_stack.interpolate(title_template);

        if title.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("title is required".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }
        if title.chars().count() > MAX_TITLE_CHARS {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(format!(
                        "title exceeds {MAX_TITLE_CHARS}-character limit"
                    )),
                    index: ctx.index,
                },
                None,
            );
        }

        let outcomes_template = config
            .get("outcomes")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_outcomes = ctx.arg_stack.interpolate(outcomes_template);

        let outcomes = match parse_outcomes(&raw_outcomes) {
            Ok(v) => v,
            Err(msg) => {
                return (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Failed(msg),
                        index: ctx.index,
                    },
                    None,
                );
            }
        };

        let prediction_window_seconds = config
            .get("prediction_window_seconds")
            .and_then(|v| v.as_int())
            .unwrap_or(120)
            .clamp(MIN_WINDOW_SECS, MAX_WINDOW_SECS);

        let resolved = ResolvedConfig {
            title,
            outcomes,
            prediction_window_seconds,
        };

        match self.start(&resolved).await {
            Ok(prediction_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("prediction.id".to_owned(), Variant::String(prediction_id));
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            Err(outcome) => (
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
            ),
        }
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

    /// Full valid config; tests override single keys to isolate one branch.
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

    /// Executes (optionally mutated) config and returns the posted body,
    /// asserting the call succeeded and reached Helix.
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
        // broadcaster_id is the caller's own id and lives in the QUERY, not the body.
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.unwrap().get("broadcaster_id").is_none());
        // Success output exposes prediction.id for chaining (e.g. End Prediction).
        assert_eq!(
            output.unwrap().get("prediction.id"),
            Some(&Variant::String("pred-xyz".to_owned()))
        );
    }

    // CRITICAL: Twitch's Create Prediction endpoint requires `outcomes` as an array
    // of objects `[{"title": "Yes"}]`. A flat string array `["Yes"]` returns HTTP 400.
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
        // Twitch's body key is `prediction_window` (not `prediction_window_seconds`).
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

    // The token sits in the same creds bundle the runner reads; a transport
    // failure must surface a typed error whose message never leaks it.
    #[tokio::test]
    async fn http_failure_outcome_does_not_leak_token() {
        let (_, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid outcomes".to_owned(),
        }));
        let stack = ArgStack::new();
        let (telemetry, _) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            unreachable!("expected Failed outcome on HTTP error");
        };
        assert!(!msg.contains(TOKEN_SENTINEL));
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

        // (label, config, expect_ok)
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
