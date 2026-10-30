use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigProblem, ConfigSpec, DurationBounds, DurationField, IntegerEditor,
    IntegerField, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor,
    ToggleField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText, Variant};

use super::fields::HexColorField;
use crate::account::identity::SelfIdentity;
use crate::custom_rewards::{
    MAX_PROMPT_CHARS, MAX_TITLE_CHARS, MIN_COST, RewardBody, create_request,
};
use crate::helix::HelixTransport;
use crate::helix_envelope::first_row;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_CREATE_REWARD;
const UNLIMITED: i64 = 0;

const TITLE: BoundedTextField = BoundedTextField {
    key: config_keys::TITLE,
    label: "Title",
    editor: TextEditor::Line {
        placeholder: "My Reward",
    },
    default: "",
    required: true,
    length: ValueBounds {
        min: None,
        max: Some(MAX_TITLE_CHARS as i64),
    },
};

const COST: IntegerField = IntegerField {
    key: config_keys::COST,
    label: "Cost (points)",
    editor: IntegerEditor::Number,
    default: MIN_COST,
    bounds: ValueBounds {
        min: Some(MIN_COST),
        max: None,
    },
};

const PROMPT: BoundedTextField = BoundedTextField {
    key: config_keys::PROMPT,
    label: "Prompt (optional)",
    editor: TextEditor::Line {
        placeholder: "Enter your message",
    },
    default: "",
    required: false,
    length: ValueBounds {
        min: None,
        max: Some(MAX_PROMPT_CHARS as i64),
    },
};

const IS_ENABLED: ToggleField = ToggleField {
    key: config_keys::IS_ENABLED,
    label: "Enabled",
    default: true,
};

const REQUIRES_USER_INPUT: ToggleField = ToggleField {
    key: config_keys::REQUIRES_USER_INPUT,
    label: "Require User Input",
    default: false,
};

const SKIP_REQUEST_QUEUE: ToggleField = ToggleField {
    key: config_keys::SKIP_REQUEST_QUEUE,
    label: "Skip Redemption Queue",
    default: false,
};

const MAX_PER_STREAM: IntegerField = IntegerField {
    key: config_keys::MAX_PER_STREAM,
    label: "Max Per Stream (0 = unlimited)",
    editor: IntegerEditor::Number,
    default: UNLIMITED,
    bounds: ValueBounds {
        min: Some(UNLIMITED),
        max: None,
    },
};

const MAX_PER_USER_PER_STREAM: IntegerField = IntegerField {
    key: config_keys::MAX_PER_USER_PER_STREAM,
    label: "Max Per User Per Stream (0 = unlimited)",
    ..MAX_PER_STREAM
};

const GLOBAL_COOLDOWN: DurationField = DurationField {
    key: config_keys::GLOBAL_COOLDOWN_SECONDS,
    label: "Global Cooldown (0 = disabled)",
    default: UNLIMITED,
    bounds: DurationBounds::in_seconds(UNLIMITED, i64::MAX),
};

const BACKGROUND_COLOR: HexColorField = HexColorField {
    key: config_keys::BACKGROUND_COLOR_HEX,
    label: "Background Color (#RRGGBB or empty)",
    placeholder: "#9147FF",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &TITLE,
    &COST,
    &PROMPT,
    &IS_ENABLED,
    &REQUIRES_USER_INPUT,
    &SKIP_REQUEST_QUEUE,
    &MAX_PER_STREAM,
    &MAX_PER_USER_PER_STREAM,
    &GLOBAL_COOLDOWN,
    &BACKGROUND_COLOR,
]);

struct ResolvedConfig {
    title: String,
    cost: i64,
    prompt: String,
    is_enabled: bool,
    is_user_input_required: bool,
    should_redemptions_skip_request_queue: bool,
    max_per_stream: i64,
    max_per_user_per_stream: i64,
    global_cooldown_seconds: u64,
    background_color_hex: String,
}

