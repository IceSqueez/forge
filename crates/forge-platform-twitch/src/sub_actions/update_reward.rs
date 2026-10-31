use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    BoundedTextField, ChoiceField, ConfigProblem, ConfigSpec, DurationBounds, DurationField,
    IntegerEditor, IntegerField, OptionalField, OptionalSwitch, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::enable_reward::REWARD_ID;
use super::fields::{HexColorField, ModeToggle, mode_toggle};
use crate::account::identity::SelfIdentity;
use crate::custom_rewards::{
    MAX_PROMPT_CHARS, MAX_TITLE_CHARS, MIN_COST, RewardBody, update_request,
};
use crate::helix::HelixTransport;
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_UPDATE_REWARD;
const MIN_TITLE_CHARS: i64 = 1;
const UNLIMITED: i64 = 0;

const TITLE: OptionalField<BoundedTextField> = OptionalField::shared(BoundedTextField {
    key: config_keys::TITLE,
    label: "Title",
    editor: TextEditor::Line {
        placeholder: "My Reward",
    },
    default: "",
    required: true,
    length: ValueBounds {
        min: Some(MIN_TITLE_CHARS),
        max: Some(MAX_TITLE_CHARS as i64),
    },
});

const COST: OptionalField<IntegerField> = OptionalField::shared(IntegerField {
    key: config_keys::COST,
    label: "Cost (points)",
    editor: IntegerEditor::Number,
    default: MIN_COST,
    bounds: ValueBounds {
        min: Some(MIN_COST),
        max: None,
    },
});

const PROMPT: OptionalField<BoundedTextField> = OptionalField::shared(BoundedTextField {
    key: config_keys::PROMPT,
    label: "Prompt",
    editor: TextEditor::Line {
        placeholder: "Enter your message",
    },
    default: "",
    required: false,
    length: ValueBounds {
        min: None,
        max: Some(MAX_PROMPT_CHARS as i64),
    },
});

const BACKGROUND_COLOR: OptionalField<HexColorField> = OptionalField {
    field: HexColorField {
        key: config_keys::BACKGROUND_COLOR_HEX,
        label: "Background Color (#RRGGBB)",
        placeholder: "#9147FF",
    },
    switch: OptionalSwitch::SharedKey {
        label: Some("Background Color"),
    },
};

const UNLIMITED_COUNT: IntegerField = IntegerField {
    key: config_keys::MAX_PER_STREAM,
    label: "Max Per Stream (0 = unlimited)",
    editor: IntegerEditor::Number,
    default: UNLIMITED,
    bounds: ValueBounds {
        min: Some(UNLIMITED),
        max: None,
    },
};

const MAX_PER_STREAM: OptionalField<IntegerField> = OptionalField {
    field: UNLIMITED_COUNT,
    switch: OptionalSwitch::SharedKey {
        label: Some("Max Per Stream"),
    },
};

const MAX_PER_USER_PER_STREAM: OptionalField<IntegerField> = OptionalField {
    field: IntegerField {
        key: config_keys::MAX_PER_USER_PER_STREAM,
        label: "Max Per User Per Stream (0 = unlimited)",
        ..UNLIMITED_COUNT
    },
    switch: OptionalSwitch::SharedKey {
        label: Some("Max Per User Per Stream"),
    },
};

const GLOBAL_COOLDOWN: OptionalField<DurationField> = OptionalField {
    field: DurationField {
        key: config_keys::GLOBAL_COOLDOWN_SECONDS,
        label: "Global Cooldown (0 = disabled)",
        default: UNLIMITED,
        bounds: DurationBounds::in_seconds(UNLIMITED, i64::MAX),
    },
    switch: OptionalSwitch::SharedKey {
        label: Some("Global Cooldown Seconds"),
    },
};

const IS_ENABLED: ChoiceField<ModeToggle> = mode_toggle(config_keys::IS_ENABLED, "Enabled");
const REQUIRES_USER_INPUT: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::REQUIRES_USER_INPUT, "Require User Input");
const SKIP_REQUEST_QUEUE: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::SKIP_REQUEST_QUEUE, "Skip Redemption Queue");
const IS_PAUSED: ChoiceField<ModeToggle> = mode_toggle(config_keys::IS_PAUSED, "Paused");

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &REWARD_ID,
    &TITLE,
    &COST,
    &PROMPT,
    &BACKGROUND_COLOR,
    &MAX_PER_STREAM,
    &MAX_PER_USER_PER_STREAM,
    &GLOBAL_COOLDOWN,
    &IS_ENABLED,
    &REQUIRES_USER_INPUT,
    &SKIP_REQUEST_QUEUE,
    &IS_PAUSED,
]);

