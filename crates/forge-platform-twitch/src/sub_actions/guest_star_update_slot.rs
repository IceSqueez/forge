use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::guest_star::{interpolate, session_id_field, validate_session_id, with_session_id};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.guest_star.update_slot";

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

        // An empty body would still cost a rate-limit token for a no-op PATCH.
        if body.is_empty() {
            return SubActionOutcome::Success;
        }

        // Requires channel:manage:guest_star scope.
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
    config.str(key)
}

fn toggle_to_bool(value: Option<&str>) -> Option<bool> {
    match value {
        Some(ON) => Some(true),
        Some(OFF) => Some(false),
        _ => None,
    }
}

// A present well-typed Int means "include"; a Bool gate value or absent key means "skip".
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
                label: "Volume (0-100)",
                inner: Box::new(FormField::Integer {
                    key: "volume",
                    label: "Volume (0-100)",
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
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'slot_id' is required"
                )));
            }
        }

        for key in &["audio_enabled", "video_enabled"] {
            match config.get(*key) {
                None => {}
                Some(Variant::String(s)) if TOGGLE_OPTIONS.contains(&s.as_str()) => {}
                _ => {
                    return Err(RegistryError::InvalidConfig(format!(
                        "{KIND_ID}: '{key}' must be one of: unchanged, on, off"
                    )));
                }
            }
        }

        if let Some(vol) = read_opt_int(config, "volume")
            && !(VOLUME_MIN..=VOLUME_MAX).contains(&vol)
        {
            return Err(RegistryError::InvalidConfig(format!(
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
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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
    ) -> (Arc<MockTransport>, GuestStarUpdateSlotRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = GuestStarUpdateSlotRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(session: &str, slot: &str, edits: &[(&str, Variant)]) -> SubActionConfig {
        let mut c = BTreeMap::from([
            ("session_id".to_owned(), Variant::String(session.to_owned())),
            ("slot_id".to_owned(), Variant::String(slot.to_owned())),
        ]);
        for (k, v) in edits {
            c.insert((*k).to_owned(), v.clone());
        }
        c
    }

    fn select(v: &str) -> Variant {
        Variant::String(v.to_owned())
    }

    #[tokio::test]
    async fn all_fields_opted_patches_full_body_with_self_query_params() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let config = cfg(
            "SESSION-7",
            "2",
            &[
                ("audio_enabled", select(ON)),
                ("video_enabled", select(OFF)),
                ("volume", Variant::Int(50)),
            ],
        );

        let (telemetry, out) = runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none());
        assert_eq!(transport.call_count(), 1);

        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/guest_star/slot_settings");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster must be self: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
            "moderator must be self: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("session_id".to_owned(), "SESSION-7".to_owned())),
            "session_id missing from query: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("slot_id".to_owned(), "2".to_owned())),
            "slot_id missing from query: {:?}",
            request.query
        );
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({
                "is_audio_enabled": true,
                "is_video_enabled": false,
                "volume": 50,
            }),
        );
    }

    #[tokio::test]
    async fn only_audio_opted_yields_single_key_body() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let config = cfg(
            "SESSION-7",
            "2",
            &[
                ("audio_enabled", select(ON)),
                ("video_enabled", select(UNCHANGED)),
                ("volume", Variant::Bool(true)),
            ],
        );

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "is_audio_enabled": true }),
        );
    }

    #[tokio::test]
    async fn no_opted_fields_succeeds_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let config = cfg(
            "SESSION-7",
            "2",
            &[
                ("audio_enabled", select(UNCHANGED)),
                ("video_enabled", select(UNCHANGED)),
            ],
        );

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            0,
            "empty body must short-circuit before Helix"
        );
    }

    #[tokio::test]
    async fn volume_gate_bool_is_omitted_only_int_is_sent() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let gated = cfg("SESSION-7", "2", &[("volume", Variant::Bool(true))]);
        let (telemetry, _) = runner.execute(&gated, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            0,
            "gate-Bool volume contributes no key, so the PATCH is skipped"
        );

        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let typed = cfg("SESSION-7", "2", &[("volume", Variant::Int(33))]);
        let (telemetry, _) = runner.execute(&typed, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "volume": 33 }),
        );
    }

    #[tokio::test]
    async fn empty_session_id_fails_before_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let config = cfg("", "2", &[("audio_enabled", select(ON))]);

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn empty_slot_id_fails_before_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let config = cfg("SESSION-7", "", &[("audio_enabled", select(ON))]);

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[test]
    fn validate_config_enforces_required_fields_tristate_and_volume_range() {
        let (_t, runner) = runner_with(Ok(serde_json::Value::Null));

        assert!(
            runner
                .validate_config(&cfg("%guest_star.session_id%", "1", &[]))
                .is_ok(),
            "session + slot with no edits is valid"
        );
        assert!(
            runner
                .validate_config(&cfg(
                    "s",
                    "1",
                    &[
                        ("audio_enabled", select(ON)),
                        ("video_enabled", select(OFF)),
                        ("volume", Variant::Int(0)),
                    ],
                ))
                .is_ok(),
            "boundary volume 0 with on/off toggles is valid"
        );
        assert!(
            runner
                .validate_config(&cfg("s", "1", &[("volume", Variant::Int(100))]))
                .is_ok(),
            "boundary volume 100 is valid"
        );

        for (label, config) in [
            ("empty session_id", cfg("", "1", &[])),
            ("empty slot_id", cfg("s", "", &[])),
            (
                "tri-state out of set",
                cfg("s", "1", &[("audio_enabled", select("maybe"))]),
            ),
            (
                "volume below range",
                cfg("s", "1", &[("volume", Variant::Int(-1))]),
            ),
            (
                "volume above range",
                cfg("s", "1", &[("volume", Variant::Int(101))]),
            ),
        ] {
            assert!(
                runner.validate_config(&config).is_err(),
                "expected rejection: {label}"
            );
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "bad slot".to_owned(),
        }));
        let stack = ArgStack::new();
        let config = cfg("SESSION-7", "2", &[("audio_enabled", select(ON))]);

        let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("400") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
