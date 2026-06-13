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

const KIND_ID: &str = "twitch.poll.end";

// Allowed status strings from the config Select; stored as Variant::String.
// Mapped to uppercase before sending to Twitch (Twitch requires "TERMINATED" or "ARCHIVED").
const STATUS_TERMINATED: &str = "terminated";
const STATUS_ARCHIVED: &str = "archived";

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

    async fn end(&self, poll_id: &str, status_uppercase: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        // PATCH /helix/polls — broadcaster_id is a query param; id and status go in the body.
        // status must be uppercase: "TERMINATED" stops immediately, "ARCHIVED" ends and hides.
        // Requires channel:manage:polls scope.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/polls")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({
                "id": poll_id,
                "status": status_uppercase,
            }));

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for EndPollRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "poll_id".to_owned(),
                Variant::String("%poll.id%".to_owned()),
            ),
            (
                "status".to_owned(),
                Variant::String(STATUS_TERMINATED.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "poll_id",
                label: "Poll ID",
                placeholder: "%poll.id%",
            },
            FormField::Select {
                key: "status",
                label: "End Status",
                options: &[STATUS_TERMINATED, STATUS_ARCHIVED],
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let poll_id = match config.get("poll_id") {
            Some(Variant::String(s)) => s.as_str(),
            _ => "",
        };
        if poll_id.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'poll_id' is required"
            )));
        }

        match config.get("status") {
            Some(Variant::String(s)) if s == STATUS_TERMINATED || s == STATUS_ARCHIVED => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'status' must be '{STATUS_TERMINATED}' or '{STATUS_ARCHIVED}'"
                )));
            }
        }

        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let poll_id_template = config
            .get("poll_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let poll_id = ctx.arg_stack.interpolate(poll_id_template);

        if poll_id.is_empty() {
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("poll_id is required".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        // Config stores lowercase ("terminated"/"archived"); Twitch requires uppercase.
        let status_lower = config
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or(STATUS_TERMINATED);
        let status_uppercase = status_lower.to_uppercase();

        let outcome = self.end(&poll_id, &status_uppercase).await;

        (
            SubActionTelemetry {
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

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

    // Config stores lowercase; Twitch's PATCH body requires uppercase status.
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
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            unreachable!("expected Failed outcome on HTTP error");
        };
        assert!(!msg.contains(TOKEN_SENTINEL));
    }

    #[test]
    fn validate_config_requires_poll_id_and_known_status() {
        let runner = EndPollRunner::new(
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );

        // (label, config, expect_ok)
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
