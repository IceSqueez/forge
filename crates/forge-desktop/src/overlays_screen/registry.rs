use std::collections::HashMap;
use std::sync::Arc;

use forge_components::{ToastKind, tr};
use forge_overlay::MediaIssue;
use forge_storage::{OverlayDefinition, OverlayId};
use gpui::{Context, Pixels, Point};

use super::OverlaysView;
use super::code_pane::LeaveIntent;
use super::sound_choice;
use crate::async_bridge::{self, Loadable};
use crate::repo_failure::RepoFailure;
use crate::toasts::copy_to_clipboard;
use forge_components::PushToast;

#[derive(Default)]
pub(super) struct RegistryState {
    pub(super) overlays: Vec<OverlayDefinition>,
    pub(super) selected: Option<OverlayId>,
    pub(super) list_load: Loadable<()>,
    media_issues: HashMap<OverlayId, Vec<MediaIssue>>,
    pub(super) menu_open: Option<OverlayId>,
    pub(super) menu_click_pos: Option<Point<Pixels>>,
}

impl RegistryState {
    fn index_of(&self, id: &OverlayId) -> Option<usize> {
        self.overlays.iter().position(|item| &item.id == id)
    }

    fn selected_definition(&self) -> Option<&OverlayDefinition> {
        self.selected
            .as_ref()
            .and_then(|id| self.overlays.iter().find(|item| &item.id == id))
    }

    fn enabled_count(&self) -> usize {
        self.overlays.iter().filter(|item| item.enabled).count()
    }

    fn issues_of(&self, id: &OverlayId) -> &[MediaIssue] {
        self.media_issues
            .get(id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

impl OverlaysView {
    pub(super) fn index_of(&self, id: &OverlayId) -> Option<usize> {
        self.registry.index_of(id)
    }

    pub(super) fn selected_definition(&self) -> Option<&OverlayDefinition> {
        self.registry.selected_definition()
    }

    pub(super) fn enabled_count(&self) -> usize {
        self.registry.enabled_count()
    }

    pub(super) fn set_media_issues(
        &mut self,
        id: &OverlayId,
        issues: Vec<MediaIssue>,
        cx: &mut Context<Self>,
    ) {
        if issues.is_empty() {
            self.registry.media_issues.remove(id);
        } else {
            self.registry
                .media_issues
                .insert(id.clone(), issues.clone());
        }
        let Some(panel) = self
            .editor
            .panel
            .as_ref()
            .filter(|open| open.view.read(cx).overlay_id() == id)
            .map(|open| open.view.clone())
        else {
            return;
        };
        panel.update(cx, |panel, cx| panel.set_media_issues(issues, cx));
    }

    pub(super) fn issues_of(&self, id: &OverlayId) -> &[MediaIssue] {
        self.registry.issues_of(id)
    }

    pub(super) fn media_badge(&self, id: &OverlayId) -> Option<String> {
        sound_choice::badge_note(self.issues_of(id))
    }

    pub(super) fn load(&mut self, cx: &mut Context<Self>) {
        self.registry.list_load = Loadable::Loading;
        let repo = Arc::clone(&self.handles.repo);
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move { repo.list().await.map_err(RepoFailure::from) },
            |this, result, cx| this.apply_list(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_list(
        &mut self,
        result: Result<Vec<OverlayDefinition>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(rows) => {
                self.registry.list_load = Loadable::Loaded(());
                self.registry.overlays = rows;
                let live = &self.registry.overlays;
                self.registry
                    .media_issues
                    .retain(|id, _| live.iter().any(|item| &item.id == id));
                let selection_survived = self
                    .registry
                    .selected
                    .as_ref()
                    .is_some_and(|id| self.index_of(id).is_some());
                if !selection_survived {
                    self.registry.selected =
                        self.registry.overlays.first().map(|item| item.id.clone());
                    self.clear_test();
                }
                self.sync_panel(cx);
                self.sync_preview();
                self.sync_source(cx);
                self.sync_wiring(cx);
            }
            Err(message) => {
                if !self.registry.overlays.is_empty() {
                    self.report(&message.text(), cx);
                }
                self.registry.list_load = Loadable::Failed(message);
            }
        }
        cx.notify();
    }

    fn sync_wiring(&mut self, cx: &mut Context<Self>) {
        let selection = self.selected_definition().cloned();
        self.wiring
            .view
            .update(cx, |wiring, cx| wiring.focus_overlay(selection, cx));
    }

    pub(super) fn select(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        if self.registry.selected.as_ref() == Some(&id) {
            return;
        }
        if self.code_dirty(cx) {
            self.request_leave(LeaveIntent::Overlay(id), cx);
            return;
        }
        self.registry.selected = Some(id);
        self.clear_test();
        self.sync_panel(cx);
        self.sync_preview();
        self.sync_source(cx);
        self.sync_wiring(cx);
        cx.notify();
    }

    pub(super) fn toggle_enabled(&mut self, id: OverlayId, cx: &mut Context<Self>) {
        let Some(index) = self.index_of(&id) else {
            return;
        };
        let next = !self.registry.overlays[index].enabled;
        self.registry.overlays[index].enabled = next;
        cx.notify();

        let service = self.handles.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.handles.rt_handle,
            async move {
                service
                    .set_enabled(&target, next)
                    .await
                    .map_err(RepoFailure::from)
            },
            move |this, result: Result<bool, RepoFailure>, cx| {
                let failure = match result {
                    Ok(true) => None,
                    Ok(false) => Some(tr!("overlays_toast_missing")),
                    Err(failure) => Some(failure.text()),
                };
                let Some(message) = failure else {
                    return;
                };
                if let Some(index) = this.index_of(&id) {
                    this.registry.overlays[index].enabled = !next;
                }
                this.report(&message, cx);
            },
            cx,
        );
    }

    pub(super) fn copy_url(&mut self, id: &OverlayId, cx: &mut Context<Self>) {
        self.registry.menu_open = None;
        match self.overlay_url(id) {
            Some(url) => copy_to_clipboard(url, cx),
            None => cx.push_toast(ToastKind::Info, tr!("overlays_toast_url_unavailable")),
        }
        cx.notify();
    }

    pub(super) fn toggle_menu(
        &mut self,
        id: &OverlayId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.registry.menu_open = if self.registry.menu_open.as_ref() == Some(id) {
            None
        } else {
            self.registry.menu_click_pos = Some(position);
            Some(id.clone())
        };
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.registry.menu_open = None;
        cx.notify();
    }
}
