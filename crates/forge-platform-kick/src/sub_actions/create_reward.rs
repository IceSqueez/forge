use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use futures::future::BoxFuture;

use super::field_kinds::{WholeNumberField, non_empty};
use super::step_failure;
use crate::api::rewards::{CreateRewardParams, KickRewards};
use crate::vocabulary::variables::created_reward;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::REWARD_CREATE;
const MIN_COST: i64 = 1;

const TITLE: TextField = TextField {
    key: config_keys::TITLE,
    label: "Reward Title",
    editor: TextEditor::Line {
        placeholder: "e.g. Hydrate",
    },
    default: "",
    required: true,
};

const COST: WholeNumberField = WholeNumberField {
    key: config_keys::COST,
    label: "Cost (channel points)",
    placeholder: "e.g. 500",
    bounds: ValueBounds {
        min: Some(MIN_COST),
        max: None,
    },
};

const DESCRIPTION: TextField = TextField {
    key: config_keys::DESCRIPTION,
    label: "Description (optional)",
    editor: TextEditor::Line {
        placeholder: "Leave empty to omit",
    },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE, &COST, &DESCRIPTION]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<CreateRewardParams, ConfigProblem> {
    Ok(CreateRewardParams {
        title: TITLE.read(config, args)?,
        cost: COST.read(config, args)?,
        description: non_empty(DESCRIPTION.read(config, args)?),
        background_color: None,
        is_enabled: None,
        is_user_input_required: None,
        should_redemptions_skip_request_queue: None,
    })
}

pub(crate) struct CreateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl CreateRewardRunner {
    pub(crate) fn new(
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
impl SubActionRunner for CreateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Create Channel Reward"
    }

    fn summary(&self) -> &str {
        "Creates a new channel point reward on Kick. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward create points redeem"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![created_reward::ID.to_owned()],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let params = match read_config(config, ctx.arg_stack) {
            Ok(params) => params,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let token = match (self.token_source)().await {
            Ok(token) => token,
            Err(e) => {
                return (timer.failed(step_failure::token_failed(&e)), None);
            }
        };
        match self.client.create(params, &token).await {
            Ok(Some(reward_id)) => {
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(created_reward::ID.to_owned(), Variant::String(reward_id));
                (timer.success(), Some(stack))
            }
            Ok(None) => (timer.success(), None),
            Err(e) => (timer.failed(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{make_ctx, token_source};

    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> CreateRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::api_at(&server.uri()));
        CreateRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> CreateRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::unreachable_api());
        CreateRewardRunner::new(Arc::new(client), token_source())
    }

    fn config(title: &str, cost: &str, description: &str) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(title.to_owned())),
            ("cost".to_owned(), Variant::String(cost.to_owned())),
            (
                "description".to_owned(),
                Variant::String(description.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_with_interpolated_title_and_cost_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(serde_json::json!({"id": "x"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String("Hydrate".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("%g%", "500", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = server.received_requests().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    async fn created_with_response(body: ResponseTemplate) -> Option<ArgStack> {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(body)
            .mount(&server)
            .await;
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("alice".to_owned()));

        let (telemetry, produced) = runner_on(&server)
            .execute(&config("Hydrate", "500", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        produced
    }

    #[tokio::test]
    async fn a_created_reward_publishes_its_id_on_top_of_the_incoming_variables() {
        for body in [
            serde_json::json!({"id": "rw-9"}),
            serde_json::json!({"data": {"id": "rw-9"}}),
        ] {
            let produced = created_with_response(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(&body),
            )
            .await
            .unwrap_or_else(|| panic!("no arg stack for {body}"));

            assert_eq!(
                produced.get("kick.reward.id"),
                Some(&Variant::String("rw-9".to_owned())),
                "{body}"
            );
            assert_eq!(
                produced.get("user"),
                Some(&Variant::String("alice".to_owned())),
                "{body}"
            );
        }
    }

    #[tokio::test]
    async fn a_created_reward_without_a_readable_id_succeeds_without_an_arg_stack() {
        for body in [
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .set_body_json(serde_json::json!({"data": {}})),
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("not json"),
            ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT),
        ] {
            assert!(created_with_response(body).await.is_none());
        }
    }

    #[tokio::test]
    async fn non_numeric_cost_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("Hydrate", "lots", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric cost must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn zero_cost_fails_the_minimum_guard_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("Hydrate", "0", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a cost below 1 must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn empty_title_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("%g%", "500", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an empty resolved title must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_non_empty_title_and_cost() {
        let runner = runner_offline();
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("both empty", config("", "", ""), false),
            ("title only", config("Hydrate", "", ""), false),
            ("cost only", config("", "500", ""), false),
            ("both present", config("Hydrate", "500", ""), true),
        ];
        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
