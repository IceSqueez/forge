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
