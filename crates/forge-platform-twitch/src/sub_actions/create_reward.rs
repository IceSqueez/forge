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

const KIND_ID: &str = "twitch.channel_points.create_reward";
const MAX_TITLE_CHARS: usize = 45;
const MAX_PROMPT_CHARS: usize = 200;
// Matches the format Twitch accepts: "#RRGGBB" — exactly 7 chars, '#' + 6 hex digits.
const HEX_COLOR_LEN: usize = 7;

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
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return Err(SubActionOutcome::Failed(e.to_string())),
        };

        let body = build_body(cfg);

        // POST /helix/channel_points/custom_rewards returns 200 with { "data": [{ "id", ... }] }.
        // Requires channel:manage:redemptions scope.
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channel_points/custom_rewards")
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
            .ok_or_else(|| SubActionOutcome::Failed("empty response from create_reward".to_owned()))
    }
}

struct ResolvedConfig {
    title: String,
    cost: i64,
    prompt: String,
    is_enabled: bool,
    is_user_input_required: bool,
    should_redemptions_skip_request_queue: bool,
    max_per_stream: i64,
    max_per_user_per_stream: i64,
    global_cooldown_seconds: i64,
    background_color_hex: String,
}

fn build_body(cfg: &ResolvedConfig) -> serde_json::Map<String, serde_json::Value> {
    let mut body = serde_json::Map::new();

    body.insert("title".to_owned(), cfg.title.clone().into());
    body.insert("cost".to_owned(), cfg.cost.into());
    body.insert("is_enabled".to_owned(), cfg.is_enabled.into());
    body.insert(
        "is_user_input_required".to_owned(),
        cfg.is_user_input_required.into(),
    );
    body.insert(
        "should_redemptions_skip_request_queue".to_owned(),
        cfg.should_redemptions_skip_request_queue.into(),
    );

    if cfg.is_user_input_required && !cfg.prompt.is_empty() {
        body.insert("prompt".to_owned(), cfg.prompt.clone().into());
    }

    if !cfg.background_color_hex.is_empty() {
        body.insert(
            "background_color".to_owned(),
            cfg.background_color_hex.clone().into(),
        );
    }

    // Twitch requires both the flag and the value. value==0 means "disabled" from the user's
    // perspective (maps to enabled=false, value omitted). value>0 means enabled=true + value.
    if cfg.max_per_stream > 0 {
        body.insert("is_max_per_stream_enabled".to_owned(), true.into());
        body.insert("max_per_stream".to_owned(), cfg.max_per_stream.into());
    } else {
        body.insert("is_max_per_stream_enabled".to_owned(), false.into());
    }

    if cfg.max_per_user_per_stream > 0 {
        body.insert("is_max_per_user_per_stream_enabled".to_owned(), true.into());
        body.insert(
            "max_per_user_per_stream".to_owned(),
            cfg.max_per_user_per_stream.into(),
        );
    } else {
        body.insert(
            "is_max_per_user_per_stream_enabled".to_owned(),
            false.into(),
        );
    }

    if cfg.global_cooldown_seconds > 0 {
        body.insert("is_global_cooldown_enabled".to_owned(), true.into());
        body.insert(
            "global_cooldown_seconds".to_owned(),
            cfg.global_cooldown_seconds.into(),
        );
    } else {
        body.insert("is_global_cooldown_enabled".to_owned(), false.into());
    }

    body
}

