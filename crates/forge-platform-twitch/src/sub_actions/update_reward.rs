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
// a Bool/absent value means "skip". The gate is a UI affordance only — ignored on read.
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
        SubActionCategory::Twitch
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
