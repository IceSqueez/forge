use std::sync::Arc;

use forge_storage::{
    ActionRepo, ActionTelemetry, CommandRepo, HistoryRepo, QueueRepo, SoundboardClipsRepo,
    StorageError, TriggerRepo,
};
use forge_types::{ActionId, ClipId, SubActionStep};
use time::OffsetDateTime;

use super::types::{ActionDetail, ActionSummary};

pub struct ActionsService {
    actions: Arc<dyn ActionRepo>,
    queues: Arc<dyn QueueRepo>,
    history: Arc<dyn HistoryRepo>,
    triggers: Arc<dyn TriggerRepo>,
    commands: Arc<dyn CommandRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
}

impl ActionsService {
    pub fn new(
        actions: Arc<dyn ActionRepo>,
        queues: Arc<dyn QueueRepo>,
        history: Arc<dyn HistoryRepo>,
        triggers: Arc<dyn TriggerRepo>,
        commands: Arc<dyn CommandRepo>,
        clips: Arc<dyn SoundboardClipsRepo>,
    ) -> Self {
        Self {
            actions,
            queues,
            history,
            triggers,
            commands,
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
            let action_triggers = self.triggers.list_for_action(action.id).await?;
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
        let triggers = self.triggers.list_for_action(id).await?;
        let all_commands = self.commands.list().await?;
        let commands: Vec<_> = all_commands
            .into_iter()
            .filter(|c| c.action_id == id)
            .collect();
        let recent = self.history.recent_for_action(id, 20).await?;
        let sub_action_avg_ms = compute_sub_action_averages(&recent, action.sub_actions.len());

        Ok(ActionDetail {
            action,
            triggers,
            commands,
            sub_action_avg_ms,
        })
    }

    pub async fn save_sub_action(
        &self,
        action_id: ActionId,
        step: SubActionStep,
        editing_index: Option<usize>,
    ) -> Result<(), StorageError> {
        let Some(mut action) = self.actions.get(action_id).await? else {
            return Err(StorageError::NotFound {
                key: action_id.to_string(),
            });
        };
        if let Some(idx) = editing_index {
            if idx < action.sub_actions.len() {
                action.sub_actions[idx] = step;
            } else {
                action.sub_actions.push(step);
            }
        } else {
            action.sub_actions.push(step);
        }
        self.actions.save(&action).await
    }

    pub async fn move_sub_action(
        &self,
        action_id: ActionId,
        from: usize,
        to: usize,
    ) -> Result<ActionId, StorageError> {
        let Some(mut action) = self.actions.get(action_id).await? else {
            return Err(StorageError::NotFound {
                key: action_id.to_string(),
            });
        };
        let len = action.sub_actions.len();
        if from < len && to < len && from != to {
            let item = action.sub_actions.remove(from);
            action.sub_actions.insert(to, item);
            self.actions.save(&action).await?;
        }
        Ok(action_id)
    }

    pub async fn duplicate_sub_action(
        &self,
        action_id: ActionId,
        index: usize,
    ) -> Result<ActionId, StorageError> {
        let Some(mut action) = self.actions.get(action_id).await? else {
            return Err(StorageError::NotFound {
                key: action_id.to_string(),
            });
        };
        if index < action.sub_actions.len() {
            let copy = action.sub_actions[index].clone();
            action.sub_actions.insert(index + 1, copy);
            self.actions.save(&action).await?;
        }
        Ok(action_id)
    }

    pub async fn remove_sub_action(
        &self,
        action_id: ActionId,
        index: usize,
    ) -> Result<(), StorageError> {
        let Some(mut action) = self.actions.get(action_id).await? else {
            return Err(StorageError::NotFound {
                key: action_id.to_string(),
            });
        };
        if index < action.sub_actions.len() {
            action.sub_actions.remove(index);
        }
        self.actions.save(&action).await
    }

    pub async fn load_telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        self.actions.telemetry(id).await
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
            if t.index < sub_action_count {
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
