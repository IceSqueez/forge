use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigProblem, ConfigSpec, DurationBounds, DurationField, IntegerEditor,
    IntegerField, RunContext, SeparatedListField, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, ToggleField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::POLL_START;
const MAX_TITLE_CHARS: i64 = 60;
const MIN_CHOICES: i64 = 2;
const MAX_CHOICES: i64 = 5;
const MAX_CHOICE_CHARS: i64 = 25;
const DEFAULT_DURATION_SECS: i64 = 60;
const MIN_DURATION_SECS: i64 = 15;
const MAX_DURATION_SECS: i64 = 1800;
const DEFAULT_POINTS_PER_VOTE: i64 = 0;
const MAX_POINTS_PER_VOTE: i64 = 1_000_000;

const TITLE: BoundedTextField = BoundedTextField {
    key: config_keys::TITLE,
    label: "Question",
    editor: TextEditor::Line {
        placeholder: "What should we play next?",
    },
    default: "",
    required: true,
    length: ValueBounds {
        min: None,
        max: Some(MAX_TITLE_CHARS),
    },
};

const CHOICES: SeparatedListField = SeparatedListField {
    key: config_keys::CHOICES,
    label: "Choices (one per line or comma-separated, 2-5)",
    default: "",
    count: ValueBounds {
        min: Some(MIN_CHOICES),
        max: Some(MAX_CHOICES),
    },
    item_length: ValueBounds {
        min: None,
        max: Some(MAX_CHOICE_CHARS),
    },
};

pub(crate) const POLL_DURATION: DurationField = DurationField {
    key: config_keys::DURATION_SECONDS,
    label: "Duration",
    default: DEFAULT_DURATION_SECS,
    bounds: DurationBounds::in_seconds(MIN_DURATION_SECS, MAX_DURATION_SECS),
};

const CHANNEL_POINTS_VOTING: ToggleField = ToggleField {
    key: config_keys::CHANNEL_POINTS_VOTING_ENABLED,
    label: "Enable Channel Points Voting",
    default: false,
};

const CHANNEL_POINTS_PER_VOTE: IntegerField = IntegerField {
    key: config_keys::CHANNEL_POINTS_PER_VOTE,
    label: "Channel Points Per Vote (0 = default; active only when voting enabled)",
    editor: IntegerEditor::Number,
    default: DEFAULT_POINTS_PER_VOTE,
    bounds: ValueBounds {
        min: Some(DEFAULT_POINTS_PER_VOTE),
        max: Some(MAX_POINTS_PER_VOTE),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &TITLE,
    &CHOICES,
    &POLL_DURATION,
    &CHANNEL_POINTS_VOTING,
    &CHANNEL_POINTS_PER_VOTE,
]);

struct ResolvedConfig {
    title: String,
    choices: Vec<String>,
    duration_seconds: u64,
    channel_points_voting_enabled: bool,
    channel_points_per_vote: i64,
}

impl ResolvedConfig {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            title: TITLE.read(config, args)?,
            choices: CHOICES.read(config, args)?,
            duration_seconds: POLL_DURATION.read(config, args)?.as_secs(),
            channel_points_voting_enabled: CHANNEL_POINTS_VOTING.read(config, args)?,
            channel_points_per_vote: CHANNEL_POINTS_PER_VOTE.read(config, args)?,
        })
    }
}

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
        if cfg.title.is_empty() {
            return Err(SubActionOutcome::Failed("title is required".to_owned()));
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return Err(SubActionOutcome::Failed(e.to_string())),
        };
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
            if cfg.channel_points_per_vote > DEFAULT_POINTS_PER_VOTE {
                body.insert(
                    "channel_points_per_vote".to_owned(),
                    cfg.channel_points_per_vote.into(),
                );
            }
        }
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
            Ok(poll_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("poll.id".to_owned(), Variant::String(poll_id));
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
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use std::collections::BTreeMap;

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
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.unwrap().get("broadcaster_id").is_none());
        assert_eq!(
            output.unwrap().get("poll.id"),
            Some(&Variant::String("poll-xyz".to_owned()))
        );
    }

    #[tokio::test]
    async fn choices_are_title_objects_not_bare_strings() {
        let body = body_for(full_cfg()).await;
        assert_eq!(
            body.get("choices"),
            Some(&serde_json::json!([{ "title": "Dust" }, { "title": "Nuke" }]))
        );
    }

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
        assert_eq!(body.get("duration"), Some(&serde_json::json!(300)));
        assert!(body.get("duration_seconds").is_none());
    }

    #[tokio::test]
    async fn channel_points_fields_track_enable_flag_and_value() {
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
            (
                "points per vote zero",
                cfg_with("channel_points_per_vote", Variant::Int(0)),
                true,
            ),
            (
                "points per vote negative",
                cfg_with("channel_points_per_vote", Variant::Int(-1)),
                false,
            ),
            (
                "points per vote at maximum",
                cfg_with("channel_points_per_vote", Variant::Int(1_000_000)),
                true,
            ),
            (
                "points per vote above maximum",
                cfg_with("channel_points_per_vote", Variant::Int(1_000_001)),
                false,
            ),
            (
                "whitespace-only title",
                cfg_with("title", Variant::String("   ".to_owned())),
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
