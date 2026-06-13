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
        SubActionCategory::Twitch
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
