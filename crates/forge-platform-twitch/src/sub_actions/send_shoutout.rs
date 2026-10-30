use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::SenderPreference;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, Interpolation, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::{SENDER, SendAs, TARGET_LOGIN};
use crate::account::identity::SelfIdentity;
use crate::account::user_target::{InterpolatedTarget, TargetFields, UserTarget};
use crate::bot_account::TwitchBotAccount;
use crate::cadence::CadenceLedger;
use crate::chat_sender::{ChatDelivery, ChatSenders, StepResult, failed_step, step_result};
use crate::helix::HelixTransport;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_SEND_SHOUTOUT;

const TARGET: TargetFields = TargetFields {
    id: TextField {
        key: config_keys::TO_BROADCASTER_ID,
        label: "Target Channel ID",
        editor: TextEditor::Line { placeholder: "" },
        default: "",
        required: false,
    },
    login: TextField {
        key: config_keys::TO_BROADCASTER_LOGIN,
        label: "Target Channel",
        required: false,
        ..TARGET_LOGIN
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TARGET.login, &SENDER]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(InterpolatedTarget, SendAs), ConfigProblem> {
    Ok((TARGET.read(config, args)?, SENDER.read(config, args)?))
}

pub struct SendShoutoutRunner {
    senders: ChatSenders,
}

impl SendShoutoutRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            senders: ChatSenders::new(transport, identity),
        }
    }

    pub fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.senders = self.senders.with_bot(bot);
        self
    }

    pub(crate) fn with_cadence(mut self, cadence: Arc<CadenceLedger>) -> Self {
        self.senders = self.senders.with_cadence(cadence);
        self
    }

    async fn shoutout(
        &self,
        target: Option<UserTarget<'_>>,
        sender: SenderPreference,
        events: &dyn EventPublisher,
    ) -> StepResult {
        let Some(target) = target else {
            return failed_step(TARGET.empty_after_interpolation());
        };
        step_result(
            self.senders
                .deliver(ChatDelivery::Shoutout { target }, sender, events)
                .await,
        )
    }
}

#[async_trait]
impl SubActionRunner for SendShoutoutRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Send Shoutout"
    }

    fn summary(&self) -> &str {
        "Sends a shoutout to another broadcaster in the channel chat."
    }

    fn search_text(&self) -> &str {
        "twitch shoutout so channel raid highlight streamer"
    }

    fn icon_name(&self) -> &str {
        "shoutout"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn check_config(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        TARGET.check(config, interpolation)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (target, sender) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let (outcome, produced) = self
            .shoutout(target.target(), sender.0, ctx.publisher)
            .await;
        let mut telemetry = timer.finish(outcome);
        telemetry.produced = produced;
        (telemetry, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_failed_with_detail, make_ctx, users_fixture,
    };
    use forge_platform_core::SENDER_FIELD;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, SendShoutoutRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = SendShoutoutRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(login: &str) -> SubActionConfig {
        BTreeMap::from([(
            "to_broadcaster_login".to_owned(),
            Variant::String(login.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_shoutout_with_self_as_from_and_moderator() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new().set(
            "user_login".to_owned(),
            Variant::String("target".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&config("%user_login%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then shoutout");

        let resolve = transport.request(0);
        assert_eq!(resolve.method, HelixMethod::Get);
        assert_eq!(resolve.path, "/helix/users");
        assert!(
            resolve
                .query
                .contains(&("login".to_owned(), "target".to_owned())),
            "resolve must look up the interpolated login: {:?}",
            resolve.query
        );

        let act = transport.last_request();
        assert_eq!(act.method, HelixMethod::Post);
        assert_eq!(act.path, "/helix/chat/shoutouts");
        assert!(
            act.query
                .contains(&("from_broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "from must be self: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("to_broadcaster_id".to_owned(), "555".to_owned())),
            "to must be the RESOLVED id, not the login: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
            "moderator must be self: {:?}",
            act.query
        );
        assert_eq!(act.body, None, "shoutout carries no JSON body");
    }

    #[tokio::test]
    async fn resolve_failure_skips_the_shoutout_call() {
        let (transport, runner) = runner_with(vec![Err(HelixError::Http {
            status: 404,
            body: "user not found".to_owned(),
        })]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("ghost"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "resolve failure must not issue the shoutout POST"
        );
    }

    #[tokio::test]
    async fn shoutout_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 429,
                body: "shoutout ratelimited".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the shoutout call, not the resolve"
        );
        assert_failed_with_detail(&telemetry.outcome, "twitch.request_failed", "HTTP 429");
    }

    #[tokio::test]
    async fn a_sent_shoutout_records_the_acting_sender() {
        let (_transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);

        let (telemetry, _) = runner
            .execute(&config("target"), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                &telemetry.produced[forge_platform_core::SENT_AS_FIELD]
            )
            .unwrap(),
            serde_json::json!({"role": "streamer", "login": "streamer"})
        );
    }

    #[tokio::test]
    async fn unknown_sender_is_rejected_by_validate_and_by_execute_without_a_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let mut config = config("target");
        config.insert(SENDER_FIELD.to_owned(), Variant::Int(0));

        let validation = runner.validate_config(&config);
        let (telemetry, _) = runner.execute(&config, &make_ctx(&ArgStack::new())).await;

        assert!(validation.is_err());
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }
}
