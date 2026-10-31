use super::IntegrationDetail;
use crate::async_bridge;
use crate::builtin_sections::StepClick;
use crate::collection_manager::{CollectionManager, CollectionManagerEvent};
use crate::integration_quick_action_modal::{QuickActionModal, QuickActionModalEvent};
use crate::integration_quick_actions::accent_color;
use crate::navigation::Screen;
use crate::repo_failure::RepoFailure;
use crate::run_history_modal::{RunHistoryDismissed, RunHistoryModal, RunHistoryOpenIntegration};
use forge_components::focus_inside_modal;
use forge_components::{ActivePresentation, InputEvent, PushToast, TextInput, ToastKind, tr};
use forge_platform_core::{BuiltinCollections, CollectionId, QuickAction, QuickActions};
use forge_types::SubActionStep;
use gpui::{Context, Entity, Rgba, Window, prelude::*};
use std::sync::Arc;

const HISTORY_LIMIT: u32 = 50;

pub(super) fn reachable_quick_actions(
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

impl IntegrationDetail {
    pub(super) fn on_qa_search(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if self.qa_search.on_changed(event) {
            cx.notify();
        }
    }

    pub(crate) fn on_quick_action(
        &mut self,
        idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.quick_actions.get(idx) else {
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
        let option_source = self.option_source.clone();
        let rt_handle = self.rt_handle.clone();
        let modal = cx.new(|cx| QuickActionModal::new(action, option_source, rt_handle, cx));
        focus_inside_modal(window, cx, |window, cx| {
            modal.update(cx, |m, cx| m.focus(window, cx))
        });
        self._qa_modal_sub = Some(cx.subscribe(&modal, Self::on_quick_action_modal_event));
        self.quick_action_modal = Some(modal);
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

    fn enqueue_builtin_step(
        &mut self,
        step: SubActionStep,
        label: String,
        settled: impl FnOnce(&mut Self, Result<(), RepoFailure>, &mut Context<Self>) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let builtin_id = self.status.id().as_str().to_owned();
        let engine = self.action_engine.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async_bridge::run_quick_step(engine, step, builtin_id, label),
            settled,
            cx,
        );
    }

    fn run_quick_action(&mut self, step: SubActionStep, label: String, cx: &mut Context<Self>) {
        let toast_label = label.clone();
        self.enqueue_builtin_step(
            step,
            label,
            move |detail, result, cx| {
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
                detail.reload(cx);
            },
            cx,
        );
    }

    pub(super) fn run_row_step(&mut self, click: &StepClick, cx: &mut Context<Self>) {
        let row = click.row.to_string();
        let kind = click.step.kind_id.clone();
        if !self.row_steps_in_flight.try_begin(&row, &kind) {
            return;
        }
        cx.notify();
        self.enqueue_builtin_step(
            click.step.clone(),
            row.clone(),
            move |detail, result, cx| {
                detail.row_steps_in_flight.finish(&row, &kind);
                if let Err(err) = result {
                    tracing::warn!(error = %err, "content row action failed");
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("integration_row_action_failed", error = err.text()),
                    );
                }
                cx.notify();
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
        let Some(capability) = self.collections.clone() else {
            return;
        };
        let Some(metadata) = capability
            .collections()
            .into_iter()
            .find(|metadata| &metadata.id == collection)
        else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        let manager =
            cx.new(|cx| CollectionManager::new(capability, metadata, accent, rt_handle, cx));
        self._collection_manager_sub =
            Some(cx.subscribe(&manager, Self::on_collection_manager_event));
        self.collection_manager = Some(manager);
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

    pub(super) fn close_collection_manager(&mut self) {
        self.collection_manager = None;
        self._collection_manager_sub = None;
    }

    fn close_quick_action_modal(&mut self, cx: &mut Context<Self>) {
        self.quick_action_modal = None;
        self._qa_modal_sub = None;
        cx.notify();
    }

    pub(crate) fn open_run_history(&mut self, cx: &mut Context<Self>) {
        let registry = Arc::clone(&self.trigger_registry);
        let modal = cx.new(|_| RunHistoryModal::new(self.display_name.clone(), registry));
        self._history_modal_subs = vec![
            cx.subscribe(&modal, Self::on_run_history_event),
            cx.subscribe(&modal, Self::on_run_history_open_integration),
        ];
        self.history_modal = Some(modal.clone());

        let history = Arc::clone(&self.history);
        let builtin_id = self.status.id().as_str().to_owned();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                history
                    .recent_for_builtin(&builtin_id, HISTORY_LIMIT)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |_detail, result, cx| match result {
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
        self.history_modal = None;
        self._history_modal_subs.clear();
        cx.notify();
    }

    fn on_run_history_open_integration(
        &mut self,
        _modal: Entity<RunHistoryModal>,
        event: &RunHistoryOpenIntegration,
        cx: &mut Context<Self>,
    ) {
        self.history_modal = None;
        self._history_modal_subs.clear();
        self.navigate_to(Screen::BuiltinDetail(event.0.clone()), cx);
        cx.notify();
    }
}
