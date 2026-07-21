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

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel_points.fulfill_redemption";

pub struct FulfillRedemptionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl FulfillRedemptionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn redemption_default_config() -> SubActionConfig {
    BTreeMap::from([
        (
            "redemption_id".to_owned(),
            Variant::String("%redemption.id%".to_owned()),
        ),
        (
            "reward_id".to_owned(),
            Variant::String("%reward.id%".to_owned()),
        ),
    ])
}

pub(crate) fn redemption_config_fields() -> Vec<FormField> {
    vec![
        FormField::Text {
            key: "redemption_id",
            label: "Redemption ID",
            placeholder: "%redemption.id%",
        },
        FormField::Text {
            key: "reward_id",
            label: "Reward ID",
            placeholder: "%reward.id%",
        },
    ]
}

pub(crate) fn validate_redemption_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("redemption_id") {
        Some(Variant::String(s)) if !s.is_empty() => {}
        _ => {
            return Err(RegistryError::InvalidConfig(format!(
                "{kind_id}: 'redemption_id' is required"
            )));
        }
    }
    match config.get("reward_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: 'reward_id' is required"
        ))),
    }
}

/// PATCH /helix/channel_points/custom_rewards/redemptions with three query params and
/// status in the body. Twitch requires broadcaster_id + reward_id + id as query params
/// (not body) with the new status as the sole body field.
pub(crate) async fn patch_redemption_status(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    redemption_id: &str,
    reward_id: &str,
    status: &str,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let mut body = serde_json::Map::new();
    body.insert("status".to_owned(), status.into());

    let request = HelixRequest::new(
        HelixMethod::Patch,
        "/helix/channel_points/custom_rewards/redemptions",
    )
    .query("broadcaster_id", user_id)
    .query("reward_id", reward_id.to_owned())
    .query("id", redemption_id.to_owned())
    .body(serde_json::Value::Object(body));

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
    }
}

pub(crate) async fn execute_redemption_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    status: &str,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let redemption_id_template = config.str("redemption_id").unwrap_or_default();
    let redemption_id = ctx.arg_stack.interpolate(redemption_id_template);

    let reward_id_template = config.str("reward_id").unwrap_or_default();
    let reward_id = ctx.arg_stack.interpolate(reward_id_template);

    let outcome = if redemption_id.is_empty() {
        SubActionOutcome::Failed("redemption_id is required".to_owned())
    } else if reward_id.is_empty() {
        SubActionOutcome::Failed("reward_id is required".to_owned())
    } else {
        patch_redemption_status(
            transport,
            identity,
            kind_id,
            &redemption_id,
            &reward_id,
            status,
        )
        .await
    };

    (
        SubActionTelemetry {
            args_in: ::std::collections::BTreeMap::new(),
            produced: ::std::collections::BTreeMap::new(),
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
impl SubActionRunner for FulfillRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Fulfill Channel Point Redemption"
    }

    fn summary(&self) -> &str {
        "Marks a channel point redemption as fulfilled."
    }

    fn search_text(&self) -> &str {
        "twitch channel points redemption fulfill complete done"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn default_config(&self) -> SubActionConfig {
        redemption_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        redemption_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_redemption_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_redemption_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "FULFILLED",
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

    fn fulfill_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, FulfillRedemptionRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = FulfillRedemptionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn redemption_stack() -> ArgStack {
        ArgStack::new()
            .set(
                "redemption.id".to_owned(),
                Variant::String("rd5".to_owned()),
            )
            .set("reward.id".to_owned(), Variant::String("rw7".to_owned()))
    }

    // Distinct PATCH shape AND the shared three-query-param contract (asserted ONCE
    // here as the representative redemption runner): broadcaster_id=self, reward_id
    // and id both resolved from the stack, with body {"status":"FULFILLED"}.
    #[tokio::test]
    async fn fulfill_patches_all_three_query_params_with_fulfilled_body() {
        let (transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));

        let (telemetry, out) = runner
            .execute(&redemption_default_config(), &make_ctx(&redemption_stack()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "redemption runners never push an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(
            request.path,
            "/helix/channel_points/custom_rewards/redemptions"
        );
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "missing broadcaster_id=self: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("reward_id".to_owned(), "rw7".to_owned())),
            "reward_id must be the interpolated reward.id: {:?}",
            request.query
        );
        assert!(
            request.query.contains(&("id".to_owned(), "rd5".to_owned())),
            "id must be the interpolated redemption.id: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "status": "FULFILLED" })),
            "fulfill must send status FULFILLED"
        );
    }

    // SHARED: both ids interpolate from their distinct templates - reward_id from
    // %reward.id%, id from %redemption.id% - not passed through verbatim.
    #[tokio::test]
    async fn both_ids_interpolate_from_their_own_templates() {
        let (transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new()
            .set(
                "redemption.id".to_owned(),
                Variant::String("redA".to_owned()),
            )
            .set("reward.id".to_owned(), Variant::String("rewB".to_owned()));

        let _ = runner
            .execute(&redemption_default_config(), &make_ctx(&stack))
            .await;

        let query = transport.request(0).query;
        assert!(
            query.contains(&("id".to_owned(), "redA".to_owned())),
            "redemption id must interpolate from %redemption.id%: {query:?}"
        );
        assert!(
            query.contains(&("reward_id".to_owned(), "rewB".to_owned())),
            "reward_id must interpolate from %reward.id%: {query:?}"
        );
    }

    // SHARED: empty redemption_id fails before any Helix call.
    #[tokio::test]
    async fn empty_redemption_id_fails_without_helix_call() {
        let (transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([
            ("redemption_id".to_owned(), Variant::String(String::new())),
            ("reward_id".to_owned(), Variant::String("rw7".to_owned())),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&ArgStack::new())).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty redemption_id must short-circuit before PATCH"
        );
    }

    // SHARED: empty reward_id (with a valid redemption_id) also fails before the call.
    #[tokio::test]
    async fn empty_reward_id_fails_without_helix_call() {
        let (transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([
            (
                "redemption_id".to_owned(),
                Variant::String("rd5".to_owned()),
            ),
            ("reward_id".to_owned(), Variant::String(String::new())),
        ]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&ArgStack::new())).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty reward_id must short-circuit before PATCH"
        );
    }

    // SHARED: validate_config gates on BOTH ids being non-empty Strings.
    #[tokio::test]
    async fn validate_config_requires_both_ids_non_empty() {
        let (_transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("both present", redemption_default_config(), true),
            (
                "empty redemption_id",
                BTreeMap::from([
                    ("redemption_id".to_owned(), Variant::String(String::new())),
                    ("reward_id".to_owned(), Variant::String("rw7".to_owned())),
                ]),
                false,
            ),
            (
                "empty reward_id",
                BTreeMap::from([
                    (
                        "redemption_id".to_owned(),
                        Variant::String("rd5".to_owned()),
                    ),
                    ("reward_id".to_owned(), Variant::String(String::new())),
                ]),
                false,
            ),
            (
                "missing redemption_id",
                BTreeMap::from([("reward_id".to_owned(), Variant::String("rw7".to_owned()))]),
                false,
            ),
            (
                "missing reward_id",
                BTreeMap::from([(
                    "redemption_id".to_owned(),
                    Variant::String("rd5".to_owned()),
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

    // SHARED: a Helix failure maps to Failed without leaking the sentinel token.
    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = fulfill_runner_with(Err(HelixError::Http {
            status: 401,
            body: "unauthorized".to_owned(),
        }));

        let (telemetry, _) = runner
            .execute(&redemption_default_config(), &make_ctx(&redemption_stack()))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
