use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};

use crate::account::identity::SelfIdentity;
use crate::collection_keys::REWARD_OPTIONS_KEY;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_FULFILL_REDEMPTION;
const REDEMPTION_ID_TEMPLATE: &str = "%redemption.id%";

const REDEMPTION_ID: TextField = TextField {
    key: config_keys::REDEMPTION_ID,
    label: "Redemption ID",
    editor: TextEditor::Line {
        placeholder: REDEMPTION_ID_TEMPLATE,
    },
    default: REDEMPTION_ID_TEMPLATE,
    required: true,
};

const REWARD_ID: PickedTextField = PickedTextField {
    key: config_keys::REWARD_ID,
    label: "Reward ID",
    picker: TextPicker::Options {
        options_key: REWARD_OPTIONS_KEY,
    },
    default: "%reward.id%",
    required: true,
};

pub(crate) const REDEMPTION_CONFIG: ConfigSpec = ConfigSpec::new(&[&REDEMPTION_ID, &REWARD_ID]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((
        REDEMPTION_ID.read(config, args)?,
        REWARD_ID.read(config, args)?,
    ))
}

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

async fn patch_redemption_status(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    redemption_id: &str,
    reward_id: &str,
    status: &str,
) -> SubActionOutcome {
    if redemption_id.is_empty() {
        return SubActionOutcome::Failed(UserText::unlocalized("redemption_id is required"));
    }
    if reward_id.is_empty() {
        return SubActionOutcome::Failed(UserText::unlocalized("reward_id is required"));
    }
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
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
        Err(e) => SubActionOutcome::Failed(UserText::unlocalized(format!("{kind_id}: {e}"))),
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
    let timer = StepTimer::start(ctx, kind_id);
    let (redemption_id, reward_id) = match read_config(config, ctx.arg_stack) {
        Ok(values) => values,
        Err(problem) => return (timer.invalid_config(&problem), None),
    };
    let outcome = patch_redemption_status(
        transport,
        identity,
        kind_id,
        &redemption_id,
        &reward_id,
        status,
    )
    .await;
    (timer.finish(outcome), None)
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(REDEMPTION_CONFIG)
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
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

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

    #[tokio::test]
    async fn fulfill_patches_all_three_query_params_with_fulfilled_body() {
        let (transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));

        let (telemetry, out) = runner
            .execute(
                &REDEMPTION_CONFIG.default_config(),
                &make_ctx(&redemption_stack()),
            )
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
            .execute(&REDEMPTION_CONFIG.default_config(), &make_ctx(&stack))
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

    #[tokio::test]
    async fn validate_config_requires_both_ids_non_empty() {
        let (_transport, runner) = fulfill_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("both present", REDEMPTION_CONFIG.default_config(), true),
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
                "missing redemption_id reads the declared template",
                BTreeMap::from([("reward_id".to_owned(), Variant::String("rw7".to_owned()))]),
                true,
            ),
            (
                "missing reward_id reads the declared template",
                BTreeMap::from([(
                    "redemption_id".to_owned(),
                    Variant::String("rd5".to_owned()),
                )]),
                true,
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
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = fulfill_runner_with(Err(HelixError::Http {
            status: 401,
            body: "unauthorized".to_owned(),
        }));

        let (telemetry, _) = runner
            .execute(
                &REDEMPTION_CONFIG.default_config(),
                &make_ctx(&redemption_stack()),
            )
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if !msg.code_or_text().contains(TOKEN_SENTINEL)
        ));
    }
}
