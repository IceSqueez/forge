use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigProblem, ConfigSpec, IntegerEditor, IntegerField, OptionalField, RunContext,
    StepTimer, SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{ModeToggle, mode_toggle};
use super::guest_star::{SESSION_ID, SLOT_ID, first_missing};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::GUEST_STAR_UPDATE_SLOT;
const VOLUME_MIN: i64 = 0;
const VOLUME_MAX: i64 = 100;

const AUDIO_ENABLED: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::AUDIO_ENABLED, "Audio Enabled");
const VIDEO_ENABLED: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::VIDEO_ENABLED, "Video Enabled");
const VOLUME: OptionalField<IntegerField> = OptionalField::shared(IntegerField {
    key: config_keys::VOLUME,
    label: "Volume (0-100)",
    editor: IntegerEditor::Number,
    default: VOLUME_MIN,
    bounds: ValueBounds {
        min: Some(VOLUME_MIN),
        max: Some(VOLUME_MAX),
    },
});

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &SESSION_ID,
    &SLOT_ID,
    &AUDIO_ENABLED,
    &VIDEO_ENABLED,
    &VOLUME,
]);

struct SlotSettings {
    session_id: String,
    slot_id: String,
    audio_enabled: ModeToggle,
    video_enabled: ModeToggle,
    volume: Option<i64>,
}

impl SlotSettings {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            session_id: SESSION_ID.read(config, args)?,
            slot_id: SLOT_ID.read(config, args)?,
            audio_enabled: AUDIO_ENABLED.read(config, args)?,
            video_enabled: VIDEO_ENABLED.read(config, args)?,
            volume: VOLUME.read(config, args)?,
        })
    }

    fn body(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut body = serde_json::Map::new();
        if let Some(on) = self.audio_enabled.applied() {
            body.insert("is_audio_enabled".to_owned(), on.into());
        }
        if let Some(on) = self.video_enabled.applied() {
            body.insert("is_video_enabled".to_owned(), on.into());
        }
        if let Some(volume) = self.volume {
            body.insert("volume".to_owned(), volume.into());
        }
        body
    }
}

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

    async fn apply(&self, settings: &SlotSettings) -> SubActionOutcome {
        if let Some(missing) = first_missing(&[
            (&SESSION_ID, &settings.session_id),
            (&SLOT_ID, &settings.slot_id),
        ]) {
            return SubActionOutcome::Failed(missing.into());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => {
                return step_failure::failed(&e);
            }
        };
        let body = settings.body();
        if body.is_empty() {
            return SubActionOutcome::Success;
        }
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/guest_star/slot_settings")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("session_id", settings.session_id.clone())
            .query("slot_id", settings.slot_id.clone())
            .body(serde_json::Value::Object(body));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => step_failure::failed(&e),
        }
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let settings = match SlotSettings::read(config, ctx.arg_stack) {
            Ok(settings) => settings,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(&settings).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: &str = "on";
    const OFF: &str = "off";
    const UNCHANGED: &str = "unchanged";

    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
                ("volume", Variant::String(String::new())),
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
    async fn blank_volume_is_omitted_and_a_set_volume_is_sent() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let blank = cfg(
            "SESSION-7",
            "2",
            &[("volume", Variant::String(String::new()))],
        );
        let (telemetry, _) = runner.execute(&blank, &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            0,
            "a blank volume contributes no key, so the PATCH is skipped"
        );

        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let typed = cfg(
            "SESSION-7",
            "2",
            &[("volume", Variant::String("33".to_owned()))],
        );
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
            SubActionOutcome::Failed(msg) if msg.code_or_text().contains("400") && !msg.code_or_text().contains(TOKEN_SENTINEL)
        ));
    }
}
