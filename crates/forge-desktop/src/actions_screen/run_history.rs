use super::*;
use crate::async_bridge;
use crate::run_history_modal::{RunHistoryDismissed, RunHistoryModal, RunHistoryOpenIntegration};

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
        let names = self.action_names();
        let view = cx.new(|_| RunHistoryModal::new(action_name, registry).with_action_names(names));
        let subs = [
            cx.subscribe(&view, Self::on_history_event),
            cx.subscribe(&view, Self::on_history_open_integration),
        ];
        self.history_modal = Some(HistoryModalHost {
            view: view.clone(),
            _subs: subs,
        });

        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.recent_runs(id, 50).await.map_err(|e| e.to_string()) },
            move |this, result, cx| match result {
                Ok(runs) => view.update(cx, |modal, cx| modal.set_runs(runs, cx)),
                Err(message) => {
                    view.update(cx, |modal, cx| {
                        modal.set_error(
                            tr!("action_editor_run_history_failed", error = message.as_str()),
                            cx,
                        );
                    });
                    this.on_repo_error(&message, cx);
                }
            },
            cx,
        );
        cx.notify();
    }

    fn action_names(&self) -> HashMap<ActionId, SharedString> {
        self.groups
            .iter()
            .flat_map(|group| group.actions.iter())
            .map(|action| (action.id, SharedString::from(action.name.clone())))
            .collect()
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

    fn on_history_open_integration(
        &mut self,
        _view: Entity<RunHistoryModal>,
        event: &RunHistoryOpenIntegration,
        cx: &mut Context<Self>,
    ) {
        self.history_modal = None;
        cx.emit(NavRequested(Screen::BuiltinDetail(event.0.clone())));
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use gpui::TestAppContext;

    use super::*;
    use crate::actions_screen::integration_gating::tests::{Rig, detail, obs};
    use crate::integration_supervisor::LifecycleState;
    use crate::test_support::{nav_log, step};

    #[gpui::test]
    fn opening_an_integration_from_run_history_closes_the_modal_and_navigates_there(
        cx: &mut TestAppContext,
    ) {
        let open = detail(vec![step("obs.scene.set", true)]);
        let action_id = open.action.id;
        let rig = Rig::open(cx, LifecycleState::Disabled, Some(open));
        let log = cx.update(|cx| nav_log(&rig.view, cx));
        let modal = rig.view.update(cx, |view, cx| {
            view.selected = Some(action_id);
            view.open_history_modal(cx);
            view.history_modal.as_ref().map(|host| host.view.clone())
        });
        let modal = modal.expect("the history modal opens for the selected action");

        modal.update(cx, |modal, cx| modal.open_integration(obs(), cx));

        let still_open = rig
            .view
            .read_with(cx, |view, _| view.history_modal.is_some());
        let screens = log.read_with(cx, |log, _| log.screens());
        assert!(!still_open, "the history modal stayed open");
        assert_eq!(screens, vec![Screen::BuiltinDetail(obs())]);
    }
}
