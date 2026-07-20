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

const KIND_ID: &str = "twitch.channel.update_tags";
const MAX_TAGS: usize = 10;
const MAX_TAG_CHARS: usize = 25;

pub struct UpdateTagsRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl UpdateTagsRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, tags: Vec<String>) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        // PATCH /helix/channels returns 204 No Content on success; Value::Null from transport.
        // An empty tags array clears all custom tags.
        let request = HelixRequest::new(HelixMethod::Patch, "/helix/channels")
            .query("broadcaster_id", user_id)
            .body(serde_json::json!({ "tags": tags }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

/// Splits the textarea value on newlines and commas, trimming each entry and
/// dropping blanks. Returns `Err` if a tag exceeds 25 chars or there are over 10.
fn parse_tags(raw: &str) -> Result<Vec<String>, String> {
    let tags: Vec<String> = raw
        .split(['\n', ','])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    for tag in &tags {
        if tag.chars().count() > MAX_TAG_CHARS {
            return Err(format!(
                "tag '{tag}' exceeds {MAX_TAG_CHARS}-character limit"
            ));
        }
    }
    if tags.len() > MAX_TAGS {
        return Err(format!("too many tags: max {MAX_TAGS}, got {}", tags.len()));
    }
    Ok(tags)
}

#[async_trait]
impl SubActionRunner for UpdateTagsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Update Stream Tags"
    }

    fn summary(&self) -> &str {
        "Replaces the broadcaster's stream tags (max 10, each ≤25 chars). Empty clears all tags."
    }

    fn search_text(&self) -> &str {
        "twitch channel tags update broadcast stream"
    }

    fn icon_name(&self) -> &str {
        "tag"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("tags".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "tags",
            label: "Tags (one per line or comma-separated)",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let raw = match config.get("tags") {
            Some(Variant::String(s)) => s.as_str(),
            None => "",
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'tags' must be a string"
                )));
            }
        };
        parse_tags(raw).map_err(|msg| RegistryError::UnknownKindId(format!("{KIND_ID}: {msg}")))?;
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw = ctx.arg_stack.interpolate(template);

        let outcome = match parse_tags(&raw) {
            Ok(tags) => self.apply(tags).await,
            Err(msg) => SubActionOutcome::Failed(msg),
        };

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
    ) -> (Arc<MockTransport>, UpdateTagsRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = UpdateTagsRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn cfg(tags: &str) -> SubActionConfig {
        BTreeMap::from([("tags".to_owned(), Variant::String(tags.to_owned()))])
    }

    #[test]
    fn parse_tags_splits_trims_and_drops_blanks() {
        // Mixed newline + comma separators, surrounding whitespace, and blank
        // entries between separators must collapse to clean tags in order.
        assert_eq!(
            parse_tags(" Speedrun ,\n English\n\n, ,Chill ").unwrap(),
            vec!["Speedrun", "English", "Chill"]
        );
    }

    #[test]
    fn parse_tags_empty_input_yields_empty_vec() {
        assert_eq!(parse_tags("").unwrap(), Vec::<String>::new());
        assert_eq!(parse_tags("  ,\n , ").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn parse_tags_enforces_per_tag_and_count_limits() {
        let cases: Vec<(&str, String, bool)> = vec![
            ("tag at 25 chars", "a".repeat(25), true),
            ("tag over 25 chars", "a".repeat(26), false),
            (
                "exactly 10 tags",
                (1..=10)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                true,
            ),
            (
                "11 tags",
                (1..=11)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                false,
            ),
        ];
        for (label, raw, expect_ok) in cases {
            assert_eq!(parse_tags(&raw).is_ok(), expect_ok, "case: {label}");
        }
    }

    #[tokio::test]
    async fn execute_sends_parsed_tags_array() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner
            .execute(&cfg("Speedrun\nEnglish, Chill"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channels");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert_eq!(
            request.body.unwrap(),
            serde_json::json!({ "tags": ["Speedrun", "English", "Chill"] })
        );
    }

    #[tokio::test]
    async fn empty_input_clears_tags_with_empty_array() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg(""), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        // Empty input is a clear-all, NOT a skip: a PATCH with an empty array fires.
        assert_eq!(transport.call_count(), 1);
        assert_eq!(
            transport.request(0).body.unwrap(),
            serde_json::json!({ "tags": [] })
        );
    }

    #[tokio::test]
    async fn oversize_tag_and_too_many_tags_fail_without_helix_call() {
        for (label, raw) in [
            ("tag over 25 chars", "a".repeat(26)),
            (
                "11 tags",
                (1..=11)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ] {
            let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
            let stack = ArgStack::new();

            let (telemetry, _) = runner.execute(&cfg(&raw), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "case: {label}"
            );
            assert_eq!(transport.call_count(), 0, "case: {label} must skip Helix");
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 403,
            body: "missing scope".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&cfg("Chill"), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("403") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
