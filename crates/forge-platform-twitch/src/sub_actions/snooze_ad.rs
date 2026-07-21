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

const KIND_ID: &str = "twitch.channel.snooze_ad";

pub struct SnoozeAdRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SnoozeAdRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn snooze(&self) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/ads/schedule/snooze")
            .query("broadcaster_id", user_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for SnoozeAdRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Snooze Next Ad"
    }

    fn summary(&self) -> &str {
        "Delays the next scheduled ad break by 5 minutes."
    }

    fn search_text(&self) -> &str {
        "twitch ad snooze delay schedule next commercial"
    }

    fn icon_name(&self) -> &str {
        "clock-pause"
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

        let outcome = self.snooze().await;

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
    use forge_types::ArgStack;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, SnoozeAdRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = SnoozeAdRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_posts_self_broadcaster_in_query_with_no_body() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/channels/ads/schedule/snooze");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.is_none(), "snooze must send no JSON body");
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(HelixError::Http {
            status: 429,
            body: "no snoozes remaining".to_owned(),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("429") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
