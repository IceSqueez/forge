use std::collections::HashSet;
use std::sync::Arc;

use forge_overlay::OverlayKindDescriptor;
use forge_registry::{SubActionRegistry, TriggerKindDescriptor};
use forge_storage::{
    ActionRepo, ActionTelemetry, HistoryRepo, OverlayDefinition, OverlayId, QueueRepo,
    SoundboardClipsRepo, StorageError, TriggerInstanceRepo,
};
use forge_types::{ActionId, ClipId, ExecutionContext, TriggerInstance, TriggerInstanceId};
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
    clips: Arc<dyn SoundboardClipsRepo>,
}

impl ActionsService {
    pub fn new(
        actions: Arc<dyn ActionRepo>,
        queues: Arc<dyn QueueRepo>,
        history: Arc<dyn HistoryRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        clips: Arc<dyn SoundboardClipsRepo>,
    ) -> Self {
        Self {
            actions,
            queues,
            history,
            trigger_instances,
            clips,
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
                .map(|q| q.name.clone())
                .unwrap_or_else(|| "Default".to_string());

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

    /// Excludes auto-provisioned default instances; only author-created ones are linkable.
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

    /// Appends after the action's existing links; position is the current linked count.
    pub async fn link_trigger_instance(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError> {
        let position = self
            .trigger_instances
            .list_for_action(action_id)
            .await?
            .len() as i64;
        self.trigger_instances
            .link_action(action_id, instance_id, position)
            .await
    }

    /// The instance itself survives; only the action-trigger link is removed.
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

        let mut records = OverlayWiringRecords {
            queue: Some(queue.clone()),
            ..OverlayWiringRecords::default()
        };

        let action = plan.action.on_queue(queue.id());
        if let Err(source) = self.actions.save(&action).await {
            return Err(OverlayWiringError::Incomplete {
                records: Box::new(records),
                source,
            });
        }
        records.action_id = Some(action.id);

        if let Err(source) = self.trigger_instances.save(&plan.trigger).await {
            return Err(OverlayWiringError::Incomplete {
                records: Box::new(records),
                source,
            });
        }
        records.trigger_instance_id = Some(plan.trigger.id);

        if let Err(source) = self.link_trigger_instance(action.id, plan.trigger.id).await {
            return Err(OverlayWiringError::Incomplete {
                records: Box::new(records),
                source,
            });
        }
        records.linked = true;

        Ok(OverlayWiringOutcome::Wired(records))
    }

    async fn resolve_queue(&self, placement: QueuePlacement) -> Result<WiredQueue, StorageError> {
        if let Some(existing) = self.queues.get_by_name(placement.name).await? {
            return Ok(WiredQueue::Existing(existing));
        }
        let queue = placement.into_queue();
        self.queues.save(&queue).await?;
        Ok(WiredQueue::Created(queue))
    }

    pub async fn list_clip_options(&self) -> Vec<(ClipId, String)> {
        self.clips
            .list()
            .await
            .map(|clips| clips.into_iter().map(|c| (c.id, c.name)).collect())
            .unwrap_or_default()
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
