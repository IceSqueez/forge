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

const KIND_ID: &str = "twitch.channel_points.enable_reward";

pub struct EnableRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl EnableRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

/// PATCH /helix/channel_points/custom_rewards with a single boolean field.
/// Requires channel:manage:redemptions scope. Only the supplied body key is
/// changed; Twitch leaves all other reward fields as-is.
pub(crate) async fn patch_reward_bool(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    reward_id: &str,
    body_key: &str,
    value: bool,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let mut body = serde_json::Map::new();
    body.insert(body_key.to_owned(), value.into());

    let request = HelixRequest::new(HelixMethod::Patch, "/helix/channel_points/custom_rewards")
        .query("broadcaster_id", user_id)
        .query("id", reward_id.to_owned())
        .body(serde_json::Value::Object(body));

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    }
}

pub(crate) fn default_config() -> SubActionConfig {
    BTreeMap::from([(
        "reward_id".to_owned(),
        Variant::String("%reward.id%".to_owned()),
    )])
}

pub(crate) fn config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "reward_id",
        label: "Reward ID",
        placeholder: "%reward.id%",
    }]
}

pub(crate) fn validate_reward_id(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("reward_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'reward_id' is required"
        ))),
    }
}

pub(crate) async fn execute_bool_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    body_key: &str,
    value: bool,
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
        patch_reward_bool(transport, identity, &reward_id, body_key, value).await
    };

    (
        SubActionTelemetry {
            kind: kind_id.to_owned(),
            started_at,
            duration_ms: start.elapsed().as_millis() as u64,
            outcome,
            index: ctx.index,
        },
        None,
    )
}

#[async_trait]
impl SubActionRunner for EnableRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Enable Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Enables a custom channel point reward so viewers can redeem it."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward enable on redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn default_config(&self) -> SubActionConfig {
        default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_reward_id(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_bool_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "is_enabled",
            true,
            config,
            ctx,
        )
        .await
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

    fn enable_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, EnableRewardRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = EnableRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    // Distinct-body contract for enable_reward: PATCH the custom_rewards endpoint
    // with both query params (broadcaster_id=self AND the resolved id) and a body
    // of exactly {"is_enabled": true}. This is the one body assertion EnableReward
    // owns; the other three runners assert their own distinct bodies in-file.
    #[tokio::test]
    async fn enable_patches_is_enabled_true_with_both_query_params() {
        let (transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw9".to_owned()));
        let cfg = default_config();

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

    // SHARED behavior (asserted ONCE via the representative runner): the reward_id
    // template resolves through the ArgStack. The default config holds %reward.id%,
    // so the query id must equal the stack-resolved value, not the literal template.
    #[tokio::test]
    async fn reward_id_template_interpolates_from_stack() {
        let (transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "reward.id".to_owned(),
            Variant::String("resolved42".to_owned()),
        );

        let _ = runner.execute(&default_config(), &make_ctx(&stack)).await;

        assert!(
            transport
                .request(0)
                .query
                .contains(&("id".to_owned(), "resolved42".to_owned())),
            "reward_id must interpolate, not pass %reward.id% verbatim: {:?}",
            transport.request(0).query
        );
    }

    // SHARED behavior: an empty reward_id after interpolation fails BEFORE any Helix
    // call (no broadcaster targeted). Empty stack leaves %reward.id% unresolved, but
    // an explicitly empty template is the deterministic empty case.
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

    // SHARED behavior: validate_config gates on a non-empty reward_id String.
    #[tokio::test]
    async fn validate_config_requires_non_empty_reward_id() {
        let (_transport, runner) = enable_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present non-empty", default_config(), true),
            (
                "empty string",
                BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            ("missing key", BTreeMap::new(), false),
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

    // SHARED behavior: a Helix failure surfaces as Failed carrying the status, and
    // the sentinel token never leaks into the outcome message.
    #[tokio::test]
    async fn helix_failure_maps_to_failed_with_status_and_no_token() {
        let (_transport, runner) = enable_runner_with(Err(HelixError::Http {
            status: 403,
            body: "forbidden".to_owned(),
        }));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw9".to_owned()));

        let (telemetry, _) = runner.execute(&default_config(), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("403") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
