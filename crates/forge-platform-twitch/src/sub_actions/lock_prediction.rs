use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
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
        _ => Err(RegistryError::InvalidConfig(format!(
            "{kind_id}: 'prediction_id' is required"
        ))),
    }
}

/// Status values are uppercase: "LOCKED", "RESOLVED", "CANCELED" (American single-L).
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

    let prediction_id_template = config.str("prediction_id").unwrap_or_default();
    let prediction_id = ctx.arg_stack.interpolate(prediction_id_template);

    if prediction_id.is_empty() {
        return (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: kind_id.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome: SubActionOutcome::Failed("prediction_id is required".to_owned()),
                index: ctx.index,
            },
            None,
        );
    }

    let winning_outcome_id_owned = winning_outcome_id_key
        .and_then(|key| config.str(key).map(|t| ctx.arg_stack.interpolate(t)));

    if let Some(ref wid) = winning_outcome_id_owned
        && wid.is_empty()
    {
        return (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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
            args_in: ::std::collections::BTreeMap::new(),
            produced: ::std::collections::BTreeMap::new(),
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
        SubActionCategory::PollsPredictions
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_types::SubActionOutcome;

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn lock_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, LockPredictionRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = LockPredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    fn prediction_stack() -> ArgStack {
        ArgStack::new().set(
            "prediction.id".to_owned(),
            Variant::String("pr42".to_owned()),
        )
    }

    #[tokio::test]
    async fn lock_patches_predictions_with_broadcaster_query_and_locked_body() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));

        let (telemetry, out) = runner
            .execute(&prediction_default_config(), &make_ctx(&prediction_stack()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "prediction runners never push an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/predictions");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "broadcaster_id must be self in the query, not the body: {:?}",
            request.query
        );
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "id": "pr42", "status": "LOCKED" })),
            "lock body must be id+LOCKED with no winning_outcome_id key"
        );
    }

    #[tokio::test]
    async fn prediction_id_interpolates_from_template() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set(
            "prediction.id".to_owned(),
            Variant::String("xyz9".to_owned()),
        );

        let _ = runner
            .execute(&prediction_default_config(), &make_ctx(&stack))
            .await;

        let body = transport.request(0).body.unwrap();
        assert_eq!(
            body.get("id").and_then(|v| v.as_str()),
            Some("xyz9"),
            "id must interpolate from %prediction.id%, not be sent verbatim"
        );
    }

    #[tokio::test]
    async fn empty_prediction_id_fails_without_helix_call() {
        let (transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([("prediction_id".to_owned(), Variant::String(String::new()))]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&ArgStack::new())).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty prediction_id must short-circuit before PATCH"
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_missing_prediction_id() {
        let (_transport, runner) = lock_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present", prediction_default_config(), true),
            (
                "empty prediction_id",
                BTreeMap::from([("prediction_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            ("missing prediction_id", BTreeMap::new(), false),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = lock_runner_with(Err(HelixError::Http {
            status: 401,
            body: "unauthorized".to_owned(),
        }));

        let (telemetry, _) = runner
            .execute(&prediction_default_config(), &make_ctx(&prediction_stack()))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
