mod accept_redemption;
mod ban_reason;
mod ban_user;
mod create_reward;
mod delete_message;
mod delete_reward;
mod display_name;
mod field_kinds;
mod fields;
mod lookup_category;
mod lookup_stream_stats;
mod lookup_user;
mod reject_redemption;
mod send_message;
mod step_failure;
#[cfg(test)]
mod test_support;
mod timeout_user;
mod unban_user;
mod update_info;
mod update_reward;

use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_registry::{RegistryError, SubActionRegistry};
use futures::future::BoxFuture;

pub use accept_redemption::AcceptRedemptionRunner;
pub use ban_user::BanUserRunner;
pub use create_reward::CreateRewardRunner;
pub use delete_message::DeleteMessageRunner;
pub use delete_reward::DeleteRewardRunner;
pub use lookup_category::LookupCategoryRunner;
pub use lookup_stream_stats::LookupStreamStatsRunner;
pub use lookup_user::LookupUserRunner;
pub use reject_redemption::RejectRedemptionRunner;
pub use send_message::SendMessageRunner;
pub use timeout_user::TimeoutUserRunner;
pub use unban_user::UnbanUserRunner;
pub use update_info::UpdateInfoRunner;
pub use update_reward::UpdateRewardRunner;

use crate::api::categories::KickCategories;
use crate::api::channel::KickChannel;
use crate::api::chat_messages::KickSendChat;
use crate::api::moderation::KickModeration;
use crate::api::rewards::KickRewards;

pub struct KickSubActionDeps {
    pub client: Arc<KickSendChat>,
    pub token_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    pub broadcaster_id_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync>,
    pub moderation: Arc<KickModeration>,
    pub channel: Arc<KickChannel>,
    pub rewards: Arc<KickRewards>,
    pub categories: Arc<KickCategories>,
}

