use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, PickedTextField, RunContext, StepCatalogEntry, StepTimer, SubActionCategory,
    SubActionRunner, TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::collection_keys::MANAGEABLE_REWARD_OPTIONS_KEY;
use crate::custom_rewards::{RewardBody, update_request};
use crate::helix::HelixTransport;
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

pub(crate) const REWARD_ID: PickedTextField = PickedTextField {
    key: config_keys::REWARD_ID,
    label: "Reward ID",
    picker: TextPicker::Options {
        options_key: MANAGEABLE_REWARD_OPTIONS_KEY,
    },
    default: "%reward.id%",
    required: true,
};

pub(crate) const REWARD_CONFIG: ConfigSpec = ConfigSpec::new(&[&REWARD_ID]);

#[derive(Clone, Copy)]
struct RewardToggleSpec {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon: &'static str,
    body: fn() -> RewardBody,
}

const ENABLE: RewardToggleSpec = RewardToggleSpec {
    id: sub_action_ids::CHANNEL_POINTS_ENABLE_REWARD,
    label: "Enable Channel Point Reward",
    summary: "Enables a custom channel point reward so viewers can redeem it.",
    search_text: "twitch channel points custom reward enable on redemption",
    icon: "star",
    body: || RewardBody::new().enabled(true),
};

const DISABLE: RewardToggleSpec = RewardToggleSpec {
    id: sub_action_ids::CHANNEL_POINTS_DISABLE_REWARD,
    label: "Disable Channel Point Reward",
    summary: "Disables a custom channel point reward so viewers cannot redeem it.",
    search_text: "twitch channel points custom reward disable off redemption",
    icon: "star",
    body: || RewardBody::new().enabled(false),
};

const PAUSE: RewardToggleSpec = RewardToggleSpec {
    id: sub_action_ids::CHANNEL_POINTS_PAUSE_REWARD,
    label: "Pause Channel Point Reward",
    summary: "Pauses a custom channel point reward, queueing new redemptions without fulfilling them.",
    search_text: "twitch channel points custom reward pause hold queue redemption",
    icon: "star",
    body: || RewardBody::new().paused(true),
};

const RESUME: RewardToggleSpec = RewardToggleSpec {
    id: sub_action_ids::CHANNEL_POINTS_RESUME_REWARD,
    label: "Resume Channel Point Reward",
    summary: "Resumes a paused custom channel point reward so redemptions are fulfilled normally.",
    search_text: "twitch channel points custom reward resume unpause redemption",
    icon: "star",
    body: || RewardBody::new().paused(false),
};

pub(crate) struct RewardToggleRunner {
    spec: RewardToggleSpec,
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl RewardToggleRunner {
    fn new(
        spec: RewardToggleSpec,
        transport: Arc<dyn HelixTransport>,
        identity: Arc<SelfIdentity>,
    ) -> Self {
        Self {
            spec,
            transport,
            identity,
        }
    }

    pub(crate) fn enable(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self::new(ENABLE, transport, identity)
    }

    pub(crate) fn disable(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self::new(DISABLE, transport, identity)
    }

    pub(crate) fn pause(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self::new(PAUSE, transport, identity)
    }

    pub(crate) fn resume(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self::new(RESUME, transport, identity)
    }
}

async fn patch_reward_bool(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    reward_id: &str,
    body: RewardBody,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return step_failure::failed(&e),
    };
    let request = update_request(user_id, reward_id, body);
    step_failure::outcome(&transport.execute(request).await)
}

async fn execute_bool_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    body: RewardBody,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let timer = StepTimer::start(ctx, kind_id);
    let reward_id = match REWARD_ID.read(config, ctx.arg_stack) {
        Ok(reward_id) => reward_id,
        Err(problem) => return (timer.invalid_config(&problem), None),
    };
    let outcome = patch_reward_bool(transport, identity, &reward_id, body).await;
    (timer.finish(outcome), None)
}