struct RewardUpdate {
    reward_id: String,
    title: Option<String>,
    cost: Option<i64>,
    prompt: Option<String>,
    background_color_hex: Option<String>,
    max_per_stream: Option<i64>,
    max_per_user_per_stream: Option<i64>,
    global_cooldown_seconds: Option<u64>,
    is_enabled: ModeToggle,
    requires_user_input: ModeToggle,
    skip_request_queue: ModeToggle,
    is_paused: ModeToggle,
}

impl RewardUpdate {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            reward_id: REWARD_ID.read(config, args)?,
            title: TITLE.read(config, args)?,
            cost: COST.read(config, args)?,
            prompt: PROMPT.read(config, args)?,
            background_color_hex: BACKGROUND_COLOR.read(config, args)?,
            max_per_stream: MAX_PER_STREAM.read(config, args)?,
            max_per_user_per_stream: MAX_PER_USER_PER_STREAM.read(config, args)?,
            global_cooldown_seconds: GLOBAL_COOLDOWN
                .read(config, args)?
                .map(|cooldown| cooldown.as_secs()),
            is_enabled: IS_ENABLED.read(config, args)?,
            requires_user_input: REQUIRES_USER_INPUT.read(config, args)?,
            skip_request_queue: SKIP_REQUEST_QUEUE.read(config, args)?,
            is_paused: IS_PAUSED.read(config, args)?,
        })
    }

    fn body(self) -> RewardBody {
        let mut body = RewardBody::new();
        if let Some(title) = self.title {
            body = body.title(title);
        }
        if let Some(cost) = self.cost {
            body = body.cost(cost);
        }
        if let Some(prompt) = self.prompt {
            body = body.prompt(prompt);
        }
        if let Some(hex) = self.background_color_hex {
            body = body.background_color(hex);
        }
        if let Some(value) = self.max_per_stream {
            body = body.max_per_stream(value);
        }
        if let Some(value) = self.max_per_user_per_stream {
            body = body.max_per_user_per_stream(value);
        }
        if let Some(value) = self.global_cooldown_seconds {
            body = body.global_cooldown_seconds(value.cast_signed());
        }
        if let Some(on) = self.is_enabled.applied() {
            body = body.enabled(on);
        }
        if let Some(on) = self.requires_user_input.applied() {
            body = body.user_input_required(on);
        }
        if let Some(on) = self.skip_request_queue.applied() {
            body = body.skip_request_queue(on);
        }
        if let Some(on) = self.is_paused.applied() {
            body = body.paused(on);
        }
        body
    }
}

pub(crate) struct UpdateRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateRewardRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, update: RewardUpdate) -> SubActionOutcome {
        let reward_id = update.reward_id.clone();
        let body = update.body();
        if body.is_empty() {
            return SubActionOutcome::Success;
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = update_request(user_id, &reward_id, body);
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for UpdateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Update Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Updates selected fields of an existing custom channel point reward."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward update edit redemption pause cost"
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
        let update = match RewardUpdate::read(config, ctx.arg_stack) {
            Ok(update) => update,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(update).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, UpdateRewardRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = UpdateRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg_with(edits: &[(&str, Variant)]) -> SubActionConfig {
        let mut c = BTreeMap::from([
            ("reward_id".to_owned(), Variant::String("abc".to_owned())),
            ("title".to_owned(), Variant::String("Hi".to_owned())),
        ]);
        for (k, v) in edits {
            c.insert((*k).to_owned(), v.clone());
        }
        c
    }

    async fn body_for(config: SubActionConfig) -> serde_json::Value {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let (telemetry, out) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "update_reward never pushes an ArgStack");
        transport.request(0).body.unwrap()
    }

    #[tokio::test]
    async fn single_opted_title_patches_with_both_query_params() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("xyz".to_owned()));
        let cfg = BTreeMap::from([
            (
                "reward_id".to_owned(),
                Variant::String("%reward.id%".to_owned()),
            ),
            ("title".to_owned(), Variant::String("New Title".to_owned())),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "missing broadcaster_id: {:?}",
            request.query
        );
        assert!(
            request.query.contains(&("id".to_owned(), "xyz".to_owned())),
            "id must be the interpolated reward_id: {:?}",
            request.query
        );
        let body = request.body.unwrap();
        assert_eq!(
            body.as_object().map(|m| m.len()),
            Some(1),
            "only the opted title should appear: {body}"
        );
        assert_eq!(body.get("title"), Some(&serde_json::json!("New Title")));
    }

