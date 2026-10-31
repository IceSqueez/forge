use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::AUTOMOD_REMOVE_BLOCKED_TERM;
const TERM_ID_TEMPLATE: &str = "%blocked_term.id%";

const TERM_ID: TextField = TextField {
    key: config_keys::TERM_ID,
    label: "Blocked Term ID",
    editor: TextEditor::Line {
        placeholder: TERM_ID_TEMPLATE,
    },
    default: TERM_ID_TEMPLATE,
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TERM_ID]);

pub(crate) struct RemoveBlockedTermRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl RemoveBlockedTermRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let term_id = match TERM_ID.read(config, ctx.arg_stack) {
            Ok(term_id) => term_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = delete_blocked_term(&self.transport, &self.identity, &term_id).await;
        (timer.finish(outcome), None)
    }
}

async fn delete_blocked_term(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    term_id: &str,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return step_failure::failed(&e),
    };
    let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/blocked_terms")
        .query("broadcaster_id", user_id.clone())
        .query("moderator_id", user_id)
        .query("id", term_id.to_owned());
    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => step_failure::failed(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, RemoveBlockedTermRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = RemoveBlockedTermRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        (transport, runner)
    }

    fn cfg(term_id: &str) -> SubActionConfig {
        BTreeMap::from([("term_id".to_owned(), Variant::String(term_id.to_owned()))])
    }

    #[tokio::test]
    async fn execute_issues_bodyless_delete_with_self_and_resolved_id() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "blocked_term.id".to_owned(),
            Variant::String("bt99".to_owned()),
        );

        let (telemetry, out) = runner
            .execute(&CONFIG.default_config(), &make_ctx(&stack))
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
        let stack =
            ArgStack::new().set("blocked_term.id".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&CONFIG.default_config(), &make_ctx(&stack))
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
            body: format!("term not found {TOKEN_SENTINEL}"),
        }));
        let stack = ArgStack::new().set(
            "blocked_term.id".to_owned(),
            Variant::String("bt99".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_request_refused(&telemetry.outcome, 404);
    }

    #[test]
    fn validate_config_requires_a_non_blank_term_id_and_defaults_a_missing_one() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("default %blocked_term.id%", CONFIG.default_config(), true),
            ("empty string", cfg(""), false),
            (
                "missing key reads the declared template",
                BTreeMap::new(),
                true,
            ),
            ("whitespace only", cfg("  "), false),
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
