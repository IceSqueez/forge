use super::*;
use crate::async_bridge;
use crate::run_history_modal::{RunHistoryDismissed, RunHistoryModal};

impl ScreenActionsView {
    pub(super) fn open_history_modal(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let action_name = self
            .detail
            .as_ref()
            .map(|d| d.action.name.clone())
            .unwrap_or_else(|| tr!("action_editor_this_action"));
        self.header_menu_open = None;

        let registry = Arc::clone(&self.trigger_registry);
        let view = cx.new(|_| RunHistoryModal::new(action_name, registry));
        let sub = cx.subscribe(&view, Self::on_history_event);
        self.history_modal = Some(HistoryModalHost {
            view: view.clone(),
            _sub: sub,
        });

        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.recent_runs(id, 50).await.map_err(|e| e.to_string()) },
            move |this, result, cx| match result {
                Ok(runs) => view.update(cx, |modal, cx| modal.set_runs(runs, cx)),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn on_history_event(
        &mut self,
        _view: Entity<RunHistoryModal>,
        _event: &RunHistoryDismissed,
        cx: &mut Context<Self>,
    ) {
        self.history_modal = None;
        cx.notify();
    }
}
