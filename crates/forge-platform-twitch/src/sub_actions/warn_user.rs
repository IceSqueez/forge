use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    BoundedTextField, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::{MAX_REASON_CHARS, TARGET_LOGIN};
use crate::account::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::MODERATION_WARN_USER;

const REASON: BoundedTextField = BoundedTextField {
    key: config_keys::REASON,
    label: "Reason (required, max 500 chars)",
    editor: TextEditor::Area,
    default: "",
    required: true,
    length: ValueBounds {
        min: None,
        max: Some(MAX_REASON_CHARS),
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TARGET_LOGIN, &REASON]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String), ConfigProblem> {
    Ok((TARGET_LOGIN.read(config, args)?, REASON.read(config, args)?))
}

pub struct WarnUserRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl WarnUserRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn warn(&self, target_login: &str, reason: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        if reason.is_empty() {
            return SubActionOutcome::Failed("reason is required and must not be empty".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let target_user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/warnings")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({
                "data": {
                    "user_id": target_user_id,
                    "reason": reason,
                }
            }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for WarnUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Warn User"
    }

    fn summary(&self) -> &str {
        "Sends an official Twitch warning to a user in the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation warn warning user notice"
    }

    fn icon_name(&self) -> &str {
        "alert-triangle"
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
        let (target_login, reason) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.warn(&target_login, &reason).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx, users_fixture};

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, WarnUserRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = WarnUserRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn config(target: &str, reason: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(target.to_owned()),
            ),
            ("reason".to_owned(), Variant::String(reason.to_owned())),
        ])
    }

    #[tokio::test]
    async fn execute_posts_warning_with_resolved_id_and_reason_in_body() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("target", "first strike"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/moderation/warnings");
        assert_eq!(
            request.body,
            Some(serde_json::json!({
                "data": { "user_id": "555", "reason": "first strike" }
            })),
            "body must carry the RESOLVED id and the mandatory reason"
        );
    }

    #[tokio::test]
    async fn empty_reason_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("reason".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("target", "%reason%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "missing reason must fail before the resolve call"
        );
    }
}