fn is_valid_hex_color(s: &str) -> bool {
    if s.len() != HEX_COLOR_LEN {
        return false;
    }
    let mut chars = s.chars();
    chars.next() == Some('#') && chars.all(|c| c.is_ascii_hexdigit())
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("cost".to_owned(), Variant::Int(1)),
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

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Title",
                placeholder: "My Reward",
            },
            FormField::Integer {
                key: "cost",
                label: "Cost (points)",
                min: 1,
                max: i64::MAX,
            },
            FormField::Text {
                key: "prompt",
                label: "Prompt (optional)",
                placeholder: "Enter your message",
            },
            FormField::Toggle {
                key: "is_enabled",
                label: "Enabled",
            },
            FormField::Toggle {
                key: "requires_user_input",
                label: "Require User Input",
            },
            FormField::Toggle {
                key: "should_redemptions_skip_request_queue",
                label: "Skip Redemption Queue",
            },
            FormField::Integer {
                key: "max_per_stream",
                label: "Max Per Stream (0 = unlimited)",
                min: 0,
                max: i64::MAX,
            },
            FormField::Integer {
                key: "max_per_user_per_stream",
                label: "Max Per User Per Stream (0 = unlimited)",
                min: 0,
                max: i64::MAX,
            },
            FormField::Integer {
                key: "global_cooldown_seconds",
                label: "Global Cooldown Seconds (0 = disabled)",
                min: 0,
                max: i64::MAX,
            },
            FormField::Text {
                key: "background_color_hex",
                label: "Background Color (#RRGGBB or empty)",
                placeholder: "#9147FF",
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

        match config.get("cost") {
            Some(Variant::Int(n)) if *n >= 1 => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'cost' must be ≥1"
                )));
            }
        }

        if let Some(Variant::String(s)) = config.get("prompt")
            && s.chars().count() > MAX_PROMPT_CHARS
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'prompt' must be ≤{MAX_PROMPT_CHARS} characters"
            )));
        }

        if let Some(Variant::String(s)) = config.get("background_color_hex")
            && !s.is_empty()
            && !is_valid_hex_color(s)
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'background_color_hex' must be empty or a '#RRGGBB' hex color"
            )));
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

        let cost = match config.get("cost") {
            Some(Variant::Int(n)) if *n >= 1 => *n,
            _ => {
                return (
                    SubActionTelemetry {
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Failed("cost must be ≥1".to_owned()),
                        index: ctx.index,
                    },
                    None,
                );
            }
        };

        let prompt_template = config
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let prompt = ctx.arg_stack.interpolate(prompt_template);

        let is_enabled = config
            .get("is_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let is_user_input_required = config
            .get("requires_user_input")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let should_redemptions_skip_request_queue = config
            .get("should_redemptions_skip_request_queue")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_per_stream = config
            .get("max_per_stream")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0);
        let max_per_user_per_stream = config
            .get("max_per_user_per_stream")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0);
        let global_cooldown_seconds = config
            .get("global_cooldown_seconds")
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(0);
        let background_color_hex = config
            .get("background_color_hex")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let resolved = ResolvedConfig {
            title,
            cost,
            prompt,
            is_enabled,
            is_user_input_required,
            should_redemptions_skip_request_queue,
            max_per_stream,
            max_per_user_per_stream,
            global_cooldown_seconds,
            background_color_hex,
        };

        match self.create(&resolved).await {
            Ok(reward_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("reward.id".to_owned(), Variant::String(reward_id));
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

    /// Full valid config, with every key the runner reads from. Individual
    /// tests override specific keys to exercise one mapping branch at a time.
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

    /// Executes `full_cfg` (optionally mutated) and returns the JSON body the
    /// runner posted, asserting the call reached Helix.
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

    // Regression: Twitch's Create Custom Reward endpoint rejects `is_paused`
    // (it was removed from the Create body). The runner must never emit it.
    #[tokio::test]
    async fn body_never_contains_is_paused() {
        let body = body_for(full_cfg()).await;
        assert!(
            body.get("is_paused").is_none(),
            "is_paused must not be sent on create: {body}"
        );
    }

    // The runner renames config keys to Twitch's body keys. Assert the body
    // carries Twitch's names and NOT the config-side names.
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

    // Each of the three "max"-style settings pairs an enable flag with the
    // value: 0 => flag false + value key absent; >0 => flag true + value present.
    #[tokio::test]
    async fn paired_limit_flags_track_their_values() {
        // (config key == body value key for all three; only the flag key differs)
        for (key, flag_key) in [
            ("max_per_stream", "is_max_per_stream_enabled"),
            (
                "max_per_user_per_stream",
                "is_max_per_user_per_stream_enabled",
            ),
            ("global_cooldown_seconds", "is_global_cooldown_enabled"),
        ] {
            // Disabled (0): flag false, value key absent.
            let mut zero = full_cfg();
            zero.insert(key.to_owned(), Variant::Int(0));
            let body = body_for(zero).await;
            assert_eq!(
                body.get(flag_key),
                Some(&serde_json::json!(false)),
                "{key}=0 flag"
            );
            assert!(body.get(key).is_none(), "{key}=0 value must be absent");

            // Enabled (>0): flag true, value present.
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
                "cost non-int",
                with(&[("cost", Variant::String("5".to_owned()))]),
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
