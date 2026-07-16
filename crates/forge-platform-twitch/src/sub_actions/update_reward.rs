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

const KIND_ID: &str = "twitch.channel_points.update_reward";
const MAX_TITLE_CHARS: usize = 45;
const MAX_PROMPT_CHARS: usize = 200;
const HEX_COLOR_LEN: usize = 7;

const UNCHANGED: &str = "unchanged";
const ON: &str = "on";
const OFF: &str = "off";
const TOGGLE_OPTIONS: &[&str] = &[UNCHANGED, ON, OFF];

pub struct UpdateRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(
        &self,
        reward_id: &str,
        body: serde_json::Map<String, serde_json::Value>,
    ) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        // PATCH /helix/channel_points/custom_rewards with broadcaster_id + id query params.
        // Requires channel:manage:redemptions scope. Twitch applies only the supplied body
        // keys; absent keys are left as-is.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channel_points/custom_rewards")
            .query("broadcaster_id", user_id)
            .query("id", reward_id.to_owned())
            .body(serde_json::Value::Object(body));

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

// An Optional value-field stores its value under the inner key directly; the paired gate
// Bool lives under the same key when OFF. A present well-typed value means "include in body";
// a Bool/absent value means "skip". The gate is a UI affordance only - ignored on read.
fn read_opt_str(config: &SubActionConfig, key: &str) -> Option<String> {
    match config.get(key) {
        Some(Variant::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn read_opt_int(config: &SubActionConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(n)) => Some(*n),
        _ => None,
    }
}

fn toggle_to_bool(value: Option<&str>) -> Option<bool> {
    match value {
        Some(ON) => Some(true),
        Some(OFF) => Some(false),
        _ => None,
    }
}

fn mode_toggle<'a>(config: &'a SubActionConfig, key: &str) -> Option<&'a str> {
    config.get(key).and_then(|v| v.as_str())
}

fn is_valid_hex_color(s: &str) -> bool {
    if s.len() != HEX_COLOR_LEN {
        return false;
    }
    let mut chars = s.chars();
    chars.next() == Some('#') && chars.all(|c| c.is_ascii_hexdigit())
}

