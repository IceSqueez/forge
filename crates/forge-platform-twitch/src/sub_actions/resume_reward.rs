use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{ConfigSpec, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::enable_reward::{REWARD_CONFIG, execute_bool_runner};
use crate::account::identity::SelfIdentity;
use crate::custom_rewards::RewardBody;
use crate::helix::HelixTransport;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_RESUME_REWARD;

pub(crate) struct ResumeRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ResumeRewardRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ResumeRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Resume Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Resumes a paused custom channel point reward so redemptions are fulfilled normally."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward resume unpause redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(REWARD_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_bool_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            RewardBody::new().paused(false),
            config,
            ctx,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::helix::{HelixMethod, HelixTransport};
    use crate::sub_actions::test_support::{MockTransport, SELF_USER_ID, make_ctx};
    use forge_types::{SubActionOutcome, Variant};

    #[tokio::test]
    async fn resume_patches_is_paused_false() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = ResumeRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
        );
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw3".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.query.contains(&("id".to_owned(), "rw3".to_owned())));
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "is_paused": false }))
        );
    }
}
