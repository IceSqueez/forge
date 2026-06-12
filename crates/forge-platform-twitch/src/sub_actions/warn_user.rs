use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.moderation.warn_user";
const MAX_REASON_CHARS: usize = 500;

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
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            (
                "target_user_login".to_owned(),
                Variant::String(String::new()),
            ),
            ("reason".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "target_user_login",
                label: "Target Username",
                placeholder: "%user_login%",
            },
            FormField::TextArea {
                key: "reason",
                label: "Reason (required, max 500 chars)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_user_login") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'target_user_login' must be a non-empty string"
                )));
            }
        }
        match config.get("reason") {
            Some(Variant::String(r)) if !r.is_empty() => {
                if r.chars().count() > MAX_REASON_CHARS {
                    return Err(RegistryError::UnknownKindId(format!(
                        "{KIND_ID}: 'reason' must not exceed {MAX_REASON_CHARS} characters"
                    )));
                }
            }
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'reason' is required and must be a non-empty string"
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

        let login_template = config
            .get("target_user_login")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let target_login = ctx.arg_stack.interpolate(login_template);
        let reason = config
            .get("reason")
            .and_then(|v| v.as_str())
            .map(|s| ctx.arg_stack.interpolate(s))
            .unwrap_or_default();

        let outcome = self.warn(&target_login, &reason).await;

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
