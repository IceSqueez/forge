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

const KIND_ID: &str = "twitch.automod.add_blocked_term";

// Twitch enforces 2..=500 characters on the blocked term text.
// Reference: https://dev.twitch.tv/docs/api/reference/#add-blocked-term
const MIN_TERM_CHARS: usize = 2;
const MAX_TERM_CHARS: usize = 500;

pub struct AddBlockedTermRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl AddBlockedTermRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn blocked_term_default_config() -> SubActionConfig {
    BTreeMap::from([("text".to_owned(), Variant::String(String::new()))])
}

pub(crate) fn blocked_term_config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "text",
        label: "Term to block",
        placeholder: "word or phrase (2–500 characters)",
    }]
}

pub(crate) fn validate_blocked_term_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("text") {
        Some(Variant::String(s))
            if s.chars().count() >= MIN_TERM_CHARS && s.chars().count() <= MAX_TERM_CHARS =>
        {
            Ok(())
        }
        Some(Variant::String(s)) if s.is_empty() => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'text' is required"
        ))),
        Some(Variant::String(_)) => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'text' must be between {MIN_TERM_CHARS} and {MAX_TERM_CHARS} characters"
        ))),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'text' is required"
        ))),
    }
}

#[async_trait]
impl SubActionRunner for AddBlockedTermRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Add Blocked Term"
    }

    fn summary(&self) -> &str {
        "Adds a word or phrase to the AutoMod blocked terms list."
    }

    fn search_text(&self) -> &str {
        "twitch automod blocked term add ban word phrase moderation"
    }

    fn icon_name(&self) -> &str {
        "slash"
    }

    fn default_config(&self) -> SubActionConfig {
        blocked_term_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        blocked_term_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_blocked_term_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let text_template = config
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let text = ctx.arg_stack.interpolate(text_template);

        if text.is_empty() {
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed("text is required".to_owned()),
                    index: ctx.index,
                },
                None,
            );
        }

        match post_blocked_term(&self.transport, &self.identity, KIND_ID, &text).await {
            Ok(term_id) => {
                let output_stack = ctx
                    .arg_stack
                    .clone()
                    .set("blocked_term.id".to_owned(), Variant::String(term_id));
                (
                    SubActionTelemetry {
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(output_stack),
                )
            }
            Err(outcome) => (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

// POST /helix/moderation/blocked_terms
// broadcaster_id = moderator_id = self (broadcaster is also moderator of their own channel)
// Returns data[0].id so the caller can chain into remove_blocked_term via %blocked_term.id%
async fn post_blocked_term(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    text: &str,
) -> Result<String, SubActionOutcome> {
    let user_id = identity
        .user_id()
        .await
        .map_err(|e| SubActionOutcome::Failed(e.to_string()))?;

    let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/blocked_terms")
        .query("broadcaster_id", user_id.clone())
        .query("moderator_id", user_id)
        .body(serde_json::json!({ "text": text }));

    let resp = transport
        .execute(request)
        .await
        .map_err(|e| SubActionOutcome::Failed(format!("{kind_id}: {e}")))?;

    resp["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|r| r["id"].as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| {
            SubActionOutcome::Failed(format!(
                "{kind_id}: unexpected empty response from add_blocked_term"
            ))
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn term_payload(id: &str) -> serde_json::Value {
        serde_json::json!({ "data": [{ "id": id, "text": "ignored" }] })
    }

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, AddBlockedTermRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = AddBlockedTermRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(text: &str) -> SubActionConfig {
        BTreeMap::from([("text".to_owned(), Variant::String(text.to_owned()))])
    }

    #[tokio::test]
    async fn execute_posts_term_with_self_query_and_pushes_id_output() {
        let (transport, runner) = runner_with(Ok(term_payload("bt42")));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&cfg("badword"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
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
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({ "text": "badword" })
        );

        assert_eq!(
            output.unwrap().get("blocked_term.id"),
            Some(&Variant::String("bt42".to_owned()))
        );
    }

    #[tokio::test]
    async fn text_template_is_interpolated_into_body() {
        let (transport, runner) = runner_with(Ok(term_payload("bt1")));
        let stack = ArgStack::new().set(
            "user.name".to_owned(),
            Variant::String("spammer".to_owned()),
        );

        let (telemetry, _) = runner.execute(&cfg("%user.name%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "text": "spammer" })
        );
    }

    #[tokio::test]
    async fn empty_interpolated_text_fails_without_helix_call() {
        let (transport, runner) = runner_with(Ok(term_payload("bt1")));
        // Global resolves to empty string → must short-circuit before POST.
        let stack = ArgStack::new().set("term".to_owned(), Variant::String(String::new()));
        let (telemetry, output) = runner.execute(&cfg("%term%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
        assert_eq!(
            transport.call_count(),
            0,
            "empty text must short-circuit before POST"
        );
    }

    #[tokio::test]
    async fn missing_data_id_maps_to_failed() {
        let (_transport, runner) = runner_with(Ok(serde_json::json!({ "data": [] })));

        let (telemetry, output) = runner
            .execute(&cfg("badword"), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(output.is_none());
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 400,
            body: "duplicate term".to_owned(),
        }));

        let (telemetry, output) = runner
            .execute(&cfg("badword"), &make_ctx(&ArgStack::new()))
            .await;

        assert!(output.is_none());
        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("400") && !msg.contains(TOKEN_SENTINEL)
        ));
    }

    // The length bound is enforced in CHARACTERS, not bytes (a byte-vs-char bug
    // was fixed here): a 500-char Cyrillic term is 1000 bytes and MUST validate.
    #[test]
    fn validate_config_enforces_2_to_500_char_bound() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("missing key", BTreeMap::new(), false),
            ("empty", cfg(""), false),
            ("one char (under min)", cfg("a"), false),
            ("two chars (min)", cfg("ab"), true),
            ("500 ascii (max)", cfg(&"a".repeat(500)), true),
            ("501 ascii (over max)", cfg(&"a".repeat(501)), false),
        ];
        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    // Regression: 500 Cyrillic chars = 1000 bytes. If production reverts to
    // s.len() (byte length) this validates as >500 and wrongly rejects.
    #[test]
    fn validate_config_counts_multibyte_chars_not_bytes() {
        let (_transport, runner) = runner_with(Ok(serde_json::Value::Null));
        assert!(
            runner.validate_config(&cfg(&"я".repeat(500))).is_ok(),
            "500 Cyrillic chars (1000 bytes) must validate — char count, not byte count"
        );
        assert!(
            runner.validate_config(&cfg(&"я".repeat(501))).is_err(),
            "501 chars must reject regardless of byte width"
        );
    }
}