impl ResolvedConfig {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            title: TITLE.read(config, args)?,
            cost: COST.read(config, args)?,
            prompt: PROMPT.read(config, args)?,
            is_enabled: IS_ENABLED.read(config, args)?,
            is_user_input_required: REQUIRES_USER_INPUT.read(config, args)?,
            should_redemptions_skip_request_queue: SKIP_REQUEST_QUEUE.read(config, args)?,
            max_per_stream: MAX_PER_STREAM.read(config, args)?,
            max_per_user_per_stream: MAX_PER_USER_PER_STREAM.read(config, args)?,
            global_cooldown_seconds: GLOBAL_COOLDOWN.read(config, args)?.as_secs(),
            background_color_hex: BACKGROUND_COLOR.read(config, args)?,
        })
    }

    fn body(&self) -> RewardBody {
        let mut body = RewardBody::new()
            .title(self.title.clone())
            .cost(self.cost)
            .enabled(self.is_enabled)
            .user_input_required(self.is_user_input_required)
            .skip_request_queue(self.should_redemptions_skip_request_queue);
        if self.is_user_input_required && !self.prompt.is_empty() {
            body = body.prompt(self.prompt.clone());
        }
        if !self.background_color_hex.is_empty() {
            body = body.background_color(self.background_color_hex.clone());
        }
        body.max_per_stream(self.max_per_stream)
            .max_per_user_per_stream(self.max_per_user_per_stream)
            .global_cooldown_seconds(self.global_cooldown_seconds.cast_signed())
    }
}

pub struct CreateRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CreateRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn create(&self, cfg: &ResolvedConfig) -> Result<String, SubActionOutcome> {
        if cfg.title.is_empty() {
            return Err(SubActionOutcome::Failed(UserText::unlocalized(
                "title is required",
            )));
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => {
                return Err(SubActionOutcome::Failed(UserText::unlocalized(
                    e.to_string(),
                )));
            }
        };
        let request = create_request(user_id, cfg.body());
        let resp = self
            .transport
            .execute(request)
            .await
            .map_err(|e| SubActionOutcome::Failed(UserText::unlocalized(e.to_string())))?;
        first_row(&resp)
            .and_then(|r| r["id"].as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| {
                SubActionOutcome::Failed(UserText::unlocalized("empty response from create_reward"))
            })
    }
}

