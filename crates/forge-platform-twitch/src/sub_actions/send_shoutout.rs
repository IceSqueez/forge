use crate::vocabulary::sub_action_ids;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::account::identity::SelfIdentity;
use crate::account::user_target::{TargetKeys, UserTarget};
use crate::bot_account::TwitchBotAccount;
use crate::cadence::CadenceLedger;
use crate::chat_sender::{
    ChatDelivery, ChatSenders, StepResult, configured_sender, failed_step, step_result,
};
use crate::helix::HelixTransport;

const KIND_ID: &str = sub_action_ids::CHANNEL_SEND_SHOUTOUT;
const TARGET_KEYS: TargetKeys = TargetKeys {
    id: "to_broadcaster_id",
    login: "to_broadcaster_login",
};

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
            return failed_step(&TARGET_KEYS.empty_after_interpolation());
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (TARGET_KEYS.login.to_owned(), Variant::String(String::new())),
            (
                SENDER_FIELD.to_owned(),
                Variant::String(SenderPreference::Default.as_str().to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: TARGET_KEYS.login,
                label: "Target Channel",
                placeholder: "%user_login%",
            },
            FormField::Select {
                key: SENDER_FIELD,
                label: "Send As",
                options: SenderPreference::WIRE_NAMES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        TARGET_KEYS.validate(config)?;
        configured_sender(config).map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let interpolated = TARGET_KEYS.interpolate(config, ctx);

        let (outcome, produced) = match configured_sender(config) {
            Ok(sender) => {
                self.shoutout(interpolated.target(), sender, ctx.publisher)
                    .await
            }
            Err(e) => failed_step(&e.to_string()),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced,
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
mod tests {
    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx, users_fixture,
    };
    use forge_types::SubActionOutcome;

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
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("429"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
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