    #[tokio::test]
    async fn blank_optional_fields_leave_the_reward_unchanged() {
        let body = body_for(cfg_with(&[
            ("title", Variant::String(String::new())),
            ("prompt", Variant::String("  ".to_owned())),
            ("background_color_hex", Variant::String(String::new())),
            ("max_per_stream", Variant::String(String::new())),
            ("cost", Variant::Int(200)),
        ]))
        .await;

        assert_eq!(body, serde_json::json!({ "cost": 200 }));
    }

    #[tokio::test]
    async fn tristate_selects_map_on_off_unchanged_and_rename_user_input() {
        for (cfg_key, body_key) in [
            ("is_enabled", "is_enabled"),
            ("requires_user_input", "is_user_input_required"),
            (
                "should_redemptions_skip_request_queue",
                "should_redemptions_skip_request_queue",
            ),
            ("is_paused", "is_paused"),
        ] {
            let on = body_for(cfg_with(&[(cfg_key, Variant::String("on".to_owned()))])).await;
            assert_eq!(
                on.get(body_key),
                Some(&serde_json::json!(true)),
                "{cfg_key}=on -> {body_key}:true"
            );
            if cfg_key != body_key {
                assert!(
                    on.get(cfg_key).is_none(),
                    "{cfg_key} must be renamed to {body_key}"
                );
            }

            let off = body_for(cfg_with(&[(cfg_key, Variant::String("off".to_owned()))])).await;
            assert_eq!(
                off.get(body_key),
                Some(&serde_json::json!(false)),
                "{cfg_key}=off -> {body_key}:false"
            );

            let unchanged = body_for(cfg_with(&[(
                cfg_key,
                Variant::String("unchanged".to_owned()),
            )]))
            .await;
            assert!(
                unchanged.get(body_key).is_none(),
                "{cfg_key}=unchanged -> {body_key} omitted"
            );
        }
    }

    #[tokio::test]
    async fn paired_limits_toggle_flag_and_value_together() {
        for (key, flag_key) in [
            ("max_per_stream", "is_max_per_stream_enabled"),
            (
                "max_per_user_per_stream",
                "is_max_per_user_per_stream_enabled",
            ),
            ("global_cooldown_seconds", "is_global_cooldown_enabled"),
        ] {
            let zero = body_for(cfg_with(&[(key, Variant::Int(0))])).await;
            assert_eq!(
                zero.get(flag_key),
                Some(&serde_json::json!(false)),
                "{key}=0 flag false"
            );
            assert!(zero.get(key).is_none(), "{key}=0 value absent");

            let set = body_for(cfg_with(&[(key, Variant::Int(5))])).await;
            assert_eq!(
                set.get(flag_key),
                Some(&serde_json::json!(true)),
                "{key}=5 flag true"
            );
            assert_eq!(set.get(key), Some(&serde_json::json!(5)), "{key}=5 value");
        }
    }

    #[tokio::test]
    async fn all_unchanged_with_no_value_fields_succeeds_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let cfg = BTreeMap::from([
            ("reward_id".to_owned(), Variant::String("abc".to_owned())),
            (
                "is_enabled".to_owned(),
                Variant::String("unchanged".to_owned()),
            ),
            (
                "requires_user_input".to_owned(),
                Variant::String("unchanged".to_owned()),
            ),
            (
                "is_paused".to_owned(),
                Variant::String("unchanged".to_owned()),
            ),
        ]);

