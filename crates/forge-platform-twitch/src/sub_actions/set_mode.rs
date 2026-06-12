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

const KIND_ID: &str = "twitch.chat.set_mode";

const UNCHANGED: &str = "unchanged";
const ON: &str = "on";
const OFF: &str = "off";
const TOGGLE_OPTIONS: &[&str] = &[UNCHANGED, ON, OFF];

const FOLLOWER_MODE_MIN_MINUTES: i64 = 0;
const FOLLOWER_MODE_MAX_MINUTES: i64 = 129_600;
const SLOW_MODE_MIN_WAIT_SECONDS: i64 = 3;
const SLOW_MODE_MAX_WAIT_SECONDS: i64 = 120;

pub struct SetModeRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SetModeRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, config: &SubActionConfig) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        let emote_only = mode_toggle(config, "emote_only");
        let follower_mode = mode_toggle(config, "follower_mode");
        let follower_mode_min_minutes = int_field(config, "follower_mode_min_minutes");
        let slow_mode = mode_toggle(config, "slow_mode");
        let slow_mode_wait_seconds = int_field(config, "slow_mode_wait_seconds");
        let subscriber_mode = mode_toggle(config, "subscriber_mode");
        let unique_chat_mode = mode_toggle(config, "unique_chat_mode");

        // Build partial PATCH body: only include fields that differ from unchanged.
        // Twitch applies only the provided keys; omitted keys are left as-is.
        // If all modes are unchanged, skip the network call — PATCH with an empty
        // body is a no-op but wastes a rate-limit token.
        let mut body = serde_json::Map::new();

        if let Some(on) = toggle_to_bool(emote_only) {
            body.insert("emote_mode".to_owned(), on.into());
        }
        if let Some(on) = toggle_to_bool(follower_mode) {
            body.insert("follower_mode".to_owned(), on.into());
            if on {
                let duration = follower_mode_min_minutes
                    .unwrap_or(FOLLOWER_MODE_MIN_MINUTES)
                    .clamp(FOLLOWER_MODE_MIN_MINUTES, FOLLOWER_MODE_MAX_MINUTES);
                body.insert("follower_mode_duration".to_owned(), duration.into());
            }
        }
        if let Some(on) = toggle_to_bool(slow_mode) {
            body.insert("slow_mode".to_owned(), on.into());
            if on {
                let wait = slow_mode_wait_seconds
                    .unwrap_or(SLOW_MODE_MIN_WAIT_SECONDS)
                    .clamp(SLOW_MODE_MIN_WAIT_SECONDS, SLOW_MODE_MAX_WAIT_SECONDS);
                body.insert("slow_mode_wait_time".to_owned(), wait.into());
            }
        }
        if let Some(on) = toggle_to_bool(subscriber_mode) {
            body.insert("subscriber_mode".to_owned(), on.into());
        }
        if let Some(on) = toggle_to_bool(unique_chat_mode) {
            body.insert("unique_chat_mode".to_owned(), on.into());
        }

        if body.is_empty() {
            return SubActionOutcome::Success;
        }

        let request = HelixRequest::new(HelixMethod::Patch, "/helix/chat/settings")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::Value::Object(body));

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

fn mode_toggle<'a>(config: &'a SubActionConfig, key: &str) -> Option<&'a str> {
    config.get(key).and_then(|v| v.as_str())
}

fn int_field(config: &SubActionConfig, key: &str) -> Option<i64> {
    config.get(key).and_then(|v| {
        if let Variant::Int(n) = v {
            Some(*n)
        } else {
            None
        }
    })
}

fn toggle_to_bool(value: Option<&str>) -> Option<bool> {
    match value {
        Some(ON) => Some(true),
        Some(OFF) => Some(false),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for SetModeRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Set Chat Mode"
    }

    fn summary(&self) -> &str {
        "Configures emote-only, follower, slow, subscriber, or unique-chat mode."
    }

    fn search_text(&self) -> &str {
        "twitch chat mode emote follower slow subscriber unique moderation settings"
    }

    fn icon_name(&self) -> &str {
        "adjustments"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "emote_only".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "follower_mode".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "follower_mode_min_minutes".to_owned(),
                Variant::Int(FOLLOWER_MODE_MIN_MINUTES),
            ),
            (
                "slow_mode".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "slow_mode_wait_seconds".to_owned(),
                Variant::Int(SLOW_MODE_MIN_WAIT_SECONDS),
            ),
            (
                "subscriber_mode".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
            (
                "unique_chat_mode".to_owned(),
                Variant::String(UNCHANGED.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "emote_only",
                label: "Emote-Only Mode",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "follower_mode",
                label: "Follower-Only Mode",
                options: TOGGLE_OPTIONS,
            },
            FormField::Integer {
                key: "follower_mode_min_minutes",
                label: "Follower Min. Duration (minutes)",
                min: FOLLOWER_MODE_MIN_MINUTES,
                max: FOLLOWER_MODE_MAX_MINUTES,
            },
            FormField::Select {
                key: "slow_mode",
                label: "Slow Mode",
                options: TOGGLE_OPTIONS,
            },
            FormField::Integer {
                key: "slow_mode_wait_seconds",
                label: "Slow Mode Wait (seconds)",
                min: SLOW_MODE_MIN_WAIT_SECONDS,
                max: SLOW_MODE_MAX_WAIT_SECONDS,
            },
            FormField::Select {
                key: "subscriber_mode",
                label: "Subscriber-Only Mode",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "unique_chat_mode",
                label: "Unique Chat Mode",
                options: TOGGLE_OPTIONS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        for key in &[
            "emote_only",
            "follower_mode",
            "slow_mode",
            "subscriber_mode",
            "unique_chat_mode",
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
        if let Some(Variant::Int(n)) = config.get("follower_mode_min_minutes")
            && (*n < FOLLOWER_MODE_MIN_MINUTES || *n > FOLLOWER_MODE_MAX_MINUTES)
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'follower_mode_min_minutes' must be {FOLLOWER_MODE_MIN_MINUTES}..={FOLLOWER_MODE_MAX_MINUTES}"
            )));
        }
        if let Some(Variant::Int(n)) = config.get("slow_mode_wait_seconds")
            && (*n < SLOW_MODE_MIN_WAIT_SECONDS || *n > SLOW_MODE_MAX_WAIT_SECONDS)
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'slow_mode_wait_seconds' must be {SLOW_MODE_MIN_WAIT_SECONDS}..={SLOW_MODE_MAX_WAIT_SECONDS}"
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

        let outcome = self.apply(config).await;

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
