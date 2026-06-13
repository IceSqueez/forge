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
// `id` query param is the blocked-term UUID — NOT the blocked text itself.
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
