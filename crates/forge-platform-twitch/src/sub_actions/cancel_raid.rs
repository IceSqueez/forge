use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel.cancel_raid";

pub struct CancelRaidRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CancelRaidRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn cancel_raid(&self) -> SubActionOutcome {
        let self_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request =
            HelixRequest::new(HelixMethod::Delete, "/helix/raids").query("broadcaster_id", self_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for CancelRaidRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Cancel Raid"
    }

    fn summary(&self) -> &str {
        "Cancels a pending raid initiated from this channel."
    }

    fn search_text(&self) -> &str {
        "twitch raid cancel abort stop"
    }

    fn icon_name(&self) -> &str {
        "raid-cancel"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = self.cancel_raid().await;

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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };
    use forge_types::ArgStack;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, CancelRaidRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = CancelRaidRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_deletes_raid_scoped_to_self_broadcaster() {
        let (transport, runner) = runner_with(vec![Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            1,
            "cancel issues a single call, no login resolution"
        );
        let act = transport.last_request();
        assert_eq!(act.method, HelixMethod::Delete);
        assert_eq!(act.path, "/helix/raids");
        assert_eq!(
            act.query,
            vec![("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())],
            "only broadcaster_id=self may be sent"
        );
        assert_eq!(act.body, None, "DELETE carries no JSON body");
    }

    #[tokio::test]
    async fn cancel_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![Err(HelixError::Http {
            status: 404,
            body: "no raid in progress".to_owned(),
        })]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(transport.call_count(), 1);
        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(msg.contains("404"), "status must surface: {msg}");
        assert!(!msg.contains(TOKEN_SENTINEL), "token leaked: {msg}");
        assert!(!msg.contains("api.twitch.tv"), "URL leaked: {msg}");
    }
}