pub fn register_kick_sub_actions(
    reg: &mut SubActionRegistry,
    deps: KickSubActionDeps,
) -> Result<(), RegistryError> {
    let mut reg = reg.owned_by(crate::KICK_INTEGRATION.id);
    let KickSubActionDeps {
        client,
        token_source,
        broadcaster_id_source,
        moderation,
        channel,
        rewards,
        categories,
    } = deps;
    reg.register(Box::new(SendMessageRunner::new(
        Arc::clone(&client),
        Arc::clone(&token_source),
        Arc::clone(&broadcaster_id_source),
    )))?;
    reg.register(Box::new(DeleteMessageRunner::new(
        Arc::clone(&client),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(BanUserRunner::new(
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        Arc::clone(&broadcaster_id_source),
    )))?;
    reg.register(Box::new(TimeoutUserRunner::new(
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        Arc::clone(&broadcaster_id_source),
    )))?;
    reg.register(Box::new(UnbanUserRunner::new(
        Arc::clone(&moderation),
        Arc::clone(&token_source),
        broadcaster_id_source,
    )))?;
    reg.register(Box::new(UpdateInfoRunner::new(
        Arc::clone(&channel),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(CreateRewardRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(UpdateRewardRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(DeleteRewardRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(AcceptRedemptionRunner::new(
        Arc::clone(&rewards),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(RejectRedemptionRunner::new(
        rewards,
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(LookupUserRunner::new(
        Arc::clone(&channel),
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(LookupStreamStatsRunner::new(
        channel,
        Arc::clone(&token_source),
    )))?;
    reg.register(Box::new(LookupCategoryRunner::new(
        categories,
        token_source,
    )))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use forge_registry::{ConfigProblem, ConfigReason, ValueBounds};
    use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, Variant};

    use super::*;
    use crate::endpoints_test_support::unreachable_api;

    fn offline_registry() -> SubActionRegistry {
        let mut registry = SubActionRegistry::new();
        register_kick_sub_actions(
            &mut registry,
            KickSubActionDeps {
                client: Arc::new(KickSendChat::new(unreachable_api())),
                token_source: test_support::token_source(),
                broadcaster_id_source: Arc::new(|| Box::pin(async { Ok(1) })),
                moderation: Arc::new(KickModeration::new(
                    unreachable_api(),
                    crate::ban_ledger_test_support::MemoryBanLedger::shared(),
                )),
                channel: Arc::new(KickChannel::new(unreachable_api())),
                rewards: Arc::new(KickRewards::new(unreachable_api())),
                categories: Arc::new(KickCategories::new(unreachable_api())),
            },
        )
        .unwrap();
        registry
    }

    const STORED_DEFAULT_CONFIGS: [(&str, &str); 14] = [
        (
            "kick.channel.update_info",
            r#"{"category_id":{"type":"string","value":""},"tags":{"type":"string","value":""},"title":{"type":"string","value":""}}"#,
        ),
        (
            "kick.chat.delete_message",
            r#"{"message_id":{"type":"string","value":"%message_id%"}}"#,
        ),
        (
            "kick.chat.send_message",
            r#"{"as_bot":{"type":"bool","value":false},"message":{"type":"string","value":""}}"#,
        ),
        (
            "kick.lookup.category",
            r#"{"query":{"type":"string","value":""}}"#,
        ),
        ("kick.lookup.stream_stats", r#"{}"#),
        (
            "kick.lookup.user",
            r#"{"slug":{"type":"string","value":""}}"#,
        ),
        (
            "kick.moderation.ban",
            r#"{"reason":{"type":"string","value":""},"user_id":{"type":"string","value":""}}"#,
        ),
        (
            "kick.moderation.timeout",
            r#"{"duration_minutes":{"type":"int","value":10},"reason":{"type":"string","value":""},"user_id":{"type":"string","value":""}}"#,
        ),
        (
            "kick.moderation.unban",
            r#"{"user_id":{"type":"string","value":""}}"#,
        ),
        (
            "kick.reward.create",
            r#"{"cost":{"type":"string","value":""},"description":{"type":"string","value":""},"title":{"type":"string","value":""}}"#,
        ),
        (
            "kick.reward.delete",
            r#"{"reward_id":{"type":"string","value":""}}"#,
        ),
        (
            "kick.reward.redemption_accept",
            r#"{"redemption_ids":{"type":"string","value":"%redemption_id%"}}"#,
        ),
        (
            "kick.reward.redemption_reject",
            r#"{"redemption_ids":{"type":"string","value":"%redemption_id%"}}"#,
        ),
        (
            "kick.reward.update",
            r#"{"cost":{"type":"string","value":""},"description":{"type":"string","value":""},"reward_id":{"type":"string","value":""},"title":{"type":"string","value":""}}"#,
        ),
    ];

    #[test]
    fn every_runner_seeds_the_same_stored_default_config_as_before_declared_fields() {
        let registry = offline_registry();
        let registered: BTreeSet<&str> = registry.all().map(|runner| runner.id()).collect();
        let pinned: BTreeSet<&str> = STORED_DEFAULT_CONFIGS.iter().map(|(id, _)| *id).collect();
        assert_eq!(registered, pinned);

        for (id, stored) in STORED_DEFAULT_CONFIGS {
            let expected: serde_json::Value = serde_json::from_str(stored).unwrap();

            assert_eq!(
                serde_json::to_value(registry.get(id).unwrap().default_config()).unwrap(),
                expected,
                "{id} seeds a different default config"
            );
        }
    }

    type Entries<'a> = Vec<(&'a str, &'a str)>;

    fn config_of(entries: &[(&str, &str)]) -> SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect()
    }

    #[test]
    fn literal_values_a_field_cannot_hold_are_rejected_on_their_key_at_edit_time() {
        let not_a_number = ConfigReason::NotANumber;
        let cases: Vec<(&str, Entries<'_>, &str, ConfigReason)> = vec![
            (
                "kick.moderation.ban",
                vec![("user_id", "alice")],
                "user_id",
                not_a_number.clone(),
            ),
            (
                "kick.moderation.timeout",
                vec![("user_id", "12.5")],
                "user_id",
                not_a_number.clone(),
            ),
            (
                "kick.moderation.unban",
                vec![("user_id", "-4")],
                "user_id",
                not_a_number.clone(),
            ),
            (
                "kick.reward.create",
                vec![("title", "Hydrate"), ("cost", "lots")],
                "cost",
                not_a_number.clone(),
            ),
            (
                "kick.reward.create",
                vec![("title", "Hydrate"), ("cost", "0")],
                "cost",
                ConfigReason::OutOfRange(ValueBounds::at_least(1)),
            ),
            (
                "kick.reward.update",
                vec![("reward_id", "rw-1"), ("cost", "cheap")],
                "cost",
                not_a_number.clone(),
            ),
            (
                "kick.channel.update_info",
                vec![("category_id", "just-chatting")],
                "category_id",
                not_a_number,
            ),
        ];
        let registry = offline_registry();

        for (id, entries, key, reason) in cases {
            let problem = registry
                .get(id)
                .unwrap()
                .validate_config(&config_of(&entries))
                .err()
                .map(RegistryError::into_config_problem);

            assert_eq!(
                problem,
                Some(ConfigProblem::field(key, reason)),
                "{id} {entries:?}"
            );
        }
    }

    #[test]
    fn whitespace_only_required_text_is_rejected_as_required_at_edit_time() {
        let cases: [(&str, Entries<'_>, &str); 8] = [
            (
                "kick.chat.send_message",
                vec![("message", " \t ")],
                "message",
            ),
            ("kick.moderation.ban", vec![("user_id", "  ")], "user_id"),
            (
                "kick.reward.create",
                vec![("title", "  "), ("cost", "5")],
                "title",
            ),
            ("kick.reward.delete", vec![("reward_id", " ")], "reward_id"),
            (
                "kick.reward.update",
                vec![("reward_id", " "), ("title", "New")],
                "reward_id",
            ),
            (
                "kick.reward.redemption_accept",
                vec![("redemption_ids", " , ")],
                "redemption_ids",
            ),
            ("kick.lookup.user", vec![("slug", "\t")], "slug"),
            ("kick.lookup.category", vec![("query", "  ")], "query"),
        ];
        let registry = offline_registry();

        for (id, entries, key) in cases {
            let problem = registry
                .get(id)
                .unwrap()
                .validate_config(&config_of(&entries))
                .err()
                .map(RegistryError::into_config_problem);

            assert_eq!(
                problem,
                Some(ConfigProblem::field(key, ConfigReason::Required)),
                "{id}"
            );
        }
    }

    #[tokio::test]
    async fn a_non_numeric_literal_user_id_fails_the_run_with_the_keyed_config_message() {
        let registry = offline_registry();
        let stack = ArgStack::new();

        let (telemetry, produced) = registry
            .get("kick.moderation.ban")
            .unwrap()
            .execute(
                &config_of(&[("user_id", "alice")]),
                &test_support::make_ctx(&stack),
            )
            .await;

        assert_eq!(
            telemetry.outcome,
            SubActionOutcome::Failed(test_support::step_problem(
                "kick.moderation.ban",
                &forge_registry::ConfigProblem::field(
                    "user_id",
                    forge_registry::ConfigReason::NotANumber
                )
            ))
        );
        assert!(produced.is_none());
    }

    #[test]
    fn missing_updatable_fields_fall_back_to_keep_current_and_ask_for_any_of_them() {
        let registry = offline_registry();
        let cases = [
            (
                "kick.channel.update_info",
                BTreeMap::new(),
                vec!["title", "category_id", "tags"],
            ),
            (
                "kick.reward.update",
                BTreeMap::from([("reward_id".to_owned(), Variant::String("rw-1".to_owned()))]),
                vec!["title", "cost", "description"],
            ),
        ];

        for (id, config, keys) in cases {
            let problem = registry
                .get(id)
                .unwrap()
                .validate_config(&config)
                .err()
                .map(RegistryError::into_config_problem);

            assert_eq!(
                problem,
                Some(ConfigProblem::whole(ConfigReason::any_of(keys))),
                "{id}"
            );
        }
    }
}
