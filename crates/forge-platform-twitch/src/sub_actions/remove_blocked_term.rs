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

const KIND_ID: &str = "twitch.automod.remove_blocked_term";

pub struct RemoveBlockedTermRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl RemoveBlockedTermRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn remove_blocked_term_default_config() -> SubActionConfig {
    BTreeMap::from([(
        "term_id".to_owned(),
        // Default chains from add_blocked_term output so add→remove sequences work without
        // manual config.
        Variant::String("%blocked_term.id%".to_owned()),
    )])
}

pub(crate) fn remove_blocked_term_config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "term_id",
        // The Twitch DELETE endpoint requires the blocked-term ID (not the text).
        // Use %blocked_term.id% to chain from add_blocked_term output.
        label: "Blocked Term ID",
        placeholder: "%blocked_term.id%",
    }]
}

pub(crate) fn validate_remove_blocked_term_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("term_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'term_id' is required"
        ))),
    }
}

#[async_trait]
impl SubActionRunner for RemoveBlockedTermRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Remove Blocked Term"
    }

    fn summary(&self) -> &str {
        "Removes a term from the AutoMod blocked terms list by its ID."
    }

    fn search_text(&self) -> &str {
        "twitch automod blocked term remove delete unblock word phrase moderation"
    }

    fn icon_name(&self) -> &str {
        "slash-off"
    }

    fn default_config(&self) -> SubActionConfig {
        remove_blocked_term_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        remove_blocked_term_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_remove_blocked_term_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let term_id_template = config
            .get("term_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let term_id = ctx.arg_stack.interpolate(term_id_template);

        if term_id.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("term_id is required".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = delete_blocked_term(&self.transport, &self.identity, KIND_ID, &term_id).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

// DELETE /helix/moderation/blocked_terms
// broadcaster_id = moderator_id = self (broadcaster manages their own channel's blocked terms)
// `id` query param is the blocked-term UUID - NOT the blocked text itself.
// Returns 204 No Content on success.
async fn delete_blocked_term(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    term_id: &str,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/blocked_terms")
        .query("broadcaster_id", user_id.clone())
        .query("moderator_id", user_id)
        // Twitch DELETE takes the term UUID, not the term text string.
        .query("id", term_id.to_owned());

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
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
    ) -> (Arc<MockTransport>, RemoveBlockedTermRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = RemoveBlockedTermRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(term_id: &str) -> SubActionConfig {
        BTreeMap::from([("term_id".to_owned(), Variant::String(term_id.to_owned()))])
    }

    // Default config chains %blocked_term.id% from add_blocked_term output, so the
    // default path must resolve that global into the DELETE `id` query param.
    #[tokio::test]
    async fn execute_issues_bodyless_delete_with_self_and_resolved_id() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "blocked_term.id".to_owned(),
            Variant::String("bt99".to_owned()),
        );

        let (telemetry, out) = runner
            .execute(&remove_blocked_term_default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(
            out.is_none(),
            "remove_blocked_term never pushes an ArgStack"
        );
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/moderation/blocked_terms");
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
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
            "missing moderator_id=self: {:?}",
            request.query
        );
        assert!(
            request
                .query
                .contains(&("id".to_owned(), "bt99".to_owned())),
            "id must be the interpolated term_id: {:?}",
            request.query
        );
        assert_eq!(request.body, None, "DELETE blocked_terms carries no body");
    }

    #[tokio::test]
    async fn empty_interpolated_term_id_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        // %blocked_term.id% resolves to an empty string → short-circuit before DELETE.
        let stack =
            ArgStack::new().set("blocked_term.id".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&remove_blocked_term_default_config(), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty term_id must short-circuit before DELETE"
        );
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 404,
            body: "term not found".to_owned(),
        }));
        let stack = ArgStack::new().set(
            "blocked_term.id".to_owned(),
            Variant::String("bt99".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&remove_blocked_term_default_config(), &make_ctx(&stack))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("404") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    #[test]
    fn validate_config_requires_non_empty_term_id() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            (
                "default %blocked_term.id%",
                remove_blocked_term_default_config(),
                true,
            ),
            ("empty string", cfg(""), false),
            ("missing key", BTreeMap::new(), false),
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
