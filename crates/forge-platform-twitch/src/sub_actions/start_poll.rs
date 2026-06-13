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
        SubActionCategory::PollsPredictions
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn poll_payload() -> serde_json::Value {
        serde_json::json!({ "data": [{ "id": "poll-xyz", "title": "ignored" }] })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, StartPollRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = StartPollRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    /// Full valid config; tests override single keys to isolate one branch.
    fn full_cfg() -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String("Best map?".to_owned())),
            (
                "choices".to_owned(),
                Variant::String("Dust\nNuke".to_owned()),
            ),
            ("duration_seconds".to_owned(), Variant::Int(120)),
            (
                "channel_points_voting_enabled".to_owned(),
                Variant::Bool(false),
            ),
            ("channel_points_per_vote".to_owned(), Variant::Int(0)),
        ])
    }

    /// Executes (optionally mutated) config and returns the posted body,
    /// asserting the call succeeded and reached Helix.
    async fn body_for(config: SubActionConfig) -> serde_json::Value {
        let (transport, runner) = runner_with(Ok(poll_payload()));
        let stack = ArgStack::new();
        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        transport.request(0).body.unwrap()
    }

    #[tokio::test]
    async fn execute_posts_to_polls_with_broadcaster_in_query_and_pushes_poll_id() {
        let (transport, runner) = runner_with(Ok(poll_payload()));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/polls");
        // broadcaster_id is the caller's own id and lives in the QUERY, not the body.
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.unwrap().get("broadcaster_id").is_none());
        // Success output exposes poll.id for chaining into End Poll.
        assert_eq!(
            output.unwrap().get("poll.id"),
            Some(&Variant::String("poll-xyz".to_owned()))
        );
    }

    // CRITICAL: Twitch's Create Poll endpoint requires `choices` as an array of
    // objects `[{"title": "A"}]`. A flat string array `["A"]` returns HTTP 400.
    #[tokio::test]
    async fn choices_are_title_objects_not_bare_strings() {
        let body = body_for(full_cfg()).await;
        assert_eq!(
            body.get("choices"),
            Some(&serde_json::json!([{ "title": "Dust" }, { "title": "Nuke" }]))
        );
    }

    // Twitch removed bits voting; the body must never carry those keys.
    #[tokio::test]
    async fn body_never_contains_bits_voting_fields() {
        let mut cfg = full_cfg();
        cfg.insert(
            "channel_points_voting_enabled".to_owned(),
            Variant::Bool(true),
        );
        cfg.insert("channel_points_per_vote".to_owned(), Variant::Int(500));
        let body = body_for(cfg).await;
        assert!(body.get("bits_voting_enabled").is_none(), "body: {body}");
        assert!(body.get("bits_per_vote").is_none(), "body: {body}");
    }

    #[tokio::test]
    async fn body_uses_duration_key_with_integer_seconds() {
        let mut cfg = full_cfg();
        cfg.insert("duration_seconds".to_owned(), Variant::Int(300));
        let body = body_for(cfg).await;
        // Twitch's body key is `duration` (not `duration_seconds`), an integer.
        assert_eq!(body.get("duration"), Some(&serde_json::json!(300)));
        assert!(body.get("duration_seconds").is_none());
    }

    // The channel-points pair: disabled => neither key; enabled+value => both;
    // enabled+zero => flag present, value absent.
    #[tokio::test]
    async fn channel_points_fields_track_enable_flag_and_value() {
        // Disabled: no flag (or false) AND no per-vote value.
        let body = body_for(full_cfg()).await;
        assert_ne!(
            body.get("channel_points_voting_enabled"),
            Some(&serde_json::json!(true)),
            "voting must not be enabled: {body}"
        );
        assert!(
            body.get("channel_points_per_vote").is_none(),
            "per-vote absent when disabled: {body}"
        );

        // Enabled with a positive per-vote: both keys present.
        let mut on = full_cfg();
        on.insert(
            "channel_points_voting_enabled".to_owned(),
            Variant::Bool(true),
        );
        on.insert("channel_points_per_vote".to_owned(), Variant::Int(500));
        let body = body_for(on).await;
        assert_eq!(
            body.get("channel_points_voting_enabled"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            body.get("channel_points_per_vote"),
            Some(&serde_json::json!(500))
        );

        // Enabled with zero per-vote: flag present, value omitted (Twitch default).
        let mut on_zero = full_cfg();
        on_zero.insert(
            "channel_points_voting_enabled".to_owned(),
            Variant::Bool(true),
        );
        on_zero.insert("channel_points_per_vote".to_owned(), Variant::Int(0));
        let body = body_for(on_zero).await;
        assert_eq!(
            body.get("channel_points_voting_enabled"),
            Some(&serde_json::json!(true))
        );
        assert!(
            body.get("channel_points_per_vote").is_none(),
            "per-vote absent when zero: {body}"
        );
    }

    #[tokio::test]
    async fn title_and_choices_are_interpolated_from_arg_stack() {
        let (transport, runner) = runner_with(Ok(poll_payload()));
        let stack = ArgStack::new()
            .set("q".to_owned(), Variant::String("Pick one".to_owned()))
            .set("a".to_owned(), Variant::String("Alpha".to_owned()));
        let mut cfg = full_cfg();
        cfg.insert("title".to_owned(), Variant::String("%q%".to_owned()));
        cfg.insert(
            "choices".to_owned(),
            Variant::String("%a%\nBeta".to_owned()),
        );

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = transport.request(0).body.unwrap();
        assert_eq!(body.get("title"), Some(&serde_json::json!("Pick one")));
        assert_eq!(
            body.get("choices"),
            Some(&serde_json::json!([{ "title": "Alpha" }, { "title": "Beta" }]))
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
        let transport = Arc::new(MockTransport::returning(Ok(poll_payload())));
        let runner = StartPollRunner::new(
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
            body: "invalid choices".to_owned(),
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
        let runner = StartPollRunner::new(
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
                "title over 60 chars",
                cfg_with("title", Variant::String("x".repeat(61))),
                false,
            ),
            (
                "title at 60 chars",
                cfg_with("title", Variant::String("x".repeat(60))),
                true,
            ),
            (
                "single choice",
                cfg_with("choices", Variant::String("Only".to_owned())),
                false,
            ),
            (
                "six choices",
                cfg_with("choices", Variant::String("a\nb\nc\nd\ne\nf".to_owned())),
                false,
            ),
            (
                "choice over 25 chars",
                cfg_with(
                    "choices",
                    Variant::String(format!("{}\nOk", "y".repeat(26))),
                ),
                false,
            ),
            (
                "duration below minimum",
                cfg_with("duration_seconds", Variant::Int(14)),
                false,
            ),
            (
                "duration above maximum",
                cfg_with("duration_seconds", Variant::Int(1801)),
                false,
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
