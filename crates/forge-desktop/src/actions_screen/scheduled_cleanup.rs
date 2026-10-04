use super::*;
use forge_runtime::ScheduledRunsHandle;
use forge_storage::ScheduledRunRepo;

#[derive(Clone)]
pub(super) struct ScheduledRunsAccess {
    repo: Arc<dyn ScheduledRunRepo>,
    runs: ScheduledRunsHandle,
}

impl ScheduledRunsAccess {
    pub(super) async fn cancel_pending_for(&self, action: ActionId) -> Result<(), String> {
        let pending = self.repo.list_pending().await.map_err(|e| e.to_string())?;
        for run in pending
            .into_iter()
            .filter(|run| run.spec.target_action_id == action)
        {
            self.runs.cancel(run.id).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

impl ScreenActionsView {
    pub fn with_scheduled_runs(
        mut self,
        repo: Arc<dyn ScheduledRunRepo>,
        runs: ScheduledRunsHandle,
    ) -> Self {
        self.scheduled_runs = Some(ScheduledRunsAccess { repo, runs });
        self
    }

    pub(super) fn load_delete_scheduled_count(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.delete_scheduled_count = None;
        let Some(access) = self.scheduled_runs.as_ref() else {
            return;
        };
        let repo = Arc::clone(&access.repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.count_pending_for_action(id).await },
            move |this, result, cx| {
                if let Ok(count) = result {
                    this.delete_scheduled_count = Some((id, count));
                    cx.notify();
                }
            },
            cx,
        );
    }

    pub(super) fn delete_scheduled_note(&self, id: ActionId) -> Option<String> {
        match self.delete_scheduled_count {
            Some((counted, count)) if counted == id && count > 0 => Some(tr!(
                "actions_delete_scheduled_runs",
                count = i64::try_from(count).unwrap_or(i64::MAX)
            )),
            _ => None,
        }
    }
}
