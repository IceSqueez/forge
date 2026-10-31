use super::{QuickActionSource, QuickActionsPanel, QuickActionsPanelEvent, accent_color};
use crate::async_bridge;
use crate::collection_manager::{CollectionManager, CollectionManagerEvent};
use crate::integration_quick_action_modal::{QuickActionModal, QuickActionModalEvent};
use crate::navigation::Screen;
use crate::repo_failure::RepoFailure;
use crate::run_history_modal::{RunHistoryDismissed, RunHistoryModal, RunHistoryOpenIntegration};
use forge_components::focus_inside_modal;
use forge_components::{ActivePresentation, InputEvent, PushToast, TextInput, ToastKind, tr};
use forge_platform_core::{BuiltinCollections, CollectionId, QuickAction, QuickActions};
use forge_runtime::{ActionEngineHandle, QuickActionOrigin};
use forge_types::SubActionStep;
use gpui::{Context, Entity, Rgba, Window, prelude::*};
use std::sync::Arc;

const HISTORY_LIMIT: u32 = 50;

pub(crate) fn enqueue_builtin_step<V: 'static>(
    origin: QuickActionOrigin,
    engine: ActionEngineHandle,
    rt_handle: &tokio::runtime::Handle,
    step: SubActionStep,
    settled: impl FnOnce(&mut V, Result<(), RepoFailure>, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) {
    async_bridge::run_async(
        rt_handle,
        async_bridge::run_quick_step(engine, step, origin),
        settled,
        cx,
    );
}

pub(crate) fn reachable_quick_actions(
    quick: &dyn QuickActions,
    collections: Option<&dyn BuiltinCollections>,
) -> Vec<QuickAction> {
    let declared: Vec<CollectionId> = collections
        .map(|capability| {
            capability
                .collections()
                .into_iter()
                .map(|metadata| metadata.id)
                .collect()
        })
        .unwrap_or_default();
    quick
        .actions()
        .into_iter()
        .filter(|action| {
            action
                .collection
                .as_ref()
                .is_none_or(|target| declared.contains(target))
        })
        .collect()
}

impl QuickActionsPanel {
    pub(crate) fn sync(&mut self, display_name: String, connected: bool, cx: &mut Context<Self>) {
        self.display_name = display_name;
        self.actions =
            reachable_quick_actions(&*self.source.quick, self.source.collections.as_deref());
        if let Some(manager) = self.modals.collection_manager.clone() {
            manager.update(cx, |manager, cx| manager.set_connected(connected, cx));
        }
        cx.notify();
    }

    pub(crate) fn rebind(&mut self, source: QuickActionSource, cx: &mut Context<Self>) {
        self.source = source;
        self.close_collection_manager();
        cx.notify();
    }

    pub(super) fn on_search(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.search.on_changed(event) {
            cx.notify();
        }
    }

