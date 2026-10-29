use forge_registry::runner::SubActionConfig;
use forge_registry::{ConfigProblem, ToggleField, TypedField};
use forge_types::ArgStack;

use crate::mention::MentionPolicy;
use crate::vocabulary::config_keys;

pub(super) const ALLOW_ROLE_PINGS: ToggleField = ToggleField {
    key: config_keys::ALLOW_ROLE_PINGS,
    label: "Allow role pings",
    default: MentionPolicy::DEFAULT.allow_roles,
};

pub(super) const ALLOW_EVERYONE_PINGS: ToggleField = ToggleField {
    key: config_keys::ALLOW_EVERYONE_PINGS,
    label: "Allow @everyone / @here",
    default: MentionPolicy::DEFAULT.allow_everyone,
};

pub(super) fn read_mention_policy(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<MentionPolicy, ConfigProblem> {
    Ok(MentionPolicy {
        allow_roles: ALLOW_ROLE_PINGS.read(config, args)?,
        allow_everyone: ALLOW_EVERYONE_PINGS.read(config, args)?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_events::{Event, EventPublisher};
    use forge_registry::{RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionOutcome};

    use super::*;
    use crate::embed::DiscordEmbed;
    use crate::error::DiscordError;
    use crate::runners::{EditMessageRunner, PostTextRunner};
    use crate::sink::DiscordSink;

    fn policy(allow_roles: bool, allow_everyone: bool) -> MentionPolicy {
        MentionPolicy {
            allow_roles,
            allow_everyone,
        }
    }

    fn step_config(extra: &[(&str, Variant)]) -> SubActionConfig {
        let mut config = BTreeMap::from([
            (
                "webhook_name".to_owned(),
                Variant::String("alerts".to_owned()),
            ),
            ("message_id".to_owned(), Variant::String("m1".to_owned())),
            (
                "content".to_owned(),
                Variant::String("<@&42> live".to_owned()),
            ),
        ]);
        for (key, value) in extra {
            config.insert((*key).to_owned(), value.clone());
        }
        config
    }

    #[test]
    fn mention_policy_reads_each_toggle_combination() {
        for (roles, everyone) in [(true, false), (true, true), (false, false), (false, true)] {
            let config = step_config(&[
                (ALLOW_ROLE_PINGS_KEY, Variant::Bool(roles)),
                (ALLOW_EVERYONE_PINGS_KEY, Variant::Bool(everyone)),
            ]);
            assert_eq!(mention_policy(&config), policy(roles, everyone));
        }
    }

    #[test]
    fn mention_policy_for_a_step_saved_before_the_toggles_pings_roles_but_not_everyone() {
        assert_eq!(mention_policy(&step_config(&[])), policy(true, false));
    }

    #[test]
    fn mention_policy_treats_a_non_boolean_toggle_as_absent() {
        let config = step_config(&[
            (ALLOW_ROLE_PINGS_KEY, Variant::Int(0)),
            (ALLOW_EVERYONE_PINGS_KEY, Variant::String("true".to_owned())),
        ]);
        assert_eq!(mention_policy(&config), policy(true, false));
    }

    #[derive(Default)]
    struct PolicySink {
        seen: Mutex<Vec<MentionPolicy>>,
    }

    #[async_trait]
    impl DiscordSink for PolicySink {
        async fn post_text(
            &self,
            _: &str,
            _: &str,
            mentions: MentionPolicy,
        ) -> Result<String, DiscordError> {
            self.seen.lock().unwrap().push(mentions);
            Ok("m1".to_owned())
        }
        async fn post_embed(&self, _: &str, _: DiscordEmbed) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn edit_message(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
            _: Option<DiscordEmbed>,
            mentions: MentionPolicy,
        ) -> Result<(), DiscordError> {
            self.seen.lock().unwrap().push(mentions);
            Ok(())
        }
        async fn send_file(
            &self,
            _: &str,
            _: Option<&str>,
            _: &str,
            _: &[u8],
            _: MentionPolicy,
        ) -> Result<String, DiscordError> {
            Ok(String::new())
        }
        async fn delete_message(&self, _: &str, _: &str) -> Result<(), DiscordError> {
            Ok(())
        }
    }

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    #[tokio::test]
    async fn post_and_edit_runners_hand_the_step_toggles_to_the_sink() {
        let config = step_config(&[
            (ALLOW_ROLE_PINGS_KEY, Variant::Bool(false)),
            (ALLOW_EVERYONE_PINGS_KEY, Variant::Bool(true)),
        ]);
        let sink = Arc::new(PolicySink::default());
        let runners: [Box<dyn SubActionRunner>; 2] = [
            Box::new(PostTextRunner::new(
                Arc::clone(&sink) as Arc<dyn DiscordSink>
            )),
            Box::new(EditMessageRunner::new(
                Arc::clone(&sink) as Arc<dyn DiscordSink>
            )),
        ];
        let stack = ArgStack::new();
        let publisher = NoopPublisher;
        for runner in &runners {
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
            let (telemetry, _) = runner.execute(&config, &ctx).await;
            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Success),
                "{}",
                runner.id()
            );
        }
        assert_eq!(
            *sink.seen.lock().unwrap(),
            [policy(false, true), policy(false, true)]
        );
    }
}
