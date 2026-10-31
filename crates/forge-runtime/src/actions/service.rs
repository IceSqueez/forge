use std::collections::HashSet;
use std::sync::Arc;

use forge_overlay::OverlayKindDescriptor;
use forge_registry::{SubActionRegistry, TriggerKindDescriptor};
use forge_storage::{
    ActionRepo, ActionTelemetry, HistoryRepo, OverlayDefinition, OverlayId, QueueRepo,
    StorageError, TriggerChanges, TriggerInstanceRepo, TriggerWiring,
};
use forge_types::{ActionId, ExecutionContext, TriggerInstance, TriggerInstanceId};
use time::OffsetDateTime;

use super::overlay_wiring::{
    OverlayWiringError, OverlayWiringOutcome, OverlayWiringRecords, QueuePlacement, WiredQueue,
    plan_overlay_wiring,
};
use super::types::{ActionDetail, ActionSummary, OverlayFeed};
use crate::sub_action_runners::feeds_overlay;

pub struct ActionsService {
    actions: Arc<dyn ActionRepo>,
    queues: Arc<dyn QueueRepo>,
    history: Arc<dyn HistoryRepo>,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    wiring_turn: tokio::sync::Mutex<()>,
}

impl ActionsService {
    pub fn new(
        actions: Arc<dyn ActionRepo>,
        queues: Arc<dyn QueueRepo>,
        history: Arc<dyn HistoryRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
    ) -> Self {
        Self {
            actions,
            queues,
            history,
            trigger_instances,
            wiring_turn: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn list_summaries(&self) -> Result<Vec<ActionSummary>, StorageError> {
        let actions = self.actions.list().await?;
        let all_queues = self.queues.list().await?;
        let since = OffsetDateTime::now_utc() - time::Duration::hours(24);
        let stats = self.history.stats_summary(since).await?;

        let mut summaries = Vec::with_capacity(actions.len());
        for action in actions {
            let action_triggers = self.trigger_instances.list_for_action(action.id).await?;
            let first_trigger_kind_id = action_triggers.first().map(|t| t.kind_id.clone());

            let queue_name = all_queues
                .iter()
                .find(|q| q.id == action.queue_id)
                .or_else(|| all_queues.iter().find(|q| q.is_default()))
                .map(|q| q.name.clone())
                .unwrap_or_default();

            let (last_ran, runs_24h) = stats
                .get(&action.id)
                .map(|s| (Some(s.last_ran_at), s.runs_24h))
                .unwrap_or((None, 0));

            summaries.push(ActionSummary {
                id: action.id,
                group: action.group.clone(),
                name: action.name,
                enabled: action.enabled,
                sub_action_count: action.sub_actions.len() as u16,
                first_trigger_kind_id,
                queue_name,
                last_ran,
                runs_24h,
            });
        }
        Ok(summaries)
    }

    pub async fn load_detail(&self, id: ActionId) -> Result<ActionDetail, StorageError> {
        let action = self
            .actions
            .get(id)
            .await?
            .ok_or_else(|| StorageError::NotFound {
                key: id.to_string(),
            })?;
        let trigger_instances = self.trigger_instances.list_for_action(id).await?;
        let recent = self.history.recent_for_action(id, 20).await?;
        let sub_action_avg_ms = compute_sub_action_averages(&recent, action.sub_actions.len());
        let last_step_outcomes = compute_last_step_outcomes(&recent, action.sub_actions.len());

        Ok(ActionDetail {
            action,
            trigger_instances,
            sub_action_avg_ms,
            last_step_outcomes,
        })
    }

    pub async fn recent_runs(
        &self,
        id: ActionId,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        self.history.recent_for_action(id, limit).await
    }

    pub async fn load_telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        self.actions.telemetry(id).await
    }

    pub async fn list_linkable_triggers(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        let linked: HashSet<TriggerInstanceId> = self
            .trigger_instances
            .list_for_action(action_id)
            .await?
            .into_iter()
            .map(|instance| instance.id)
            .collect();
        let available = self
            .trigger_instances
            .list_user_defined()
            .await?
            .into_iter()
            .filter(|instance| !linked.contains(&instance.id))
            .collect();
        Ok(available)
    }

    pub async fn link_trigger_instance(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError> {
        self.trigger_instances
            .link_action(action_id, instance_id)
            .await
    }

    pub async fn create_linked_trigger(
        &self,
        action_id: ActionId,
        instance: TriggerInstance,
    ) -> Result<(), StorageError> {
        let instance_id = instance.id;
        self.trigger_instances
            .apply_changes(&TriggerChanges {
                saved: vec![instance],
                linked: vec![(action_id, instance_id)],
                ..TriggerChanges::default()
            })
            .await
    }

    pub async fn unlink_trigger_instance(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError> {
        self.trigger_instances
            .unlink_action(action_id, instance_id)
            .await
            .map(|_| ())
    }

    pub async fn overlay_feeds(
        &self,
        overlay: &OverlayId,
        sub_actions: &SubActionRegistry,
    ) -> Result<Vec<OverlayFeed>, StorageError> {
        let mut feeds = Vec::new();
        for action in self.actions.list().await? {
            if !feeds_overlay(&action.sub_actions, sub_actions, overlay.as_str()) {
                continue;
            }
            let triggers = self.trigger_instances.list_for_action(action.id).await?;
            feeds.push(OverlayFeed {
                action_id: action.id,
                action_name: action.name,
                action_enabled: action.enabled,
                triggers,
            });
        }
        Ok(feeds)
    }

    pub async fn overlay_wired_to(
        &self,
        overlay: &OverlayId,
        trigger_kind_id: &str,
        sub_actions: &SubActionRegistry,
    ) -> Result<Option<ActionId>, StorageError> {
        Ok(self
            .overlay_feeds(overlay, sub_actions)
            .await?
            .into_iter()
            .find(|feed| {
                feed.triggers
                    .iter()
                    .any(|instance| instance.kind_id == trigger_kind_id)
            })
            .map(|feed| feed.action_id))
    }

    pub async fn wire_overlay_to_event(
        &self,
        overlay: &OverlayDefinition,
        kind: &dyn OverlayKindDescriptor,
        trigger: &dyn TriggerKindDescriptor,
        sub_actions: &SubActionRegistry,
    ) -> Result<OverlayWiringOutcome, OverlayWiringError> {
        let plan = plan_overlay_wiring(overlay, kind, trigger)?;
        let _one_wiring_at_a_time = self.wiring_turn.lock().await;

        match self
            .overlay_wired_to(&overlay.id, trigger.id(), sub_actions)
            .await
        {
            Ok(Some(action_id)) => return Ok(OverlayWiringOutcome::AlreadyWired { action_id }),
            Ok(None) => {}
            Err(source) => return Err(OverlayWiringError::NothingWritten { source }),
        }

        let queue = match self.resolve_queue(plan.queue).await {
            Ok(queue) => queue,
            Err(source) => return Err(OverlayWiringError::NothingWritten { source }),
        };

        let action = plan.action.on_queue(queue.id());
        let wiring = TriggerWiring {
            created_queue: queue.created().cloned(),
            action,
            instance: plan.trigger,
        };
        if let Err(source) = self
            .trigger_instances
            .commit_overlay_wiring(&wiring, Arc::clone(&self.queues), Arc::clone(&self.actions))
            .await
        {
            return Err(OverlayWiringError::NothingWritten { source });
        }

        Ok(OverlayWiringOutcome::Wired(OverlayWiringRecords {
            queue: Some(queue),
            action_id: Some(wiring.action.id),
            trigger_instance_id: Some(wiring.instance.id),
            linked: true,
        }))
    }

    async fn resolve_queue(&self, placement: QueuePlacement) -> Result<WiredQueue, StorageError> {
        match self.queues.overlay_alert_queue(placement.name).await? {
            Some(existing) => Ok(WiredQueue::Existing(existing)),
            None => Ok(WiredQueue::Created(placement.into_queue())),
        }
    }
}

fn compute_sub_action_averages(
    history: &[forge_types::ExecutionContext],
    sub_action_count: usize,
) -> Vec<Option<u64>> {
    let mut sums: Vec<u64> = vec![0; sub_action_count];
    let mut counts: Vec<u64> = vec![0; sub_action_count];
    for ctx in history {
        for t in &ctx.telemetry {
            if !t.is_nested() && t.index < sub_action_count {
                sums[t.index] += t.duration_ms;
                counts[t.index] += 1;
            }
        }
    }
    sums.iter()
        .zip(counts.iter())
        .map(|(s, c)| if *c > 0 { Some(s / c) } else { None })
        .collect()
}

fn compute_last_step_outcomes(
    history: &[forge_types::ExecutionContext],
    sub_action_count: usize,
) -> Vec<Option<forge_types::SubActionOutcome>> {
    let mut outcomes: Vec<Option<forge_types::SubActionOutcome>> = vec![None; sub_action_count];
    if let Some(latest) = history.first() {
        for t in &latest.telemetry {
            if !t.is_nested() && t.index < sub_action_count {
                outcomes[t.index] = Some(t.outcome.clone());
            }
        }
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_storage::action::MockActionRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::queue::MockQueueRepo;
    use forge_storage::trigger_instance::MockTriggerInstanceRepo;
    use forge_types::{
        Action, DEFAULT_QUEUE_ID, ExecutionContext, ExecutionMetadata, ExecutionMode,
        ExecutionOutcome, Queue, QueueId, SubActionOutcome, SubActionTelemetry, UserText,
        VerbatimText,
    };

    use super::*;

    fn queue(id: QueueId, name: &str) -> Queue {
        Queue {
            id,
            name: name.to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        }
    }

    fn step(index: usize, duration_ms: u64, outcome: SubActionOutcome) -> SubActionTelemetry {
        SubActionTelemetry {
            index,
            kind: "core.wait".to_owned(),
            started_at: time::OffsetDateTime::UNIX_EPOCH,
            duration_ms,
            outcome,
            args_in: Default::default(),
            produced: Default::default(),
        }
    }

    fn run(telemetry: Vec<SubActionTelemetry>) -> ExecutionContext {
        ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::QuickAction {
                builtin_id: "obs".to_owned(),
                label: "run".to_owned(),
            },
            arg_stack_snapshot: Default::default(),
            started_at: time::OffsetDateTime::UNIX_EPOCH,
            completed_at: None,
            telemetry,
            outcome: ExecutionOutcome::Success,
        }
    }

    fn failed() -> SubActionOutcome {
        SubActionOutcome::Failed(UserText::Verbatim(VerbatimText::new("boom")))
    }

    #[test]
    fn step_averages_divide_by_the_runs_each_step_appeared_in() {
        let history = [
            run(vec![
                step(0, 10, SubActionOutcome::Success),
                step(1, 7, SubActionOutcome::Success),
            ]),
            run(vec![step(0, 21, SubActionOutcome::Success)]),
        ];

        assert_eq!(
            compute_sub_action_averages(&history, 3),
            vec![Some(15), Some(7), None]
        );
    }

    #[test]
    fn step_averages_ignore_nested_rows_and_steps_since_deleted() {
        let history = [run(vec![
            step(SubActionTelemetry::NESTED, 1_000, SubActionOutcome::Success),
            step(2, 1_000, SubActionOutcome::Success),
            step(0, 4, SubActionOutcome::Success),
        ])];

        assert_eq!(
            compute_sub_action_averages(&history, 2),
            vec![Some(4), None]
        );
    }

    #[test]
    fn empty_history_gives_no_average_and_no_outcome_for_any_step() {
        assert_eq!(compute_sub_action_averages(&[], 2), vec![None, None]);
        assert_eq!(compute_last_step_outcomes(&[], 2), vec![None, None]);
    }

    #[test]
    fn last_outcomes_come_only_from_the_newest_run() {
        let history = [
            run(vec![step(0, 1, failed())]),
            run(vec![
                step(0, 1, SubActionOutcome::Success),
                step(1, 1, SubActionOutcome::Success),
            ]),
        ];

        assert_eq!(
            compute_last_step_outcomes(&history, 2),
            vec![Some(failed()), None]
        );
    }

    #[test]
    fn last_outcomes_ignore_nested_rows_and_steps_since_deleted() {
        let history = [run(vec![
            step(SubActionTelemetry::NESTED, 1, failed()),
            step(1, 1, failed()),
            step(0, 1, SubActionOutcome::Success),
        ])];

        assert_eq!(
            compute_last_step_outcomes(&history, 1),
            vec![Some(SubActionOutcome::Success)]
        );
    }

    #[tokio::test]
    async fn an_action_whose_queue_is_gone_is_listed_under_the_default_queue_by_id_not_by_name() {
        let orphan = Action {
            id: ActionId::new(),
            name: "Greeter".to_owned(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        };
        let mut actions = MockActionRepo::new();
        actions
            .expect_list()
            .returning(move || Ok(vec![orphan.clone()]));
        let mut queues = MockQueueRepo::new();
        queues.expect_list().returning(|| {
            Ok(vec![
                queue(QueueId::new(), "Default"),
                queue(DEFAULT_QUEUE_ID, "Main"),
            ])
        });
        let mut history = MockHistoryRepo::new();
        history
            .expect_stats_summary()
            .returning(|_| Ok(HashMap::new()));
        let mut triggers = MockTriggerInstanceRepo::new();
        triggers
            .expect_list_for_action()
            .returning(|_| Ok(Vec::new()));
        let service = ActionsService::new(
            Arc::new(actions),
            Arc::new(queues),
            Arc::new(history),
            Arc::new(triggers),
        );

        let summaries = service.list_summaries().await.unwrap();

        assert_eq!(
            summaries
                .iter()
                .map(|s| s.queue_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Main"]
        );
    }

    fn follow_trigger() -> TriggerInstance {
        TriggerInstance {
            id: forge_types::TriggerInstanceId::new(),
            kind_id: "twitch.follow".to_owned(),
            name: "follow".to_owned(),
            overrides: Default::default(),
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: forge_types::PermissionRung::Everyone,
        }
    }

    async fn service_over_store() -> (
        crate::test_support::Sandboxed<Arc<dyn forge_storage::DataProvider>>,
        ActionsService,
    ) {
        let provider = crate::test_support::sandboxed_backend([0x5c; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn forge_storage::DataProvider>);
        let service = ActionsService::new(
            provider.action_repo(),
            provider.queue_repo(),
            provider.history_repo(),
            provider.trigger_instance_repo(),
        );
        (provider, service)
    }

    #[tokio::test]
    async fn a_trigger_created_for_an_action_lands_linked_in_one_write() {
        let (provider, service) = service_over_store().await;
        let action = Action {
            id: ActionId::new(),
            name: "Greeter".to_owned(),
            group: None,
            queue_id: DEFAULT_QUEUE_ID,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        };
        provider.action_repo().save(&action).await.unwrap();
        let instance = follow_trigger();

        service
            .create_linked_trigger(action.id, instance.clone())
            .await
            .unwrap();

        let linked = provider
            .trigger_instance_repo()
            .actions_using(instance.id)
            .await
            .unwrap();
        assert_eq!(linked, [action.id]);
    }

    #[tokio::test]
    async fn a_trigger_whose_link_cannot_land_leaves_no_orphan_row() {
        let (provider, service) = service_over_store().await;
        let instance = follow_trigger();

        let created = service
            .create_linked_trigger(ActionId::new(), instance.clone())
            .await;

        assert!(created.is_err(), "got {created:?}");
        let stored = provider
            .trigger_instance_repo()
            .get(instance.id)
            .await
            .unwrap();
        assert!(stored.is_none());
    }
}