    pub(crate) fn on_quick_action(
        &mut self,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.actions.get(idx) else {
            return;
        };
        if !action.is_runnable() {
            return;
        }
        if let Some(collection) = action.collection.clone() {
            let accent = accent_color(action.accent, &cx.palette());
            self.open_collection_manager(&collection, accent, cx);
            return;
        }
        let action = action.clone();
        let option_source = self.source.option_source.clone();
        let rt_handle = self.services.rt_handle.clone();
        let modal = cx.new(|cx| QuickActionModal::new(action, option_source, rt_handle, cx));
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus(window, cx))
        });
        self.modals._quick_action_sub =
            Some(cx.subscribe(&modal, Self::on_quick_action_modal_event));
        self.modals.quick_action = Some(modal);
        cx.notify();
    }

    fn on_quick_action_modal_event(
        &mut self,
        _modal: Entity<QuickActionModal>,
        event: &QuickActionModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            QuickActionModalEvent::Run { step, label } => {
                self.run_quick_action(step.clone(), label.clone(), cx);
                self.close_quick_action_modal(cx);
            }
            QuickActionModalEvent::Cancel => self.close_quick_action_modal(cx),
        }
    }

    fn run_quick_action(&mut self, step: SubActionStep, label: String, cx: &mut Context<Self>) {
        let toast_label = label.clone();
        let origin = QuickActionOrigin {
            builtin_id: self.source.id.as_str().to_owned(),
            label,
        };
        enqueue_builtin_step(
            origin,
            self.services.action_engine.clone(),
            &self.services.rt_handle,
            step,
            move |_panel: &mut Self, result, cx| {
                match result {
                    Ok(()) => cx.push_toast(
                        ToastKind::Success,
                        tr!("integration_quick_action_ran", label = toast_label),
                    ),
                    Err(err) => {
                        tracing::warn!(error = %err, "quick action failed");
                        cx.push_toast(ToastKind::Error, tr!("integration_quick_action_failed"));
                    }
                }
                cx.emit(QuickActionsPanelEvent::Ran);
            },
            cx,
        );
    }

    fn open_collection_manager(
        &mut self,
        collection: &CollectionId,
        accent: Rgba,
        cx: &mut Context<Self>,
    ) {
        let Some(capability) = self.source.collections.clone() else {
            return;
        };
        let Some(metadata) = capability
            .collections()
            .into_iter()
            .find(|metadata| &metadata.id == collection)
        else {
            return;
        };
        let rt_handle = self.services.rt_handle.clone();
        let manager =
            cx.new(|cx| CollectionManager::new(capability, metadata, accent, rt_handle, cx));
        self.modals._collection_manager_sub =
            Some(cx.subscribe(&manager, Self::on_collection_manager_event));
        self.modals.collection_manager = Some(manager);
        cx.notify();
    }

    fn on_collection_manager_event(
        &mut self,
        _manager: Entity<CollectionManager>,
        event: &CollectionManagerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            CollectionManagerEvent::Closed => {
                self.close_collection_manager();
                cx.notify();
            }
        }
    }

    pub(crate) fn close_collection_manager(&mut self) {
        self.modals.collection_manager = None;
        self.modals._collection_manager_sub = None;
    }

    fn close_quick_action_modal(&mut self, cx: &mut Context<Self>) {
        self.modals.quick_action = None;
        self.modals._quick_action_sub = None;
        cx.notify();
    }

    pub(crate) fn open_run_history(&mut self, cx: &mut Context<Self>) {
        let registry = Arc::clone(&self.services.trigger_registry);
        let modal = cx.new(|_| RunHistoryModal::new(self.display_name.clone(), registry));
        self.modals._history_subs = vec![
            cx.subscribe(&modal, Self::on_run_history_event),
            cx.subscribe(&modal, Self::on_run_history_open_integration),
        ];
        self.modals.history = Some(modal.clone());

        let history = Arc::clone(&self.services.history);
        let builtin_id = self.source.id.as_str().to_owned();
        async_bridge::run_async(
            &self.services.rt_handle,
            async move {
                history
                    .recent_for_builtin(&builtin_id, HISTORY_LIMIT)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |_panel: &mut Self, result, cx| match result {
                Ok(runs) => modal.update(cx, |modal, cx| modal.set_runs(runs, cx)),
                Err(message) => {
                    tracing::warn!(error = %message, "quick action run history load failed");
                    modal.update(cx, |modal, cx| {
                        modal.set_error(
                            tr!("action_editor_run_history_failed", error = message.text()),
                            cx,
                        );
                    });
                    cx.push_toast(ToastKind::Error, tr!("integration_run_history_failed"));
                }
            },
            cx,
        );
        cx.notify();
    }

    fn on_run_history_event(
        &mut self,
        _modal: Entity<RunHistoryModal>,
        _event: &RunHistoryDismissed,
        cx: &mut Context<Self>,
    ) {
        self.modals.history = None;
        self.modals._history_subs.clear();
        cx.notify();
    }

    fn on_run_history_open_integration(
        &mut self,
        _modal: Entity<RunHistoryModal>,
        event: &RunHistoryOpenIntegration,
        cx: &mut Context<Self>,
    ) {
        self.modals.history = None;
        self.modals._history_subs.clear();
        cx.emit(QuickActionsPanelEvent::Navigate(Screen::BuiltinDetail(
            event.0.clone(),
        )));
        cx.notify();
    }
}
