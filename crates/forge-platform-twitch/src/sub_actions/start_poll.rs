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

const KIND_ID: &str = "twitch.poll.start";
const MAX_TITLE_CHARS: usize = 60;
const MAX_CHOICES: usize = 5;
const MIN_CHOICES: usize = 2;
const MAX_CHOICE_CHARS: usize = 25;
const MIN_DURATION_SECS: i64 = 15;
const MAX_DURATION_SECS: i64 = 1800;

pub struct StartPollRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl StartPollRunner {
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

        // Choices must be objects with a "title" key, not bare strings.
        // Twitch rejects a flat string array with HTTP 400.
        let choices: Vec<serde_json::Value> = cfg
            .choices
            .iter()
            .map(|c| serde_json::json!({ "title": c }))
            .collect();

        let mut body = serde_json::Map::new();
        body.insert("title".to_owned(), cfg.title.clone().into());
        body.insert("choices".to_owned(), choices.into());
        body.insert("duration".to_owned(), cfg.duration_seconds.into());

        if cfg.channel_points_voting_enabled {
            body.insert("channel_points_voting_enabled".to_owned(), true.into());
            // channel_points_per_vote is only meaningful when voting is enabled.
            // Sending it when disabled has no effect, but omitting it keeps the body clean.
            if cfg.channel_points_per_vote > 0 {
                body.insert(
                    "channel_points_per_vote".to_owned(),
                    cfg.channel_points_per_vote.into(),
                );
            }
        }

        // POST /helix/polls; broadcaster_id is a query param, not body.
        // Returns 200 with { "data": [{ "id", ... }] }.
        // Requires channel:manage:polls scope.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/polls")
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
            .ok_or_else(|| SubActionOutcome::Failed("empty response from start_poll".to_owned()))
    }
}

struct ResolvedConfig {
    title: String,
    choices: Vec<String>,
    duration_seconds: i64,
    channel_points_voting_enabled: bool,
    channel_points_per_vote: i64,
}

/// Splits a textarea value on newlines and commas, trims each entry, drops blanks.
/// Returns Err if count is outside [2, 5] or any choice exceeds 25 chars.
fn parse_choices(raw: &str) -> Result<Vec<String>, String> {
    let choices: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    if choices.len() < MIN_CHOICES {
        return Err(format!("at least {MIN_CHOICES} choices required"));
    }
    if choices.len() > MAX_CHOICES {
        return Err(format!(
            "too many choices: max {MAX_CHOICES}, got {}",
            choices.len()
        ));
    }
    for choice in &choices {
        if choice.chars().count() > MAX_CHOICE_CHARS {
            return Err(format!(
                "choice '{choice}' exceeds {MAX_CHOICE_CHARS}-character limit"
            ));
        }
    }
    Ok(choices)
}

#[async_trait]
impl SubActionRunner for StartPollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Start Poll"
    }

    fn summary(&self) -> &str {
        "Creates a new Twitch poll. Outputs poll.id for chaining with End Poll."
    }

    fn search_text(&self) -> &str {
        "twitch poll vote create start channel points"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("choices".to_owned(), Variant::String(String::new())),
            ("duration_seconds".to_owned(), Variant::Int(60)),
            (
                "channel_points_voting_enabled".to_owned(),
                Variant::Bool(false),
            ),
            ("channel_points_per_vote".to_owned(), Variant::Int(0)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Question",
                placeholder: "What should we play next?",
            },
            FormField::TextArea {
                key: "choices",
                label: "Choices (one per line or comma-separated, 2–5)",
            },
            FormField::Integer {
                key: "duration_seconds",
                label: "Duration (seconds, 15–1800)",
                min: MIN_DURATION_SECS,
                max: MAX_DURATION_SECS,
            },
            FormField::Toggle {
                key: "channel_points_voting_enabled",
                label: "Enable Channel Points Voting",
            },
            FormField::Integer {
                key: "channel_points_per_vote",
                label: "Channel Points Per Vote (0 = default; active only when voting enabled)",
                min: 0,
                max: 1_000_000,
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
                "{KIND_ID}: 'title' must be ≤{MAX_TITLE_CHARS} characters"
            )));
        }

        let raw_choices = match config.get("choices") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        parse_choices(raw_choices)
            .map_err(|msg| RegistryError::UnknownKindId(format!("{KIND_ID}: {msg}")))?;

        match config.get("duration_seconds") {
            Some(Variant::Int(n)) if *n >= MIN_DURATION_SECS && *n <= MAX_DURATION_SECS => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'duration_seconds' must be {MIN_DURATION_SECS}..={MAX_DURATION_SECS}"
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

        let choices_template = config
            .get("choices")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_choices = ctx.arg_stack.interpolate(choices_template);

        let choices = match parse_choices(&raw_choices) {
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

        let duration_seconds = config
            .get("duration_seconds")
            .and_then(|v| v.as_int())
            .unwrap_or(60)
            .clamp(MIN_DURATION_SECS, MAX_DURATION_SECS);

        let channel_points_voting_enabled = config
            .get("channel_points_voting_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let channel_points_per_vote = config
            .get("channel_points_per_vote")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0);

        let resolved = ResolvedConfig {
            title,
            choices,
            duration_seconds,
            channel_points_voting_enabled,
            channel_points_per_vote,
        };

        match self.start(&resolved).await {
            Ok(poll_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("poll.id".to_owned(), Variant::String(poll_id));
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