impl StepCatalogEntry for RewardToggleRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }
    fn label(&self) -> &str {
        self.spec.label
    }
    fn summary(&self) -> &str {
        self.spec.summary
    }
    fn search_text(&self) -> &str {
        self.spec.search_text
    }
    fn icon_name(&self) -> &str {
        self.spec.icon
    }
}

#[async_trait]
impl SubActionRunner for RewardToggleRunner {
    fn id(&self) -> &str {
        self.spec.id
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(REWARD_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_bool_runner(
            &self.transport,
            &self.identity,
            self.spec.id,
            (self.spec.body)(),
            config,
            ctx,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TwitchError;
    use crate::helix::HelixMethod;
    use crate::sub_actions::test_support::{
        MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn enable_runner_with(
        response: Result<serde_json::Value, TwitchError>,
    ) -> (Arc<MockTransport>, RewardToggleRunner) {
        crate::sub_actions::test_support::runner_with(vec![response], RewardToggleRunner::enable)
    }

    #[tokio::test]
    async fn enable_patches_is_enabled_true_with_both_query_params() {
        let (transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw9".to_owned()));
        let cfg = REWARD_CONFIG.default_config();

        let (telemetry, out) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "reward toggles never push an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "missing broadcaster_id=self: {:?}",
            request.query
        );
        assert!(
            request.query.contains(&("id".to_owned(), "rw9".to_owned())),
            "id must be the interpolated reward_id: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "is_enabled": true })),
            "enable_reward must send exactly is_enabled:true"
        );
    }

    #[tokio::test]
    async fn reward_id_template_interpolates_from_stack() {
        let (transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "reward.id".to_owned(),
            Variant::String("resolved42".to_owned()),
        );

        let _ = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert!(
            transport
                .request(0)
                .query
                .contains(&("id".to_owned(), "resolved42".to_owned())),
            "reward_id must interpolate, not pass %reward.id% verbatim: {:?}",
            transport.request(0).query
        );
    }

    #[tokio::test]
    async fn empty_reward_id_fails_without_helix_call() {
        let (transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let cfg = BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty reward_id must short-circuit before PATCH"
        );
    }

    #[tokio::test]
    async fn validate_config_requires_a_non_blank_reward_id_and_defaults_a_missing_one() {
        let (_transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present non-empty", REWARD_CONFIG.default_config(), true),
            (
                "empty string",
                BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            (
                "missing key reads the declared template",
                BTreeMap::new(),
                true,
            ),
            (
                "whitespace only",
                BTreeMap::from([("reward_id".to_owned(), Variant::String("  ".to_owned()))]),
                false,
            ),
            (
                "wrong type",
                BTreeMap::from([("reward_id".to_owned(), Variant::Int(7))]),
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

    #[tokio::test]
    async fn helix_failure_maps_to_failed_with_status_and_no_token() {
        let (_transport, runner) = enable_runner_with(Err(TwitchError::Http {
            status: 403,
            body: format!("forbidden {TOKEN_SENTINEL}"),
        }));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw9".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_request_refused(&telemetry.outcome, 403);
    }

    #[tokio::test]
    async fn disable_patches_is_enabled_false() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = RewardToggleRunner::disable(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw1".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.query.contains(&("id".to_owned(), "rw1".to_owned())));
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "is_enabled": false }))
        );
    }

    #[tokio::test]
    async fn pause_patches_is_paused_true() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = RewardToggleRunner::pause(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw2".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.query.contains(&("id".to_owned(), "rw2".to_owned())));
        assert_eq!(request.body, Some(serde_json::json!({ "is_paused": true })));
    }

    #[tokio::test]
    async fn resume_patches_is_paused_false() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = RewardToggleRunner::resume(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw3".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.query.contains(&("id".to_owned(), "rw3".to_owned())));
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "is_paused": false }))
        );
    }
}
