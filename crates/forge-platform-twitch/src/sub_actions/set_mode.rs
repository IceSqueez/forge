use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigProblem, ConfigSpec, DurationBounds, DurationField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{ModeToggle, mode_toggle};
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_SET_MODE;
const SECONDS_PER_MINUTE: u64 = 60;
const FOLLOWER_MODE_MIN_MINUTES: i64 = 0;
const FOLLOWER_MODE_MAX_MINUTES: i64 = 129_600;
const SLOW_MODE_MIN_WAIT_SECONDS: i64 = 3;
const SLOW_MODE_MAX_WAIT_SECONDS: i64 = 120;

pub(crate) const EMOTE_ONLY: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::EMOTE_ONLY, "Emote-Only Mode");
const FOLLOWER_MODE: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::FOLLOWER_MODE, "Follower-Only Mode");
pub(crate) const FOLLOWER_MODE_MIN_AGE: DurationField = DurationField {
    key: config_keys::FOLLOWER_MODE_MIN_MINUTES,
    label: "Follower Min. Duration",
    default: FOLLOWER_MODE_MIN_MINUTES,
    bounds: DurationBounds::in_minutes(FOLLOWER_MODE_MIN_MINUTES, FOLLOWER_MODE_MAX_MINUTES),
};
const SLOW_MODE: ChoiceField<ModeToggle> = mode_toggle(config_keys::SLOW_MODE, "Slow Mode");
pub(crate) const SLOW_MODE_WAIT: DurationField = DurationField {
    key: config_keys::SLOW_MODE_WAIT_SECONDS,
    label: "Slow Mode Wait",
    default: SLOW_MODE_MIN_WAIT_SECONDS,
    bounds: DurationBounds::in_seconds(SLOW_MODE_MIN_WAIT_SECONDS, SLOW_MODE_MAX_WAIT_SECONDS),
};
const SUBSCRIBER_MODE: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::SUBSCRIBER_MODE, "Subscriber-Only Mode");
const UNIQUE_CHAT_MODE: ChoiceField<ModeToggle> =
    mode_toggle(config_keys::UNIQUE_CHAT_MODE, "Unique Chat Mode");

const CONFIG: ConfigSpec = ConfigSpec::new(&[
    &EMOTE_ONLY,
    &FOLLOWER_MODE,
    &FOLLOWER_MODE_MIN_AGE,
    &SLOW_MODE,
    &SLOW_MODE_WAIT,
    &SUBSCRIBER_MODE,
    &UNIQUE_CHAT_MODE,
]);

struct ChatModes {
    emote_only: ModeToggle,
    follower_mode: ModeToggle,
    follower_mode_min_minutes: u64,
    slow_mode: ModeToggle,
    slow_mode_wait_seconds: u64,
    subscriber_mode: ModeToggle,
    unique_chat_mode: ModeToggle,
}

impl ChatModes {
    fn read(config: &SubActionConfig, args: &ArgStack) -> Result<Self, ConfigProblem> {
        Ok(Self {
            emote_only: EMOTE_ONLY.read(config, args)?,
            follower_mode: FOLLOWER_MODE.read(config, args)?,
            follower_mode_min_minutes: FOLLOWER_MODE_MIN_AGE.read(config, args)?.as_secs()
                / SECONDS_PER_MINUTE,
            slow_mode: SLOW_MODE.read(config, args)?,
            slow_mode_wait_seconds: SLOW_MODE_WAIT.read(config, args)?.as_secs(),
            subscriber_mode: SUBSCRIBER_MODE.read(config, args)?,
            unique_chat_mode: UNIQUE_CHAT_MODE.read(config, args)?,
        })
    }

