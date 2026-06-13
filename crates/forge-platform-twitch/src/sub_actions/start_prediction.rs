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

        // Outcomes must be objects with a "title" key — Twitch rejects a flat string array.
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
        SubActionCategory::Twitch
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
                label: "Outcomes (one per line or comma-separated, 2–10)",
            },
            FormField::Integer {
                key: "prediction_window_seconds",
                label: "Prediction Window (seconds, 30–1800)",
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
