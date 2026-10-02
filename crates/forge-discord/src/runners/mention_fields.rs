use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, SubActionConfigExt};
use forge_types::Variant;

use crate::mention::MentionPolicy;

pub(super) const ALLOW_ROLE_PINGS_KEY: &str = "allow_role_pings";
pub(super) const ALLOW_EVERYONE_PINGS_KEY: &str = "allow_everyone_pings";

pub(super) fn mention_default_entries() -> [(String, Variant); 2] {
    let defaults = MentionPolicy::default();
    [
        (
            ALLOW_ROLE_PINGS_KEY.to_owned(),
            Variant::Bool(defaults.allow_roles),
        ),
        (
            ALLOW_EVERYONE_PINGS_KEY.to_owned(),
            Variant::Bool(defaults.allow_everyone),
        ),
    ]
}

pub(super) fn mention_form_fields() -> [FormField; 2] {
    [
        FormField::Toggle {
            key: ALLOW_ROLE_PINGS_KEY,
            label: "Allow role pings",
        },
        FormField::Toggle {
            key: ALLOW_EVERYONE_PINGS_KEY,
            label: "Allow @everyone / @here",
        },
    ]
}

pub(super) fn mention_policy(config: &SubActionConfig) -> MentionPolicy {
    let defaults = MentionPolicy::default();
    MentionPolicy {
        allow_roles: config
            .bool(ALLOW_ROLE_PINGS_KEY)
            .unwrap_or(defaults.allow_roles),
        allow_everyone: config
            .bool(ALLOW_EVERYONE_PINGS_KEY)
            .unwrap_or(defaults.allow_everyone),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