    fn body(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut body = serde_json::Map::new();
        if let Some(on) = self.emote_only.applied() {
            body.insert("emote_mode".to_owned(), on.into());
        }
        if let Some(on) = self.follower_mode.applied() {
            body.insert("follower_mode".to_owned(), on.into());
            if on {
                body.insert(
                    "follower_mode_duration".to_owned(),
                    self.follower_mode_min_minutes.into(),
                );
            }
        }
        if let Some(on) = self.slow_mode.applied() {
            body.insert("slow_mode".to_owned(), on.into());
            if on {
                body.insert(
                    "slow_mode_wait_time".to_owned(),
                    self.slow_mode_wait_seconds.into(),
                );
            }
        }
        if let Some(on) = self.subscriber_mode.applied() {
            body.insert("subscriber_mode".to_owned(), on.into());
        }
        if let Some(on) = self.unique_chat_mode.applied() {
            body.insert("unique_chat_mode".to_owned(), on.into());
        }
        body
    }
}

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

    async fn apply(&self, modes: &ChatModes) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let body = modes.body();
        if body.is_empty() {
            return SubActionOutcome::Success;
        }
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/chat/settings")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::Value::Object(body));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let modes = match ChatModes::read(config, ctx.arg_stack) {
            Ok(modes) => modes,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(&modes).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, SetModeRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = SetModeRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(pairs: &[(&str, Variant)]) -> SubActionConfig {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn toggle(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    #[tokio::test]
    async fn execute_patches_only_changed_modes_and_skips_all_unchanged() {
        let shipped_defaults = runner_with(Ok(serde_json::Value::Null)).1.default_config();
        let cases: Vec<(&str, SubActionConfig, Option<serde_json::Value>)> = vec![
            (
                "all defaults unchanged skips the call",
                shipped_defaults,
                None,
            ),
            (
                "emote only on",
                cfg(&[("emote_only", toggle(ON))]),
                Some(serde_json::json!({ "emote_mode": true })),
            ),
            (
                "follower mode on carries duration",
                cfg(&[
                    ("follower_mode", toggle(ON)),
                    ("follower_mode_min_minutes", Variant::Int(10)),
                ]),
                Some(serde_json::json!({
                    "follower_mode": true,
                    "follower_mode_duration": 10,
                })),
            ),
            (
                "follower mode off drops duration",
                cfg(&[
                    ("follower_mode", toggle(OFF)),
                    ("follower_mode_min_minutes", Variant::Int(10)),
                ]),
                Some(serde_json::json!({ "follower_mode": false })),
            ),
            (
                "slow mode on carries wait time",
                cfg(&[
                    ("slow_mode", toggle(ON)),
                    ("slow_mode_wait_seconds", Variant::Int(30)),
                ]),
                Some(serde_json::json!({
                    "slow_mode": true,
                    "slow_mode_wait_time": 30,
                })),
            ),
            (
                "mixed toggles keep unchanged keys absent",
                cfg(&[
                    ("emote_only", toggle(OFF)),
                    ("subscriber_mode", toggle(ON)),
                    ("unique_chat_mode", toggle(UNCHANGED)),
                    ("slow_mode_wait_seconds", Variant::Int(30)),
                ]),
                Some(serde_json::json!({
                    "emote_mode": false,
                    "subscriber_mode": true,
                })),
            ),
        ];

        for (label, config, expected) in cases {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&config, &make_ctx(&stack)).await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Success,
                "case: {label}"
            );
            match expected {
                None => {
                    assert_eq!(transport.call_count(), 0, "case: {label} must skip Helix");
                }
                Some(body) => {
                    let request = transport.last_request();
                    assert_eq!(request.method, HelixMethod::Patch, "case: {label}");
                    assert_eq!(request.path, "/helix/chat/settings", "case: {label}");
                    assert!(
                        request
                            .query
                            .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
                        "case: {label}"
                    );
                    assert!(
                        request
                            .query
                            .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
                        "case: {label}"
                    );
                    assert_eq!(request.body.unwrap(), body, "case: {label}");
                }
            }
        }
    }

    #[tokio::test]
    async fn helix_http_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 403,
            body: "moderator scope missing".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&cfg(&[("emote_only", toggle(ON))]), &make_ctx(&stack))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("403") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[test]
    fn validate_config_rejects_bad_toggles_and_out_of_range_durations() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("all defaults", runner.default_config(), true),
            ("empty config", BTreeMap::new(), true),
            (
                "unknown toggle value",
                cfg(&[("emote_only", toggle("enabled"))]),
                false,
            ),
            (
                "non-string toggle",
                cfg(&[("slow_mode", Variant::Bool(true))]),
                false,
            ),
            (
                "follower minutes below range",
                cfg(&[("follower_mode_min_minutes", Variant::Int(-1))]),
                false,
            ),
            (
                "follower minutes at max",
                cfg(&[("follower_mode_min_minutes", Variant::Int(129_600))]),
                true,
            ),
            (
                "follower minutes above max",
                cfg(&[("follower_mode_min_minutes", Variant::Int(129_601))]),
                false,
            ),
            (
                "slow wait below min",
                cfg(&[("slow_mode_wait_seconds", Variant::Int(2))]),
                false,
            ),
            (
                "slow wait at min",
                cfg(&[("slow_mode_wait_seconds", Variant::Int(3))]),
                true,
            ),
            (
                "slow wait at max",
                cfg(&[("slow_mode_wait_seconds", Variant::Int(120))]),
                true,
            ),
            (
                "slow wait above max",
                cfg(&[("slow_mode_wait_seconds", Variant::Int(121))]),
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
