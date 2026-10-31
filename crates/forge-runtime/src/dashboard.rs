use forge_registry::{ChatTriggerFamily, TriggerRegistry};
use forge_storage::{ActionRepo, GlobalsRepo, HistoryRepo, StorageError, TriggerInstanceRepo};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct DashboardStats {
    pub actions_count: usize,
    pub commands_count: usize,
    pub triggers_fired: u64,
    pub globals_count: usize,
}

pub async fn compute_stats(
    actions: &dyn ActionRepo,
    globals: &dyn GlobalsRepo,
    history: &dyn HistoryRepo,
    triggers: &dyn TriggerInstanceRepo,
    registry: &TriggerRegistry,
    since: OffsetDateTime,
) -> Result<DashboardStats, StorageError> {
    let actions_count = actions.count().await?;
    let globals_count = globals.count().await?;
    let command_kinds: Vec<String> = registry
        .all()
        .filter(|descriptor| descriptor.chat_trigger_family() == Some(ChatTriggerFamily::Command))
        .map(|descriptor| descriptor.id().to_owned())
        .collect();
    let commands_count = triggers.count_of_kinds(&command_kinds).await?;
    let stats = history.stats_summary(since).await?;
    let triggers_fired: u64 = stats.values().map(|s| u64::from(s.runs_24h)).sum();
    Ok(DashboardStats {
        actions_count,
        commands_count,
        triggers_fired,
        globals_count,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_events::{Event, EventSource};
    use forge_registry::{
        ChatTriggerFamily, EventFilter, FormField, KindPlatformContract, TriggerCategory,
        TriggerKindDescriptor,
    };
    use forge_storage::ActionStats;
    use forge_storage::action::MockActionRepo;
    use forge_storage::globals::MockGlobalsRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::trigger_instance::MockTriggerInstanceRepo;
    use forge_types::{ActionId, ArgStack, TriggerConfig};

    use super::*;

    const COMMAND_KIND: &str = "test.command";
    const PLAIN_KIND: &str = "test.plain";
    const ACTIONS: usize = 3;
    const COMMANDS: usize = 4;
    const GLOBALS: usize = 2;
    const SINCE: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    struct FakeTrigger {
        id: &'static str,
        family: Option<ChatTriggerFamily>,
    }

    impl TriggerKindDescriptor for FakeTrigger {
        fn id(&self) -> &str {
            self.id
        }
        fn category(&self) -> TriggerCategory {
            TriggerCategory::Chat
        }
        fn label(&self) -> &str {
            ""
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            ""
        }
        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::Universal
        }
        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: Some(EventSource::Core),
                kind_prefix: None,
            }
        }
        fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
            false
        }
        fn build_arg_stack(&self, _: &Event) -> ArgStack {
            ArgStack::new()
        }
        fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
            self.family
        }
    }

    fn registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        for (id, family) in [
            (COMMAND_KIND, Some(ChatTriggerFamily::Command)),
            (PLAIN_KIND, None),
        ] {
            registry
                .register(Box::new(FakeTrigger { id, family }))
                .unwrap();
        }
        registry
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Repo {
        Actions,
        Globals,
        Triggers,
        History,
    }

    fn refusal(repo: Repo) -> StorageError {
        StorageError::Connection {
            reason: format!("{repo:?}"),
        }
    }

    fn outcome_of<T>(healthy: T, repo: Repo, failing: Option<Repo>) -> Result<T, StorageError> {
        if failing == Some(repo) {
            Err(refusal(repo))
        } else {
            Ok(healthy)
        }
    }

    async fn stats_with(
        runs: &[u32],
        failing: Option<Repo>,
    ) -> Result<DashboardStats, StorageError> {
        let mut actions = MockActionRepo::new();
        actions
            .expect_count()
            .returning(move || outcome_of(ACTIONS, Repo::Actions, failing));
        let mut globals = MockGlobalsRepo::new();
        globals
            .expect_count()
            .returning(move || outcome_of(GLOBALS, Repo::Globals, failing));
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers
            .expect_count_of_kinds()
            .withf(|kinds| kinds == [COMMAND_KIND.to_owned()])
            .returning(move |_| outcome_of(COMMANDS, Repo::Triggers, failing));
        let summary: HashMap<ActionId, ActionStats> = runs
            .iter()
            .map(|&runs_24h| {
                (
                    ActionId::new(),
                    ActionStats {
                        last_ran_at: SINCE,
                        runs_24h,
                    },
                )
            })
            .collect();
        let mut history = MockHistoryRepo::new();
        history
            .expect_stats_summary()
            .withf(|since| *since == SINCE)
            .returning(move |_| outcome_of(summary.clone(), Repo::History, failing));

        compute_stats(&actions, &globals, &history, &triggers, &registry(), SINCE).await
    }

    #[tokio::test]
    async fn stats_report_each_repo_count_and_only_command_family_triggers() {
        let stats = stats_with(&[], None).await.unwrap();

        assert_eq!(
            (
                stats.actions_count,
                stats.commands_count,
                stats.globals_count
            ),
            (ACTIONS, COMMANDS, GLOBALS)
        );
    }

    #[tokio::test]
    async fn triggers_fired_sums_the_runs_of_every_action_without_narrowing() {
        let widest_sum = u64::from(u32::MAX) * 2;
        for (runs, expected) in [
            (vec![], 0),
            (vec![5], 5),
            (vec![5, 7], 12),
            (vec![u32::MAX, u32::MAX], widest_sum),
        ] {
            let stats = stats_with(&runs, None).await.unwrap();
            assert_eq!(stats.triggers_fired, expected, "{runs:?}");
        }
    }

    #[tokio::test]
    async fn any_failing_repo_fails_the_stats_with_its_own_error() {
        for repo in [Repo::Actions, Repo::Globals, Repo::Triggers, Repo::History] {
            let failure = stats_with(&[], Some(repo)).await.unwrap_err();
            assert!(
                matches!(&failure, StorageError::Connection { reason } if *reason == format!("{repo:?}")),
                "{repo:?}: {failure:?}"
            );
        }
    }
}
