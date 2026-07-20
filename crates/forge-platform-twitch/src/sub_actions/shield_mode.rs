use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

async fn set_shield(
    transport: &dyn HelixTransport,
    identity: &SelfIdentity,
    is_active: bool,
) -> SubActionOutcome {
    let self_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };
    let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/shield_mode")
        .query("broadcaster_id", self_id.clone())
        .query("moderator_id", self_id)
        .body(serde_json::json!({ "is_active": is_active }));
    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    }
}

// ─── Shield Mode On ──────────────────────────────────────────────────────────

const ON_KIND_ID: &str = "twitch.moderation.shield_mode_on";

pub struct ShieldModeOnRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ShieldModeOnRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ShieldModeOnRunner {
    fn id(&self) -> &str {
        ON_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Enable Shield Mode"
    }

    fn summary(&self) -> &str {
        "Activates Shield Mode on the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation shield mode on enable activate protect"
    }

    fn icon_name(&self) -> &str {
        "shield"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<forge_registry::FormField> {
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

        let outcome = set_shield(self.transport.as_ref(), &self.identity, true).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: ON_KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

// ─── Shield Mode Off ─────────────────────────────────────────────────────────

const OFF_KIND_ID: &str = "twitch.moderation.shield_mode_off";

pub struct ShieldModeOffRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ShieldModeOffRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ShieldModeOffRunner {
    fn id(&self) -> &str {
        OFF_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Disable Shield Mode"
    }

    fn summary(&self) -> &str {
        "Deactivates Shield Mode on the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation shield mode off disable deactivate"
    }

    fn icon_name(&self) -> &str {
        "shield-off"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<forge_registry::FormField> {
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

        let outcome = set_shield(self.transport.as_ref(), &self.identity, false).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: OFF_KIND_ID.to_owned(),
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
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome};

    use super::*;
    use crate::helix::HelixMethod;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, SELF_USER_ID, make_ctx};

    // ── table-driven: on/off each issue exactly one POST to shield_mode ───────

    #[tokio::test]
    async fn shield_mode_on_and_off_each_post_with_correct_is_active_flag() {
        struct Case {
            label: &'static str,
            runner: Box<dyn SubActionRunner>,
            transport: Arc<MockTransport>,
            expected_is_active: bool,
        }

        fn make_transport() -> Arc<MockTransport> {
            Arc::new(MockTransport::returning_sequence(vec![Ok(
                serde_json::Value::Null,
            )]))
        }

        fn identity() -> Arc<SelfIdentity> {
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity())))
        }

        let t_on = make_transport();
        let t_off = make_transport();

        let cases: Vec<Case> = vec![
            Case {
                label: "shield_mode_on",
                runner: Box::new(ShieldModeOnRunner::new(
                    Arc::clone(&t_on) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_on,
                expected_is_active: true,
            },
            Case {
                label: "shield_mode_off",
                runner: Box::new(ShieldModeOffRunner::new(
                    Arc::clone(&t_off) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_off,
                expected_is_active: false,
            },
        ];

        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let empty_config = BTreeMap::new();

        for case in cases {
            let (telemetry, _) = case.runner.execute(&empty_config, &ctx).await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Success,
                "{}: expected Success",
                case.label
            );
            assert_eq!(
                case.transport.call_count(),
                1,
                "{}: shield_mode must issue exactly one Helix call (no resolve step)",
                case.label
            );
            let req = case.transport.last_request();
            assert_eq!(
                req.method,
                HelixMethod::Post,
                "{}: must POST to shield_mode",
                case.label
            );
            assert_eq!(
                req.path, "/helix/moderation/shield_mode",
                "{}: wrong endpoint",
                case.label
            );
            assert!(
                req.query
                    .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
                "{}: broadcaster_id must equal self id",
                case.label
            );
            assert!(
                req.query
                    .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned())),
                "{}: moderator_id must equal self id",
                case.label
            );
            assert_eq!(
                req.body,
                Some(serde_json::json!({ "is_active": case.expected_is_active })),
                "{}: body must carry the correct is_active value",
                case.label
            );
        }
    }
}
