use super::super::console::{LogTag, push_line};
use super::{ScriptEntry, ScriptList, ScriptListEvent};
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use forge_components::PushToast;
use forge_components::{ToastKind, tr};
use forge_storage::{ScriptRecord, ScriptReferences, ScriptRepo};
use forge_types::{ScriptContract, ScriptId, VariantKind};
use gpui::{App, Context};
use time::OffsetDateTime;

impl ScriptList {
    pub(in super::super) fn create_script(&mut self, cx: &mut Context<Self>) {
        let repo = self.deps.backend.script_repo();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                let now = OffsetDateTime::now_utc();
                let name = format!("script_{}", now.unix_timestamp());
                let body = "\"hello from forge\"".to_owned();
                let contract = ScriptContract {
                    inputs: Vec::new(),
                    returns: Some(VariantKind::String),
                };
                let record = ScriptRecord {
                    id: ScriptId::new(),
                    name,
                    body,
                    contract,
                    enabled: true,
                    created_at: now,
                    last_modified: now,
                };
                ScriptRepo::save(&*repo, record.clone())
                    .await
                    .map(|_| record)
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| this.apply_new_script(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_new_script(
        &mut self,
        result: Result<ScriptRecord, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(record) => {
                self.scripts.push(ScriptEntry {
                    id: record.id,
                    name: record.name.clone(),
                    status_ok: true,
                    enabled: record.enabled,
                    linked: None,
                });
                self.mark_selected(record.id, cx);
                cx.emit(ScriptListEvent::Created(record));
            }
            Err(e) => {
                tracing::warn!(error = %e, "new script creation failed");
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_create_failed", error = e.text()),
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(in super::super) fn request_delete(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        self.delete_referrers = None;
        self.row_menu = None;
        cx.notify();
        let Some(name) = self.find_entry(id).map(|e| e.name.clone()) else {
            return;
        };
        let lifecycle = self.deps.lifecycle();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { lifecycle.referrers(name).await },
            move |this, result, cx| this.apply_delete_referrers(id, result, cx),
            cx,
        );
    }

    fn apply_delete_referrers(
        &mut self,
        id: ScriptId,
        result: Result<ScriptReferences, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        if self.pending_delete.get() != Some(&id) {
            return;
        }
        match result {
            Ok(references) => self.delete_referrers = Some(references),
            Err(error) => {
                self.pending_delete.cancel();
                cx.push_toast(
                    ToastKind::Error,
                    tr!("script_editor_delete_failed", error = error.text()),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        self.delete_referrers = None;
        cx.notify();
    }

    pub(in super::super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        self.delete_referrers = None;
        let name = self
            .find_entry(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let lifecycle = self.deps.lifecycle();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { lifecycle.archive(id).await },
            move |this, result, cx| this.apply_deleted(id, name, result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_deleted(
        &mut self,
        id: ScriptId,
        name: String,
        result: Result<bool, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(archived) => {
                self.scripts.retain(|e| e.id != id);
                if self.selected == Some(id) {
                    self.selected = None;
                    cx.emit(ScriptListEvent::SelectionLost);
                }
                if archived {
                    self.raise_undo_toast(id, &name, cx);
                }
                self.load_scripts(cx);
            }
            Err(error) => {
                tracing::warn!(%error, "script archive failed");
                cx.push_toast(
                    ToastKind::Error,
                    tr!("script_editor_delete_failed", error = error.text()),
                );
                cx.notify();
            }
        }
    }

    fn raise_undo_toast(&self, id: ScriptId, name: &str, cx: &mut Context<Self>) {
        let view = cx.entity();
        cx.push_undo_toast(
            tr!("script_editor_deleted_toast", name = name),
            move |_window, app: &mut App| {
                view.update(app, |this, cx| this.restore_archived(id, cx));
            },
        );
    }

    fn restore_archived(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        let lifecycle = self.deps.lifecycle();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { lifecycle.restore(id).await },
            |this, result, cx| this.apply_restored(result, cx),
            cx,
        );
    }

    fn apply_restored(&mut self, result: Result<bool, RepoFailure>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            tracing::warn!(%error, "script restore failed");
            cx.push_toast(
                ToastKind::Error,
                tr!("script_editor_restore_failed", error = error.text()),
            );
        }
        self.load_scripts(cx);
    }

    pub(super) fn set_enabled(&mut self, id: ScriptId, enabled: bool, cx: &mut Context<Self>) {
        self.row_menu = None;
        cx.notify();
        let repo = self.deps.backend.script_repo();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                let Some(mut record) = ScriptRepo::get(&*repo, id)
                    .await
                    .map_err(RepoFailure::from)?
                else {
                    return Ok(false);
                };
                record.enabled = enabled;
                ScriptRepo::save(&*repo, record)
                    .await
                    .map(|()| true)
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| {
                let updated = result.and_then(|found| {
                    found
                        .then_some(())
                        .ok_or_else(|| RepoFailure::localized(tr!("script_console_missing")))
                });
                this.apply_enabled_set(updated, cx);
            },
            cx,
        );
    }

    fn apply_enabled_set(&mut self, result: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.load_scripts(cx),
            Err(e) => {
                tracing::warn!(error = %e, "script enable toggle failed");
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_update_failed", error = e.text()),
                    cx,
                );
                cx.notify();
            }
        }
    }
}
