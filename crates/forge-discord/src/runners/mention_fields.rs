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
    use forge_events::test_support::NullPublisher;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use forge_registry::{ConfigReason, RunContext, SubActionRunner};
    use forge_types::{ArgStack, EventId, SubActionOutcome, Variant};

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
                (config_keys::ALLOW_ROLE_PINGS, Variant::Bool(roles)),
                (config_keys::ALLOW_EVERYONE_PINGS, Variant::Bool(everyone)),
            ]);
            assert_eq!(
                read_mention_policy(&config, &ArgStack::new()),
                Ok(policy(roles, everyone))
            );
        }
    }

    #[test]
    fn mention_policy_for_a_step_saved_before_the_toggles_pings_roles_but_not_everyone() {
        assert_eq!(
            read_mention_policy(&step_config(&[]), &ArgStack::new()),
            Ok(policy(true, false))
        );
    }

    #[test]
    fn mention_policy_reads_toggle_text_and_templates() {
        let stack = ArgStack::new().set("ping".to_owned(), Variant::String("true".to_owned()));
        let config = step_config(&[
            (
                config_keys::ALLOW_ROLE_PINGS,
                Variant::String("false".to_owned()),
            ),
            (
                config_keys::ALLOW_EVERYONE_PINGS,
                Variant::String("%ping%".to_owned()),
            ),
        ]);

        assert_eq!(
            read_mention_policy(&config, &stack),
            Ok(policy(false, true))
        );
    }

    #[test]
    fn mention_policy_rejects_a_toggle_that_is_not_a_boolean() {
        for (key, value) in [
            (config_keys::ALLOW_ROLE_PINGS, Variant::Int(0)),
            (
                config_keys::ALLOW_EVERYONE_PINGS,
                Variant::String("yes".to_owned()),
            ),
        ] {
            let problem =
                read_mention_policy(&step_config(&[(key, value.clone())]), &ArgStack::new())
                    .unwrap_err();

            assert_eq!(problem.field.as_deref(), Some(key), "{value:?}");
            assert!(
                matches!(problem.reason, ConfigReason::InvalidChoice(_)),
                "{value:?}"
            );
        }
    }

    #[tokio::test]
    async fn post_and_edit_runners_fail_on_a_non_boolean_toggle_without_posting() {
        let config = step_config(&[(config_keys::ALLOW_ROLE_PINGS, Variant::Int(1))]);
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
        let publisher = NullPublisher;
        for runner in &runners {
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
            let (telemetry, _) = runner.execute(&config, &ctx).await;
            assert!(
                matches!(
                    &telemetry.outcome,
                    SubActionOutcome::Failed(forge_types::UserText::Message(message))
                        if message.arg("field")
                            == Some(&forge_types::MessageArg::StepFields {
                                kind: runner.id().to_owned(),
                                keys: vec![config_keys::ALLOW_ROLE_PINGS.to_owned()],
                            })
                ),
                "{}: {:?}",
                runner.id(),
                telemetry.outcome
            );
        }
        assert!(sink.seen.lock().unwrap().is_empty());
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

    #[tokio::test]
    async fn post_and_edit_runners_hand_the_step_toggles_to_the_sink() {
        let config = step_config(&[
            (config_keys::ALLOW_ROLE_PINGS, Variant::Bool(false)),
            (config_keys::ALLOW_EVERYONE_PINGS, Variant::Bool(true)),
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
        let publisher = NullPublisher;
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