fn build_body(
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut body = serde_json::Map::new();

    if let Some(raw) = read_opt_str(config, "title") {
        body.insert("title".to_owned(), ctx.arg_stack.interpolate(&raw).into());
    }
    if let Some(cost) = read_opt_int(config, "cost") {
        body.insert("cost".to_owned(), cost.into());
    }
    if let Some(raw) = read_opt_str(config, "prompt") {
        body.insert("prompt".to_owned(), ctx.arg_stack.interpolate(&raw).into());
    }
    if let Some(hex) = read_opt_str(config, "background_color_hex")
        && is_valid_hex_color(&hex)
    {
        body.insert("background_color".to_owned(), hex.into());
    }

    // Each limit pairs an enable flag with its value: value>0 => flag true + value;
    // value==0 => flag false, value omitted. Mirrors Twitch's requirement that the value
    // is only honored when the matching is_*_enabled flag is true.
    if let Some(value) = read_opt_int(config, "max_per_stream") {
        if value > 0 {
            body.insert("is_max_per_stream_enabled".to_owned(), true.into());
            body.insert("max_per_stream".to_owned(), value.into());
        } else {
            body.insert("is_max_per_stream_enabled".to_owned(), false.into());
        }
    }
    if let Some(value) = read_opt_int(config, "max_per_user_per_stream") {
        if value > 0 {
            body.insert("is_max_per_user_per_stream_enabled".to_owned(), true.into());
            body.insert("max_per_user_per_stream".to_owned(), value.into());
        } else {
            body.insert(
                "is_max_per_user_per_stream_enabled".to_owned(),
                false.into(),
            );
        }
    }
    if let Some(value) = read_opt_int(config, "global_cooldown_seconds") {
        if value > 0 {
            body.insert("is_global_cooldown_enabled".to_owned(), true.into());
            body.insert("global_cooldown_seconds".to_owned(), value.into());
        } else {
            body.insert("is_global_cooldown_enabled".to_owned(), false.into());
        }
    }

    if let Some(on) = toggle_to_bool(mode_toggle(config, "is_enabled")) {
        body.insert("is_enabled".to_owned(), on.into());
    }
    if let Some(on) = toggle_to_bool(mode_toggle(config, "requires_user_input")) {
        body.insert("is_user_input_required".to_owned(), on.into());
    }
    if let Some(on) = toggle_to_bool(mode_toggle(config, "should_redemptions_skip_request_queue")) {
        body.insert(
            "should_redemptions_skip_request_queue".to_owned(),
            on.into(),
        );
    }
    if let Some(on) = toggle_to_bool(mode_toggle(config, "is_paused")) {
        body.insert("is_paused".to_owned(), on.into());
    }

    body
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "reward_id".to_owned(),
                Variant::String("%reward.id%".to_owned()),
            ),
            (
                "is_enabled".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "requires_user_input".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "should_redemptions_skip_request_queue".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "is_paused".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "reward_id",
                label: "Reward ID",
                placeholder: "%reward.id%",
            },
            FormField::Optional {
                key: "title",
                label: "Title",
                inner: Box::new(FormField::Text {
                    key: "title",
                    label: "Title",
                    placeholder: "My Reward",
                }),
            },
            FormField::Optional {
                key: "cost",
                label: "Cost (points)",
                inner: Box::new(FormField::Integer {
                    key: "cost",
                    label: "Cost (points)",
                    min: 1,
                    max: i64::MAX,
                }),
            },
            FormField::Optional {
                key: "prompt",
                label: "Prompt",
                inner: Box::new(FormField::Text {
                    key: "prompt",
                    label: "Prompt",
                    placeholder: "Enter your message",
                }),
            },
            FormField::Optional {
                key: "background_color_hex",
                label: "Background Color",
                inner: Box::new(FormField::Text {
                    key: "background_color_hex",
                    label: "Background Color (#RRGGBB)",
                    placeholder: "#9147FF",
                }),
            },
            FormField::Optional {
                key: "max_per_stream",
                label: "Max Per Stream",
                inner: Box::new(FormField::Integer {
                    key: "max_per_stream",
                    label: "Max Per Stream (0 = unlimited)",
                    min: 0,
                    max: i64::MAX,
                }),
            },
            FormField::Optional {
                key: "max_per_user_per_stream",
                label: "Max Per User Per Stream",
                inner: Box::new(FormField::Integer {
                    key: "max_per_user_per_stream",
                    label: "Max Per User Per Stream (0 = unlimited)",
                    min: 0,
                    max: i64::MAX,
                }),
            },
            FormField::Optional {
                key: "global_cooldown_seconds",
                label: "Global Cooldown Seconds",
                inner: Box::new(FormField::Integer {
                    key: "global_cooldown_seconds",
                    label: "Global Cooldown Seconds (0 = disabled)",
                    min: 0,
                    max: i64::MAX,
                }),
            },
            FormField::Select {
                key: "is_enabled",
                label: "Enabled",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "requires_user_input",
                label: "Require User Input",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "should_redemptions_skip_request_queue",
                label: "Skip Redemption Queue",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "is_paused",
                label: "Paused",
                options: TOGGLE_OPTIONS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("reward_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'reward_id' is required"
                )));
            }
        }

        if let Some(title) = read_opt_str(config, "title") {
            let count = title.chars().count();
            if !(1..=MAX_TITLE_CHARS).contains(&count) {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'title' must be 1..={MAX_TITLE_CHARS} characters"
                )));
            }
        }

        if let Some(cost) = read_opt_int(config, "cost")
            && cost < 1
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'cost' must be ≥1"
            )));
        }

        if let Some(prompt) = read_opt_str(config, "prompt")
            && prompt.chars().count() > MAX_PROMPT_CHARS
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'prompt' must be ≤{MAX_PROMPT_CHARS} characters"
            )));
        }

        if let Some(hex) = read_opt_str(config, "background_color_hex")
            && !hex.is_empty()
            && !is_valid_hex_color(&hex)
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'background_color_hex' must be empty or a '#RRGGBB' hex color"
            )));
        }

        for key in &[
            "is_enabled",
            "requires_user_input",
            "should_redemptions_skip_request_queue",
            "is_paused",
        ] {
            match config.get(*key) {
                None => {}
                Some(Variant::String(s)) if TOGGLE_OPTIONS.contains(&s.as_str()) => {}
                _ => {
                    return Err(RegistryError::UnknownKindId(format!(
                        "{KIND_ID}: '{key}' must be one of: unchanged, on, off"
                    )));
                }
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

        let reward_id_template = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let reward_id = ctx.arg_stack.interpolate(reward_id_template);

        let outcome = if reward_id.is_empty() {
            SubActionOutcome::Failed("reward_id is required".to_owned())
        } else {
            let body = build_body(config, ctx);
            // Nothing opted-in => empty body. A PATCH with no fields changes nothing on
            // Twitch's side yet still costs a rate-limit token, so short-circuit to Success.
            if body.is_empty() {
                SubActionOutcome::Success
            } else {
                self.apply(&reward_id, body).await
            }
        };

        (
            SubActionTelemetry {
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
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

    /// A reward_id plus exactly one opted-in field. Tests add/override keys to
    /// exercise one mapping branch at a time; the base on its own yields only a
    /// `title` body so the PATCH is never empty.
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

    /// Executes the config against a stubbed-OK transport and returns the JSON
    /// body the runner PATCHed, asserting the call reached Helix.
    async fn body_for(config: SubActionConfig) -> serde_json::Value {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let (telemetry, out) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "update_reward never pushes an ArgStack");
        transport.request(0).body.unwrap()
    }

    // Behavior 1 + 7: a single opted value-field yields exactly that body key,
    // and the query carries BOTH broadcaster_id=self AND the interpolated id.
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

    // Behavior 2: an Optional value-field holding the gate Bool (toggled on but
    // no value entered yet) is OMITTED; the same key as a typed Variant is sent.
    #[tokio::test]
    async fn optional_value_field_gate_bool_is_omitted_typed_value_is_sent() {
        let omitted = body_for(cfg_with(&[("cost", Variant::Bool(true))])).await;
        assert!(
            omitted.get("cost").is_none(),
            "gate-Bool cost must be omitted: {omitted}"
        );

        let sent = body_for(cfg_with(&[("cost", Variant::Int(200))])).await;
        assert_eq!(sent.get("cost"), Some(&serde_json::json!(200)));
    }

    // Behavior 3: tri-state Selects. "on"->true, "off"->false, "unchanged"->omitted,
    // for each of the four, asserting requires_user_input RENAMES to
    // is_user_input_required while the others keep their config key.
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
            // The config-side name must NOT leak when it differs from the body name.
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

    // Behavior 4: each paired limit emits flag false + NO value at 0, flag true +
    // value at >0. Config key == body value key; only the flag key differs.
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

    // Behavior 5: nothing opted-in => empty PATCH body would change nothing on
    // Twitch's side, so the runner short-circuits to Success without a Helix call.
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
            // Optional value-fields present only as gate Bools => omitted.
            ("cost".to_owned(), Variant::Bool(true)),
        ]);

        let (telemetry, out) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none());
        assert_eq!(transport.call_count(), 0, "empty body must skip the PATCH");
    }

    // Behavior 6: an empty reward_id after interpolation fails before any Helix
    // call (no broadcaster targeted, nothing to update).
    #[tokio::test]
    async fn empty_reward_id_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let cfg = BTreeMap::from([
            // Empty template interpolates to empty; runner must bail before Helix.
            ("reward_id".to_owned(), Variant::String(String::new())),
            ("title".to_owned(), Variant::String("Hi".to_owned())),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    // Behavior 7 (title/prompt half): templated value-fields resolve through the
    // ArgStack into the body.
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

    // Behavior 8: a valid #RRGGBB color is renamed to background_color; an invalid
    // or empty value is omitted (build_body silently drops bad hex).
    #[tokio::test]
    async fn background_color_included_only_when_valid_hex() {
        for (label, hex, expected) in [
            ("valid", "#9147FF", Some(serde_json::json!("#9147FF"))),
            ("too short", "#9147F", None),
            ("missing hash", "9147FF0", None),
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
                "reward_id missing",
                BTreeMap::from([("title".to_owned(), Variant::String("Hi".to_owned()))]),
                false,
            ),
            (
                "opted title 0 chars",
                cfg_with(&[("title", Variant::String(String::new()))]),
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

    // Behavior 10: a Helix failure surfaces as Failed and the sentinel token
    // never appears in the outcome message.
    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "invalid id".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, out) = runner.execute(&cfg_with(&[]), &make_ctx(&stack)).await;

        assert!(out.is_none());
        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("400") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
