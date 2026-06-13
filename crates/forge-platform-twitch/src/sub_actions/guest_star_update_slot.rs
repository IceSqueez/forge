use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::guest_star::{interpolate, session_id_field, validate_session_id, with_session_id};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.guest_star.update_slot";

// Tri-state options mirroring set_mode.rs and update_reward.rs.
const UNCHANGED: &str = "unchanged";
const ON: &str = "on";
const OFF: &str = "off";
const TOGGLE_OPTIONS: &[&str] = &[UNCHANGED, ON, OFF];

const VOLUME_MIN: i64 = 0;
const VOLUME_MAX: i64 = 100;

pub struct GuestStarUpdateSlotRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl GuestStarUpdateSlotRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(
        &self,
        session_id: &str,
        slot_id: &str,
        config: &SubActionConfig,
    ) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        };

        // Build the JSON body including only the fields the user opted to change.
        // Tri-state selects (audio_enabled, video_enabled) contribute a body key only
        // when != "unchanged". The Optional volume contributes only when a well-typed
        // Int is present; a Bool (gate-off) or absent key means "skip".
        let mut body = serde_json::Map::new();

        if let Some(on) = toggle_to_bool(mode_toggle(config, "audio_enabled")) {
            body.insert("is_audio_enabled".to_owned(), on.into());
        }
        if let Some(on) = toggle_to_bool(mode_toggle(config, "video_enabled")) {
            body.insert("is_video_enabled".to_owned(), on.into());
        }
        if let Some(vol) = read_opt_int(config, "volume") {
            body.insert("volume".to_owned(), vol.into());
        }

        // Nothing opted-in: the PATCH body would be empty, which changes nothing on
        // Twitch's side yet still costs a rate-limit token. Short-circuit to Success.
        if body.is_empty() {
            return SubActionOutcome::Success;
        }

        // Verified against dev.twitch.tv (2026-06-13, BETA): PATCH
        // /helix/guest_star/slot_settings. broadcaster_id, moderator_id, session_id,
        // slot_id are query params; is_audio_enabled, is_video_enabled, volume are
        // body fields. Scope: channel:manage:guest_star.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/guest_star/slot_settings")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("session_id", session_id.to_owned())
            .query("slot_id", slot_id.to_owned())
            .body(serde_json::Value::Object(body));

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("{KIND_ID}: {e}")),
        }
    }
}

fn mode_toggle<'a>(config: &'a SubActionConfig, key: &str) -> Option<&'a str> {
    config.get(key).and_then(|v| v.as_str())
}

fn toggle_to_bool(value: Option<&str>) -> Option<bool> {
    match value {
        Some(ON) => Some(true),
        Some(OFF) => Some(false),
        _ => None,
    }
}

// An Optional value-field stores its value under the inner key directly; the
// paired gate Bool is stored under the same key when the UI toggle is off.
// A present well-typed Int means "include"; Bool or absent means "skip".
fn read_opt_int(config: &SubActionConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(n)) => Some(*n),
        _ => None,
    }
}

#[async_trait]
impl SubActionRunner for GuestStarUpdateSlotRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Guest Star Slot Settings"
    }

    fn summary(&self) -> &str {
        "Updates audio, video, or volume settings for a slot in the active Guest Star session."
    }

    fn search_text(&self) -> &str {
        "twitch guest star slot audio video volume settings update collab session"
    }

    fn icon_name(&self) -> &str {
        "adjustments"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut config = with_session_id(BTreeMap::new());
        config.insert("slot_id".to_owned(), Variant::String(String::new()));
        config.insert(
            "audio_enabled".to_owned(),
            Variant::String(UNCHANGED.to_owned()),
        );
        config.insert(
            "video_enabled".to_owned(),
            Variant::String(UNCHANGED.to_owned()),
        );
        config
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            session_id_field(),
            FormField::Text {
                key: "slot_id",
                label: "Slot ID",
                placeholder: "1",
            },
            FormField::Select {
                key: "audio_enabled",
                label: "Audio Enabled",
                options: TOGGLE_OPTIONS,
            },
            FormField::Select {
                key: "video_enabled",
                label: "Video Enabled",
                options: TOGGLE_OPTIONS,
            },
            FormField::Optional {
                key: "volume",
                label: "Volume (0–100)",
                inner: Box::new(FormField::Integer {
                    key: "volume",
                    label: "Volume (0–100)",
                    min: VOLUME_MIN,
                    max: VOLUME_MAX,
                }),
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_session_id(KIND_ID, config)?;

        match config.get("slot_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'slot_id' is required"
                )));
            }
        }

        for key in &["audio_enabled", "video_enabled"] {
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

        if let Some(vol) = read_opt_int(config, "volume")
            && !(VOLUME_MIN..=VOLUME_MAX).contains(&vol)
        {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'volume' must be {VOLUME_MIN}..={VOLUME_MAX}"
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

        let session_id = interpolate(config, ctx.arg_stack, "session_id");
        let slot_id = interpolate(config, ctx.arg_stack, "slot_id");

        let outcome = if session_id.is_empty() {
            SubActionOutcome::Failed("session_id is required".to_owned())
        } else if slot_id.is_empty() {
            SubActionOutcome::Failed("slot_id is required".to_owned())
        } else {
            self.apply(&session_id, &slot_id, config).await
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
