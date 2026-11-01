use super::*;
use crate::app::async_bridge;
use crate::automation::run_history_modal::{
    RunHistoryDismissed, RunHistoryModal, RunHistoryOpenIntegration,
};
use crate::shared::repo_failure::RepoFailure;

const RUN_HISTORY_PAGE: u32 = 50;

impl ScreenActionsView {
    pub(super) fn open_history_modal(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.tree.read(cx).selected() else {
            return;
        };
        let action_name = self
            .editor
            .read(cx)
            .open_action_name()
            .unwrap_or_else(|| tr!("action_editor_this_action"));

        let registry = Arc::clone(&self.deps.trigger_registry);
        let names = self.tree.read(cx).action_names();
        let view = cx.new(|_| RunHistoryModal::new(action_name, registry).with_action_names(names));
        let subs = [
            cx.subscribe(&view, Self::on_history_event),
            cx.subscribe(&view, Self::on_history_open_integration),
        ];
        self.modals.history = Some(HistoryModalHost {
            view: view.clone(),
            _subs: subs,
        });
        self.sync_step_drag_lock(cx);

        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                service
                    .recent_runs(id, RUN_HISTORY_PAGE)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |_this, result, cx| match result {
                Ok(runs) => view.update(cx, |modal, cx| modal.set_runs(runs, cx)),
                Err(failure) => {
                    on_repo_error(&failure, cx);
                    view.update(cx, |modal, cx| modal.set_error(failure, cx));
                }
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
        self.modals.history = None;
        self.sync_step_drag_lock(cx);
        cx.notify();
    }

    fn on_history_open_integration(
        &mut self,
        _view: Entity<RunHistoryModal>,
        event: &RunHistoryOpenIntegration,
        cx: &mut Context<Self>,
    ) {
        self.modals.history = None;
        self.sync_step_drag_lock(cx);
        cx.emit(NavRequested(Screen::BuiltinDetail(event.0.clone())));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::automation::actions_screen::editor::integration_gating::tests::{Rig, detail, obs};
    use crate::test_support::{nav_log, step};
    use forge_runtime::LifecycleState;

    #[gpui::test]
    fn opening_an_integration_from_run_history_closes_the_modal_and_navigates_there(
        cx: &mut TestAppContext,
    ) {
        let open = detail(vec![step("obs.scene.set", true)]);
        let action_id = open.action.id;
        let rig = Rig::open(cx, LifecycleState::Disabled, Some(open));
        let log = cx.update(|cx| nav_log(&rig.host, cx));
        let tree = rig.host.read_with(cx, |host, _| host.tree.clone());
        tree.update(cx, |tree, _| tree.set_selected(Some(action_id)));
        let modal = rig.host.update(cx, |view, cx| {
            view.open_history_modal(cx);
            view.modals.history.as_ref().map(|host| host.view.clone())
        });
        let modal = modal.expect("the history modal opens for the selected action");

        modal.update(cx, |modal, cx| modal.open_integration(obs(), cx));

        let still_open = rig
            .host
            .read_with(cx, |view, _| view.modals.history.is_some());
        let screens = log.read_with(cx, |log, _| log.screens());
        assert!(!still_open, "the history modal stayed open");
        assert_eq!(screens, vec![Screen::BuiltinDetail(obs())]);
    }
}
