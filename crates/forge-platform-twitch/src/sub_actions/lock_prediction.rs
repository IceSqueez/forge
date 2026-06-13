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

const KIND_ID: &str = "twitch.prediction.lock";

pub struct LockPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl LockPredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

pub(crate) fn prediction_default_config() -> SubActionConfig {
    BTreeMap::from([(
        "prediction_id".to_owned(),
        Variant::String("%prediction.id%".to_owned()),
    )])
}

pub(crate) fn prediction_config_fields() -> Vec<FormField> {
    vec![FormField::Text {
        key: "prediction_id",
        label: "Prediction ID",
        placeholder: "%prediction.id%",
    }]
}

pub(crate) fn validate_prediction_config(
    kind_id: &str,
    config: &SubActionConfig,
) -> Result<(), RegistryError> {
    match config.get("prediction_id") {
        Some(Variant::String(s)) if !s.is_empty() => Ok(()),
        _ => Err(RegistryError::UnknownKindId(format!(
            "{kind_id}: 'prediction_id' is required"
        ))),
    }
}

/// PATCH /helix/predictions — broadcaster_id is a query param; id and status go in the body.
/// Status values are uppercase: "LOCKED", "RESOLVED", "CANCELED" (American single-L).
/// winning_outcome_id is only sent for RESOLVED; lock and cancel pass None.
/// Requires channel:manage:predictions scope.
pub(crate) async fn patch_prediction_status(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    prediction_id: &str,
    status: &str,
    winning_outcome_id: Option<&str>,
) -> SubActionOutcome {
    let user_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };

    let mut body = serde_json::Map::new();
    body.insert("id".to_owned(), prediction_id.into());
    body.insert("status".to_owned(), status.into());
    if let Some(wid) = winning_outcome_id {
        body.insert("winning_outcome_id".to_owned(), wid.into());
    }

    let request = HelixRequest::new(HelixMethod::Patch, "/helix/predictions")
        .query("broadcaster_id", user_id)
        .body(serde_json::Value::Object(body));

    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(format!("{kind_id}: {e}")),
    }
}

pub(crate) async fn execute_prediction_runner(
    transport: &Arc<dyn HelixTransport>,
    identity: &Arc<SelfIdentity>,
    kind_id: &str,
    status: &str,
    winning_outcome_id_key: Option<&str>,
    config: &SubActionConfig,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let started_at = OffsetDateTime::now_utc();
    let start = Instant::now();

    let prediction_id_template = config
        .get("prediction_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let prediction_id = ctx.arg_stack.interpolate(prediction_id_template);

    if prediction_id.is_empty() {
        return (
            SubActionTelemetry {
                kind: kind_id.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome: SubActionOutcome::Failed("prediction_id is required".to_owned()),
                index: ctx.index,
            },
            None,
        );
    }

    // winning_outcome_id is only present for the resolve runner (status RESOLVED).
    let winning_outcome_id_owned = winning_outcome_id_key.and_then(|key| {
        config
            .get(key)
            .and_then(|v| v.as_str())
            .map(|t| ctx.arg_stack.interpolate(t))
    });

    if let Some(ref wid) = winning_outcome_id_owned
        && wid.is_empty()
    {
        return (
            SubActionTelemetry {
                kind: kind_id.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome: SubActionOutcome::Failed("winning_outcome_id is required".to_owned()),
                index: ctx.index,
            },
            None,
        );
    }

    let outcome = patch_prediction_status(
        transport,
        identity,
        kind_id,
        &prediction_id,
        status,
        winning_outcome_id_owned.as_deref(),
    )
    .await;

    (
        SubActionTelemetry {
            kind: kind_id.to_owned(),
            started_at,
            duration_ms: start.elapsed().as_millis() as u64,
            outcome,
            index: ctx.index,
        },
        None,
    )
}

#[async_trait]
impl SubActionRunner for LockPredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Lock Prediction"
    }

    fn summary(&self) -> &str {
        "Locks an active prediction, preventing new votes. Viewers can no longer change their outcome."
    }

    fn search_text(&self) -> &str {
        "twitch prediction lock stop votes"
    }

    fn icon_name(&self) -> &str {
        "lock"
    }

    fn default_config(&self) -> SubActionConfig {
        prediction_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        prediction_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_prediction_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_prediction_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "LOCKED",
            None,
            config,
            ctx,
        )
        .await
    }
}
