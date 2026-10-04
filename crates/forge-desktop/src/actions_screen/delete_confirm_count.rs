use super::*;
use forge_storage::ScheduledRunRepo;

impl ScreenActionsView {
    pub fn with_scheduled_runs(mut self, repo: Arc<dyn ScheduledRunRepo>) -> Self {
        self.scheduled_runs = Some(repo);
        self
    }

    pub(super) fn load_delete_scheduled_count(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.delete_scheduled_count = None;
        let Some(repo) = self.scheduled_runs.as_ref().map(Arc::clone) else {
            return;
        };
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
