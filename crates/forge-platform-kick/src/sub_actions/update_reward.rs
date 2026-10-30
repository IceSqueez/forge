use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, Interpolation, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};
use futures::future::BoxFuture;

use super::field_kinds::{Blankable, WholeNumberField, non_empty, stores_blank};
use super::fields::REWARD_ID;
use crate::api::rewards::{KickRewards, UpdateRewardParams};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::REWARD_UPDATE;
const MIN_COST: i64 = 1;
const KEEP_CURRENT_HINT: &str = "Leave empty to keep current";

const TITLE: TextField = TextField {
    key: config_keys::TITLE,
    label: "New Title (optional)",
    editor: TextEditor::Line {
        placeholder: KEEP_CURRENT_HINT,
    },
    default: "",
    required: false,
};

const COST: Blankable<WholeNumberField> = Blankable(WholeNumberField {
    key: config_keys::COST,
    label: "New Cost (optional)",
    placeholder: KEEP_CURRENT_HINT,
    bounds: ValueBounds {
        min: Some(MIN_COST),
        max: None,
    },
});

const DESCRIPTION: TextField = TextField {
    key: config_keys::DESCRIPTION,
    label: "New Description (optional)",
    editor: TextEditor::Line {
        placeholder: KEEP_CURRENT_HINT,
    },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&REWARD_ID, &TITLE, &COST, &DESCRIPTION]);

struct RewardUpdate {
    reward_id: String,
    title: Option<String>,
    cost: Option<u64>,
    description: Option<String>,
}

impl RewardUpdate {
    fn changes_nothing(&self) -> bool {
        self.title.is_none() && self.cost.is_none() && self.description.is_none()
    }
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<RewardUpdate, ConfigProblem> {
    Ok(RewardUpdate {
        reward_id: REWARD_ID.read(config, args)?,
        title: non_empty(TITLE.read(config, args)?),
        cost: COST.read(config, args)?,
        description: non_empty(DESCRIPTION.read(config, args)?),
    })
}

pub struct UpdateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateRewardRunner {
    pub fn new(
        client: Arc<KickRewards>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for UpdateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Update Channel Reward"
    }

    fn summary(&self) -> &str {
        "Updates an existing Kick channel reward. Requires channel:rewards:write scope and reward ownership."
    }

    fn search_text(&self) -> &str {
        "kick channel reward update edit points"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        let updatable = [
            config_keys::TITLE,
            config_keys::COST,
            config_keys::DESCRIPTION,
        ];
        if updatable.iter().all(|key| stores_blank(config, key)) {
            Err(ConfigProblem::whole(ConfigReason::any_of(updatable)))
        } else {
            Ok(())
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let update = match read_config(config, ctx.arg_stack) {
            Ok(update) => update,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if update.reward_id.is_empty() {
            return (
                timer.failed(UserText::unlocalized(
                    "reward_id is empty after interpolation",
                )),
                None,
            );
        }
        if update.changes_nothing() {
            return (
                timer.failed(UserText::unlocalized(
                    "all updatable fields are empty after interpolation; nothing to update",
                )),
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(UserText::unlocalized(format!("token error: {e}"))),
            Ok(token) => SubActionOutcome::from_result(
                &self
                    .client
                    .update(
                        &update.reward_id,
                        UpdateRewardParams {
                            title: update.title,
                            cost: update.cost,
                            description: update.description,
                            background_color: None,
                            is_enabled: None,
                            is_paused: None,
                            is_user_input_required: None,
                            should_redemptions_skip_request_queue: None,
                        },
                        &token,
                    )
                    .await,
            ),
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RegistryError;
    use forge_types::EventId;
    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UpdateRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::api_at(&server.uri()));
        UpdateRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> UpdateRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::unreachable_api());
        UpdateRewardRunner::new(Arc::new(client), token_source())
    }

    fn config(reward_id: &str, title: &str, cost: &str, description: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "reward_id".to_owned(),
                Variant::String(reward_id.to_owned()),
            ),
            ("title".to_owned(), Variant::String(title.to_owned())),
            ("cost".to_owned(), Variant::String(cost.to_owned())),
            (
                "description".to_owned(),
                Variant::String(description.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_with_reward_id_and_one_field_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(
                &config("rw_1", "New Title", "", ""),
                &make_ctx(&ArgStack::new()),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn empty_reward_id_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("id".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("%id%", "New", "", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an empty resolved reward_id must not reach the transport"
        );
    }

    #[tokio::test]
    async fn non_numeric_cost_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("rw_1", "", "lots", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric cost must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn all_updatable_fields_empty_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("e".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("rw_1", "%e%", "%e%", "%e%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an all-empty resolved update must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_reward_id_and_at_least_one_updatable_field() {
        let runner = runner_offline();
        let no_reward = forge_registry::ConfigProblem::field("reward_id", ConfigReason::Required);
        let no_update = forge_registry::ConfigProblem::whole(ConfigReason::any_of([
            "title",
            "cost",
            "description",
        ]));
        for (label, cfg, expected) in [
            ("no reward_id", config("", "New", "", ""), Some(no_reward)),
            (
                "reward_id but no fields",
                config("rw_1", "", "", ""),
                Some(no_update),
            ),
            ("reward_id + title", config("rw_1", "New", "", ""), None),
            ("reward_id + cost", config("rw_1", "", "500", ""), None),
            ("reward_id + description", config("rw_1", "", "", "d"), None),
        ] {
            assert_eq!(
                runner
                    .validate_config(&cfg)
                    .err()
                    .map(RegistryError::into_config_problem),
                expected,
                "case: {label}"
            );
        }
    }
}
