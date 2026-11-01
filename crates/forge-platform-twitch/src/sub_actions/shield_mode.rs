use crate::step_failure;
use crate::vocabulary::sub_action_ids;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

async fn set_shield(
    transport: &dyn HelixTransport,
    identity: &SelfIdentity,
    is_active: bool,
) -> SubActionOutcome {
    let self_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return step_failure::failed(&e),
    };
    let request = HelixRequest::new(HelixMethod::Put, "/helix/moderation/shield_mode")
        .query("broadcaster_id", self_id.clone())
        .query("moderator_id", self_id)
        .body(serde_json::json!({ "is_active": is_active }));
    step_failure::outcome(&transport.execute(request).await)
}

struct ShieldSwitch {
    id: &'static str,
    is_active: bool,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
}

const SHIELD_ON: ShieldSwitch = ShieldSwitch {
    id: sub_action_ids::MODERATION_SHIELD_MODE_ON,
    is_active: true,
    label: "Enable Shield Mode",
    summary: "Activates Shield Mode on the broadcaster's channel.",
    search_text: "twitch moderation shield mode on enable activate protect",
    icon_name: "shield",
};

const SHIELD_OFF: ShieldSwitch = ShieldSwitch {
    id: sub_action_ids::MODERATION_SHIELD_MODE_OFF,
    is_active: false,
    label: "Disable Shield Mode",
    summary: "Deactivates Shield Mode on the broadcaster's channel.",
    search_text: "twitch moderation shield mode off disable deactivate",
    icon_name: "shield-off",
};

pub(crate) struct ShieldModeRunner {
    switch: ShieldSwitch,
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ShieldModeRunner {
    pub(crate) fn on(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            switch: SHIELD_ON,
            transport,
            identity,
        }
    }

    pub(crate) fn off(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            switch: SHIELD_OFF,
            transport,
            identity,
        }
    }
}

impl StepCatalogEntry for ShieldModeRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }
    fn label(&self) -> &str {
        self.switch.label
    }
    fn summary(&self) -> &str {
        self.switch.summary
    }
    fn search_text(&self) -> &str {
        self.switch.search_text
    }
    fn icon_name(&self) -> &str {
        self.switch.icon_name
    }
}

#[async_trait]
impl SubActionRunner for ShieldModeRunner {
    fn id(&self) -> &str {
        self.switch.id
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.switch.id);
        let outcome = set_shield(
            self.transport.as_ref(),
            &self.identity,
            self.switch.is_active,
        )
        .await;
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome};

    use super::*;
    use crate::helix::HelixMethod;
    use crate::sub_actions::test_support::{MockTransport, SELF_USER_ID, make_ctx};

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
            crate::sub_actions::test_support::self_identity()
        }

        let t_on = make_transport();
        let t_off = make_transport();

        let cases: Vec<Case> = vec![
            Case {
                label: "shield_mode_on",
                runner: Box::new(ShieldModeRunner::on(
                    Arc::clone(&t_on) as Arc<dyn HelixTransport>,
                    identity(),
                )),
                transport: t_on,
                expected_is_active: true,
            },
            Case {
                label: "shield_mode_off",
                runner: Box::new(ShieldModeRunner::off(
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
                HelixMethod::Put,
                "{}: must PUT to shield_mode",
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
