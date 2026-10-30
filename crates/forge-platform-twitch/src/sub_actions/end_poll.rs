use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ChoiceField, ConfigChoice, ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::POLL_END;
const POLL_ID_TEMPLATE: &str = "%poll.id%";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PollEndStatus {
    Terminated,
    Archived,
}

impl PollEndStatus {
    fn helix_status(self) -> &'static str {
        match self {
            Self::Terminated => "TERMINATED",
            Self::Archived => "ARCHIVED",
        }
    }
}

impl ConfigChoice for PollEndStatus {
    const ALL: &'static [Self] = &[Self::Terminated, Self::Archived];
    const WIRE_NAMES: &'static [&'static str] = &["terminated", "archived"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Terminated => "terminated",
            Self::Archived => "archived",
        }
    }
}

const POLL_ID: TextField = TextField {
    key: config_keys::POLL_ID,
    label: "Poll ID",
    editor: TextEditor::Line {
        placeholder: POLL_ID_TEMPLATE,
    },
    default: POLL_ID_TEMPLATE,
    required: true,
};

const STATUS: ChoiceField<PollEndStatus> = ChoiceField {
    key: config_keys::STATUS,
    label: "End Status",
    default: PollEndStatus::Terminated,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&POLL_ID, &STATUS]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, PollEndStatus), ConfigProblem> {
    Ok((POLL_ID.read(config, args)?, STATUS.read(config, args)?))
}

pub struct EndPollRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl EndPollRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn end(&self, poll_id: &str, status: PollEndStatus) -> SubActionOutcome {
        if poll_id.is_empty() {
            return SubActionOutcome::Failed(messages::field_required("poll_id").into());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/polls")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({
                "id": poll_id,
                "status": status.helix_status(),
            }));
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for EndPollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::PollsPredictions
    }

    fn label(&self) -> &str {
        "End Poll"
    }

    fn summary(&self) -> &str {
        "Ends an active poll immediately. Use 'terminated' to stop with results visible or 'archived' to hide it."
    }

    fn search_text(&self) -> &str {
        "twitch poll end stop terminate archive"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
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
        let (poll_id, status) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.end(&poll_id, status).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, EndPollRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = EndPollRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(poll_id: &str, status: &str) -> SubActionConfig {
        BTreeMap::from([
            ("poll_id".to_owned(), Variant::String(poll_id.to_owned())),
            ("status".to_owned(), Variant::String(status.to_owned())),
        ])
    }

    #[tokio::test]
    async fn patches_polls_with_broadcaster_in_query_and_id_in_body() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(&cfg("poll-42", "terminated"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/polls");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        let body = request.body.unwrap();
        assert_eq!(body.get("id"), Some(&serde_json::json!("poll-42")));
        assert!(body.get("broadcaster_id").is_none());
    }

    #[tokio::test]
    async fn status_is_uppercased_in_body() {
        for (config_status, expected) in [("terminated", "TERMINATED"), ("archived", "ARCHIVED")] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();
            let (telemetry, _) = runner
                .execute(&cfg("poll-1", config_status), &make_ctx(&stack))
                .await;
            assert_eq!(telemetry.outcome, SubActionOutcome::Success);
            assert_eq!(
                transport.request(0).body.unwrap().get("status"),
                Some(&serde_json::json!(expected)),
                "config status: {config_status}"
            );
        }
    }

    #[tokio::test]
    async fn poll_id_is_interpolated_from_arg_stack() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack =
            ArgStack::new().set("poll.id".to_owned(), Variant::String("chained".to_owned()));
        let (telemetry, _) = runner
            .execute(&cfg("%poll.id%", "terminated"), &make_ctx(&stack))
            .await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap().get("id"),
            Some(&serde_json::json!("chained"))
        );
    }

    #[tokio::test]
    async fn empty_poll_id_fails_without_calling_helix() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();
        let (telemetry, _) = runner
            .execute(&cfg("", "terminated"), &make_ctx(&stack))
            .await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn http_failure_outcome_does_not_leak_token() {
        let (_, runner) = runner_with(Err(HelixError::Http {
            status: 404,
            body: "poll not found".to_owned(),
        }));
        let stack = ArgStack::new();
        let (telemetry, _) = runner
            .execute(&cfg("poll-1", "terminated"), &make_ctx(&stack))
            .await;
        assert_request_refused(&telemetry.outcome, 404);
    }

    #[test]
    fn validate_config_requires_poll_id_and_known_status() {
        let runner = EndPollRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );

        let cases = [
            ("valid terminated", cfg("poll-1", "terminated"), true),
            ("valid archived", cfg("poll-1", "archived"), true),
            ("empty poll_id", cfg("", "terminated"), false),
            (
                "uppercase status rejected",
                cfg("poll-1", "TERMINATED"),
                false,
            ),
            ("unknown status", cfg("poll-1", "cancelled"), false),
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
