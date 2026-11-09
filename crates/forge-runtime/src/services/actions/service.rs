use crate::error::RuntimeError;
use std::collections::HashSet;
use std::sync::Arc;

use forge_registry::SubActionRegistry;
use forge_storage::{
    ActionRepo, ActionTelemetry, HistoryRepo, OverlayId, QueueRepo, StorageError, TriggerChanges,
    TriggerInstanceRepo, TriggerWiring,
};
use forge_types::{Action, ActionId, ExecutionContext, TriggerInstance, TriggerInstanceId};
use time::OffsetDateTime;

use super::overlay_wiring::{
    OverlayWiringOutcome, OverlayWiringPlan, OverlayWiringPreview, OverlayWiringRecords,
    OverlayWiringRefusal, OverlayWiringRequest, PlannedAction, PlannedTrigger, WiringTarget,
    assemble, chosen_queue_id, ensure_eligible, extended, new_action, new_action_name, new_trigger,
    send_step,
};
use super::types::{ActionDetail, ActionSummary, OverlayFeed};
use crate::sub_action_runners::feeds_overlay;

const RUN_STATS_WINDOW: time::Duration = time::Duration::DAY;
const DETAIL_HISTORY_RUNS: u32 = 20;

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
        let since = OffsetDateTime::now_utc() - RUN_STATS_WINDOW;
        let stats = self.history.stats_summary(since).await?;

        let links = self.trigger_instances.instances_by_action().await?;

        let mut summaries = Vec::with_capacity(actions.len());
        for action in actions {
            let first_trigger_kind_id = links
                .get(&action.id)
                .and_then(|triggers| triggers.first())
                .map(|t| t.kind_id.clone());

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
                sub_action_count: u16::try_from(action.sub_actions.len()).unwrap_or(u16::MAX),
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
        let recent = self
            .history
            .recent_for_action(id, DETAIL_HISTORY_RUNS)
            .await?;
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
        let mut links = self.trigger_instances.instances_by_action().await?;
        let mut feeds = Vec::new();
        for action in self.actions.list().await? {
            if !feeds_overlay(&action.sub_actions, sub_actions, overlay.as_str()) {
                continue;
            }
            let triggers = links.remove(&action.id).unwrap_or_default();
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

    pub async fn plan_overlay_wiring(
        &self,
        request: OverlayWiringRequest<'_>,
        sub_actions: &SubActionRegistry,
    ) -> Result<OverlayWiringPreview, RuntimeError> {
        let OverlayWiringRequest {
            overlay,
            kind,
            trigger,
            target,
            content,
        } = request;
        ensure_eligible(overlay, kind)?;
        let step = send_step(overlay, kind, content);

        match target {
            WiringTarget::NewAction { name, queue_id } => {
                let name = new_action_name(&name)?;
                let queue_id = chosen_queue_id(queue_id);
                let queue = self
                    .queues
                    .get(queue_id)
                    .await?
                    .ok_or(OverlayWiringRefusal::QueueMissing { queue_id })?;
                let action = new_action(name, queue_id, step.clone());
                Ok(OverlayWiringPreview::Ready(Box::new(assemble(
                    overlay,
                    trigger.id(),
                    PlannedAction::Create { action, queue },
                    step,
                    PlannedTrigger::Create(new_trigger(overlay, trigger)),
                ))))
            }
            WiringTarget::ExistingAction(action_id) => {
                let current = self
                    .current_wiring_state(action_id, &overlay.id, trigger.id(), sub_actions)
                    .await?;
                if current.already_wired {
                    return Ok(OverlayWiringPreview::AlreadyWired { action_id });
                }
                let planned_trigger = match current.linked_instance {
                    Some(instance) => PlannedTrigger::Reuse(instance),
                    None => PlannedTrigger::Create(new_trigger(overlay, trigger)),
                };
                Ok(OverlayWiringPreview::Ready(Box::new(assemble(
                    overlay,
                    trigger.id(),
                    PlannedAction::Extend {
                        action: extended(current.action, step.clone()),
                    },
                    step,
                    planned_trigger,
                ))))
            }
        }
    }

    pub async fn commit_overlay_wiring(
        &self,
        plan: &OverlayWiringPlan,
        sub_actions: &SubActionRegistry,
    ) -> Result<OverlayWiringOutcome, RuntimeError> {
        let _one_wiring_at_a_time = self.wiring_turn.lock().await;

        let wiring = match self.wiring_to_write(plan, sub_actions).await {
            Ok(Ok(wiring)) => wiring,
            Ok(Err(action_id)) => return Ok(OverlayWiringOutcome::AlreadyWired { action_id }),
            Err(RuntimeError::Storage(source)) => {
                return Err(RuntimeError::OverlayWiringNothingWritten { source });
            }
            Err(error) => return Err(error),
        };

        if let Err(source) = self.trigger_instances.commit_overlay_wiring(&wiring).await {
            return Err(RuntimeError::OverlayWiringNothingWritten { source });
        }

        Ok(OverlayWiringOutcome::Wired(OverlayWiringRecords {
            action_id: wiring.action.id,
            action_created: matches!(plan.action(), PlannedAction::Create { .. }),
            trigger_instance_id: plan.trigger().instance().id,
            trigger_created: wiring.instance.is_some(),
        }))
    }

    async fn wiring_to_write(
        &self,
        plan: &OverlayWiringPlan,
        sub_actions: &SubActionRegistry,
    ) -> Result<Result<TriggerWiring, ActionId>, RuntimeError> {
        match plan.action() {
            PlannedAction::Create { action, .. } => {
                if self.queues.get(action.queue_id).await?.is_none() {
                    return Err(OverlayWiringRefusal::QueueMissing {
                        queue_id: action.queue_id,
                    }
                    .into());
                }
                let PlannedTrigger::Create(instance) = plan.trigger() else {
                    return Err(OverlayWiringRefusal::PlanOutdated.into());
                };
                Ok(Ok(TriggerWiring {
                    action: action.clone(),
                    instance: Some(instance.clone()),
                }))
            }
            PlannedAction::Extend { action } => {
                let current = self
                    .current_wiring_state(
                        action.id,
                        plan.overlay_id(),
                        plan.trigger_kind_id(),
                        sub_actions,
                    )
                    .await?;
                if current.already_wired {
                    return Ok(Err(action.id));
                }
                let instance = match (plan.trigger(), current.linked_instance) {
                    (PlannedTrigger::Create(planned), None) => Some(planned.clone()),
                    (PlannedTrigger::Reuse(planned), Some(linked)) if planned.id == linked.id => {
                        None
                    }
                    _ => return Err(OverlayWiringRefusal::PlanOutdated.into()),
                };
                Ok(Ok(TriggerWiring {
                    action: extended(current.action, plan.added_step().clone()),
                    instance,
                }))
            }
        }
    }

    async fn current_wiring_state(
        &self,
        action_id: ActionId,
        overlay: &OverlayId,
        trigger_kind_id: &str,
        sub_actions: &SubActionRegistry,
    ) -> Result<CurrentWiring, RuntimeError> {
        let action = self
            .actions
            .get(action_id)
            .await?
            .ok_or(OverlayWiringRefusal::ActionMissing { action_id })?;
        let linked_instance = self
            .trigger_instances
            .list_for_action(action_id)
            .await?
            .into_iter()
            .find(|instance| instance.kind_id == trigger_kind_id);
        let already_wired = linked_instance.is_some()
            && feeds_overlay(&action.sub_actions, sub_actions, overlay.as_str());
        Ok(CurrentWiring {
            action,
            linked_instance,
            already_wired,
        })
    }
}

struct CurrentWiring {
    action: Action,
    linked_instance: Option<TriggerInstance>,
    already_wired: bool,
}

fn compute_sub_action_averages(
    history: &[forge_types::ExecutionContext],
    sub_action_count: usize,
) -> Vec<Option<u64>> {
    let mut sums: Vec<u64> = vec![0; sub_action_count];
    let mut counts: Vec<u64> = vec![0; sub_action_count];
    for ctx in history {
        for t in &ctx.telemetry {
            if let Some(index) = t.position.top_level().filter(|i| *i < sub_action_count) {
                sums[index] += t.duration_ms;
                counts[index] += 1;
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
            if let Some(index) = t.position.top_level().filter(|i| *i < sub_action_count) {
                outcomes[index] = Some(t.outcome.clone());
            }
        }
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_storage::MockActionRepo;
    use forge_storage::MockHistoryRepo;
    use forge_storage::MockQueueRepo;
    use forge_storage::MockTriggerInstanceRepo;
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

    fn nested_step(duration_ms: u64, outcome: SubActionOutcome) -> SubActionTelemetry {
        SubActionTelemetry {
            position: forge_types::StepPosition::Nested,
            ..step(0, duration_ms, outcome)
        }
    }

    fn step(index: usize, duration_ms: u64, outcome: SubActionOutcome) -> SubActionTelemetry {
        SubActionTelemetry {
            position: forge_types::StepPosition::TopLevel(index),
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
            nested_step(1_000, SubActionOutcome::Success),
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
            nested_step(1, failed()),
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
        triggers
            .expect_instances_by_action()
            .returning(|| Ok(Default::default()));
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
        let provider = crate::test_support::sandboxed_backend()
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
