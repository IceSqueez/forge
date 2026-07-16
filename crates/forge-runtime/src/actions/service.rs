use std::collections::HashSet;
use std::sync::Arc;

use forge_storage::{
    ActionRepo, ActionTelemetry, HistoryRepo, QueueRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo,
};
use forge_types::{ActionId, ClipId, TriggerInstance, TriggerInstanceId};
use time::OffsetDateTime;

use super::types::{ActionDetail, ActionSummary};

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

        Ok(ActionDetail {
            action,
            trigger_instances,
            sub_action_avg_ms,
        })
    }

    pub async fn load_telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        self.actions.telemetry(id).await
    }

    /// The user-defined trigger instances not yet linked to `action_id` - the set
    /// offered when linking a new trigger to the action. Auto-provisioned default
    /// instances are excluded; only author-created ones are linkable here.
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

    /// Links `instance_id` to `action_id`, appending it after the action's existing
    /// links (its position is the current linked count).
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

    /// Unlinks `instance_id` from `action_id`. The instance itself survives; only the
    /// action↔trigger link is removed.
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