        let (telemetry, out) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none());
        assert_eq!(transport.call_count(), 0, "empty body must skip the PATCH");
    }

    #[tokio::test]
    async fn empty_reward_id_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let cfg = BTreeMap::from([
            ("reward_id".to_owned(), Variant::String(String::new())),
            ("title".to_owned(), Variant::String("Hi".to_owned())),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn title_and_prompt_interpolate_from_stack() {
        let stack = ArgStack::new()
            .set("who".to_owned(), Variant::String("Nova".to_owned()))
            .set("pts".to_owned(), Variant::String("guess".to_owned()));
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([
            ("reward_id".to_owned(), Variant::String("abc".to_owned())),
            ("title".to_owned(), Variant::String("Hi %who%".to_owned())),
            (
                "prompt".to_owned(),
                Variant::String("Please %pts%".to_owned()),
            ),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = transport.request(0).body.unwrap();
        assert_eq!(body.get("title"), Some(&serde_json::json!("Hi Nova")));
        assert_eq!(body.get("prompt"), Some(&serde_json::json!("Please guess")));
    }

    #[tokio::test]
    async fn background_color_is_sent_when_valid_and_left_unchanged_when_blank() {
        for (label, hex, expected) in [
            ("valid", "#9147FF", Some(serde_json::json!("#9147FF"))),
            ("empty", "", None),
        ] {
            let body = body_for(cfg_with(&[(
                "background_color_hex",
                Variant::String(hex.to_owned()),
            )]))
            .await;
            assert_eq!(
                body.get("background_color").cloned(),
                expected,
                "case: {label}"
            );
            assert!(
                body.get("background_color_hex").is_none(),
                "config key must never appear in body: {label}"
            );
        }
    }

    #[tokio::test]
    async fn malformed_background_color_fails_without_helix_call() {
        for hex in ["#9147F", "9147FF0", "#9147FZ"] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();
            let cfg = cfg_with(&[("background_color_hex", Variant::String(hex.to_owned()))]);

            let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

            assert!(
                matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if crate::sub_actions::test_support::rejected_field(msg) == Some("background_color_hex")),
                "{hex}: {:?}",
                telemetry.outcome
            );
            assert_eq!(transport.call_count(), 0, "{hex}");
        }
    }

    #[test]
    fn validate_config_enforces_field_constraints() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("minimal valid reward_id", cfg_with(&[]), true),
            (
                "reward_id empty",
                BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            (
                "reward_id missing reads the declared template",
                BTreeMap::from([("title".to_owned(), Variant::String("Hi".to_owned()))]),
                true,
            ),
            (
                "empty title leaves it unchanged",
                cfg_with(&[("title", Variant::String(String::new()))]),
                true,
            ),
            (
                "title at one char",
                cfg_with(&[("title", Variant::String("t".to_owned()))]),
                true,
            ),
            (
                "cost as numeric text",
                cfg_with(&[("cost", Variant::String("5".to_owned()))]),
                true,
            ),
            (
                "cost as a template",
                cfg_with(&[("cost", Variant::String("%pts%".to_owned()))]),
                true,
            ),
            (
                "negative max per stream",
                cfg_with(&[("max_per_stream", Variant::Int(-1))]),
                false,
            ),
            (
                "wrong-type reward_id",
                cfg_with(&[("reward_id", Variant::Int(7))]),
                false,
            ),
            (
                "opted title 45 chars",
                cfg_with(&[("title", Variant::String("t".repeat(45)))]),
                true,
            ),
            (
                "opted title 46 chars",
                cfg_with(&[("title", Variant::String("t".repeat(46)))]),
                false,
            ),
            (
                "opted cost 0",
                cfg_with(&[("cost", Variant::Int(0))]),
                false,
            ),
            ("opted cost 1", cfg_with(&[("cost", Variant::Int(1))]), true),
            (
                "opted prompt 200 chars",
                cfg_with(&[("prompt", Variant::String("p".repeat(200)))]),
                true,
            ),
            (
                "opted prompt 201 chars",
                cfg_with(&[("prompt", Variant::String("p".repeat(201)))]),
                false,
            ),
            (
                "bad hex color",
                cfg_with(&[("background_color_hex", Variant::String("#zzz".to_owned()))]),
                false,
            ),
            (
                "empty hex color allowed",
                cfg_with(&[("background_color_hex", Variant::String(String::new()))]),
                true,
            ),
            (
                "tri-state value 'maybe'",
                cfg_with(&[("is_enabled", Variant::String("maybe".to_owned()))]),
                false,
            ),
            (
                "tri-state value 'off'",
                cfg_with(&[("is_paused", Variant::String("off".to_owned()))]),
                true,
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
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid id".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, out) = runner.execute(&cfg_with(&[]), &make_ctx(&stack)).await;

        assert!(out.is_none());
        assert_request_refused(&telemetry.outcome, 400);
    }
}