#[async_trait]
impl SubActionRunner for CreateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Create Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Creates a new custom channel point reward on the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward create redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
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
        match self.create(&resolved).await {
            Ok(reward_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("reward.id".to_owned(), Variant::String(reward_id));
                (timer.success(), Some(output_stack))
            }
            Err(outcome) => (timer.finish(outcome), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use std::collections::BTreeMap;

    fn reward_payload() -> serde_json::Value {
        serde_json::json!({
            "data": [{ "id": "reward-abc", "title": "ignored" }]
        })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, CreateRewardRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = CreateRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn full_cfg() -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String("My Reward".to_owned())),
            ("cost".to_owned(), Variant::Int(500)),
            ("prompt".to_owned(), Variant::String(String::new())),
            ("is_enabled".to_owned(), Variant::Bool(true)),
            ("requires_user_input".to_owned(), Variant::Bool(false)),
            (
                "should_redemptions_skip_request_queue".to_owned(),
                Variant::Bool(false),
            ),
            ("max_per_stream".to_owned(), Variant::Int(0)),
            ("max_per_user_per_stream".to_owned(), Variant::Int(0)),
            ("global_cooldown_seconds".to_owned(), Variant::Int(0)),
            (
                "background_color_hex".to_owned(),
                Variant::String(String::new()),
            ),
        ])
    }

    async fn body_for(config: SubActionConfig) -> serde_json::Value {
        let (transport, runner) = runner_with(Ok(reward_payload()));
        let stack = ArgStack::new();
        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        transport.request(0).body.unwrap()
    }

    #[tokio::test]
    async fn execute_posts_to_custom_rewards_and_pushes_reward_id() {
        let (transport, runner) = runner_with(Ok(reward_payload()));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert_eq!(
            output.unwrap().get("reward.id"),
            Some(&Variant::String("reward-abc".to_owned()))
        );
    }

    #[tokio::test]
    async fn body_never_contains_is_paused() {
        let body = body_for(full_cfg()).await;
        assert!(
            body.get("is_paused").is_none(),
            "is_paused must not be sent on create: {body}"
        );
    }

    #[tokio::test]
    async fn body_uses_twitch_key_names_not_config_names() {
        let mut cfg = full_cfg();
        cfg.insert("requires_user_input".to_owned(), Variant::Bool(true));
        cfg.insert("prompt".to_owned(), Variant::String("ask".to_owned()));
        cfg.insert(
            "background_color_hex".to_owned(),
            Variant::String("#9147FF".to_owned()),
        );
        let body = body_for(cfg).await;

        assert!(body.get("is_user_input_required").is_some());
        assert!(body.get("requires_user_input").is_none());
        assert!(body.get("background_color").is_some());
        assert!(body.get("background_color_hex").is_none());
    }

    #[tokio::test]
    async fn body_carries_title_cost_and_skip_queue_flag() {
        let mut cfg = full_cfg();
        cfg.insert(
            "should_redemptions_skip_request_queue".to_owned(),
            Variant::Bool(true),
        );
        let body = body_for(cfg).await;

        assert_eq!(body.get("title"), Some(&serde_json::json!("My Reward")));
        assert_eq!(body.get("cost"), Some(&serde_json::json!(500)));
        assert_eq!(
            body.get("should_redemptions_skip_request_queue"),
            Some(&serde_json::json!(true))
        );
    }

    #[tokio::test]
    async fn prompt_included_only_when_user_input_required_and_non_empty() {
        for (label, requires, prompt, expect_prompt) in [
            ("required + text", true, "answer me", true),
            ("required + empty", true, "", false),
            ("not required + text", false, "answer me", false),
            ("not required + empty", false, "", false),
        ] {
            let mut cfg = full_cfg();
            cfg.insert("requires_user_input".to_owned(), Variant::Bool(requires));
            cfg.insert("prompt".to_owned(), Variant::String(prompt.to_owned()));
            let body = body_for(cfg).await;

            assert_eq!(
                body.get("prompt") == Some(&serde_json::json!(prompt)),
                expect_prompt,
                "case: {label}"
            );
            if !expect_prompt {
                assert!(body.get("prompt").is_none(), "case: {label}");
            }
        }
    }

    #[tokio::test]
    async fn background_color_included_only_when_non_empty() {
        for (label, hex, expect_key) in [("set", "#9147FF", true), ("empty", "", false)] {
            let mut cfg = full_cfg();
            cfg.insert(
                "background_color_hex".to_owned(),
                Variant::String(hex.to_owned()),
            );
            let body = body_for(cfg).await;

            match expect_key {
                true => assert_eq!(
                    body.get("background_color"),
                    Some(&serde_json::json!(hex)),
                    "case: {label}"
                ),
                false => assert!(body.get("background_color").is_none(), "case: {label}"),
            }
        }
    }

    #[tokio::test]
    async fn paired_limit_flags_track_their_values() {
        for (key, flag_key) in [
            ("max_per_stream", "is_max_per_stream_enabled"),
            (
                "max_per_user_per_stream",
                "is_max_per_user_per_stream_enabled",
            ),
            ("global_cooldown_seconds", "is_global_cooldown_enabled"),
        ] {
            let mut zero = full_cfg();
            zero.insert(key.to_owned(), Variant::Int(0));
            let body = body_for(zero).await;
            assert_eq!(
                body.get(flag_key),
                Some(&serde_json::json!(false)),
                "{key}=0 flag"
            );
            assert!(body.get(key).is_none(), "{key}=0 value must be absent");

            let mut set = full_cfg();
            set.insert(key.to_owned(), Variant::Int(7));
            let body = body_for(set).await;
            assert_eq!(
                body.get(flag_key),
                Some(&serde_json::json!(true)),
                "{key}=7 flag"
            );
            assert_eq!(body.get(key), Some(&serde_json::json!(7)), "{key}=7 value");
        }
    }

    #[tokio::test]
    async fn missing_data_array_maps_to_failed() {
        let (_transport, runner) = runner_with(Ok(serde_json::json!({ "data": [] })));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid background_color".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&full_cfg(), &make_ctx(&stack)).await;

        assert!(output.is_none());
        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("400") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[tokio::test]
    async fn templated_cost_is_read_as_a_number_after_interpolation() {
        let (transport, runner) = runner_with(Ok(reward_payload()));
        let stack = ArgStack::new().set("pts".to_owned(), Variant::String("750".to_owned()));
        let mut cfg = full_cfg();
        cfg.insert("cost".to_owned(), Variant::String("%pts%".to_owned()));

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap()["cost"],
            serde_json::json!(750)
        );
    }

    #[tokio::test]
    async fn title_over_the_limit_after_interpolation_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(reward_payload()));
        let stack = ArgStack::new().set("name".to_owned(), Variant::String("n".repeat(40)));
        let mut cfg = full_cfg();
        cfg.insert(
            "title".to_owned(),
            Variant::String("Prize %name%".to_owned()),
        );

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if msg.contains("title")),
            "{:?}",
            telemetry.outcome
        );
        assert_eq!(transport.call_count(), 0);
    }

    #[test]
    fn validate_config_enforces_field_constraints() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));

        let with = |edits: &[(&str, Variant)]| -> SubActionConfig {
            let mut c = full_cfg();
            for (k, v) in edits {
                c.insert((*k).to_owned(), v.clone());
            }
            c
        };

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid full config", full_cfg(), true),
            (
                "empty title",
                with(&[("title", Variant::String(String::new()))]),
                false,
            ),
            (
                "title at 45 chars",
                with(&[("title", Variant::String("t".repeat(45)))]),
                true,
            ),
            (
                "title over 45 chars",
                with(&[("title", Variant::String("t".repeat(46)))]),
                false,
            ),
            ("cost zero", with(&[("cost", Variant::Int(0))]), false),
            (
                "whitespace-only title",
                with(&[("title", Variant::String("   ".to_owned()))]),
                false,
            ),
            ("cost at minimum", with(&[("cost", Variant::Int(1))]), true),
            (
                "cost as numeric text",
                with(&[("cost", Variant::String("5".to_owned()))]),
                true,
            ),
            (
                "cost as non-numeric text",
                with(&[("cost", Variant::String("five".to_owned()))]),
                false,
            ),
            (
                "cost as a template",
                with(&[("cost", Variant::String("%pts%".to_owned()))]),
                true,
            ),
            (
                "max per stream zero means unlimited",
                with(&[("max_per_stream", Variant::Int(0))]),
                true,
            ),
            (
                "negative max per stream",
                with(&[("max_per_stream", Variant::Int(-1))]),
                false,
            ),
            (
                "negative max per user per stream",
                with(&[("max_per_user_per_stream", Variant::Int(-1))]),
                false,
            ),
            (
                "negative global cooldown",
                with(&[("global_cooldown_seconds", Variant::Int(-1))]),
                false,
            ),
            (
                "prompt at 200 chars",
                with(&[("prompt", Variant::String("p".repeat(200)))]),
                true,
            ),
            (
                "prompt over 200 chars",
                with(&[("prompt", Variant::String("p".repeat(201)))]),
                false,
            ),
            (
                "color empty is allowed",
                with(&[("background_color_hex", Variant::String(String::new()))]),
                true,
            ),
            (
                "color valid hex",
                with(&[(
                    "background_color_hex",
                    Variant::String("#9147FF".to_owned()),
                )]),
                true,
            ),
            (
                "color missing hash",
                with(&[(
                    "background_color_hex",
                    Variant::String("9147FF0".to_owned()),
                )]),
                false,
            ),
            (
                "color too short",
                with(&[("background_color_hex", Variant::String("#9147F".to_owned()))]),
                false,
            ),
            (
                "color non-hex digit",
                with(&[(
                    "background_color_hex",
                    Variant::String("#9147FZ".to_owned()),
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
