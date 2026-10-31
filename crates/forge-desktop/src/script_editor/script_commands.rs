use super::console::LogTag;
use super::{OpenScript, PendingNav, ScriptEditorView, ScriptEntry};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use crate::script_lifecycle::ScriptLifecycle;
use forge_components::PushToast;
use forge_components::{ToastAction, ToastKind, tr};
use forge_script::{format_script, validate_syntax};
use forge_storage::{ScriptRecord, ScriptReferences, ScriptRepo};
use forge_types::{ScriptContract, ScriptId, VariantKind};
use gpui::{App, Context};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

const UNDO_TOAST_MS: u64 = 6000;

enum ScriptSaveFailure {
    Syntax(String),
    Storage(RepoFailure),
}

impl ScriptEditorView {
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        if self.saving || !self.current_dirty(cx) {
            return;
        }
        let Some(record) = self.open.as_ref().map(|o| o.record.clone()) else {
            return;
        };
        let body = self.code_input.read(cx).content().to_owned();
        let mut record = record;
        record.body = body.clone();
        record.last_modified = OffsetDateTime::now_utc();

        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let registry = Arc::clone(&self.script_registry);
        let bus = Arc::clone(&self.bus);
        self.saving = true;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let candidate = record.body.clone();
                tokio::task::spawn_blocking(move || validate_syntax(&candidate))
                    .await
                    .map_err(|e| ScriptSaveFailure::Storage(e.into()))?
                    .map_err(|e| ScriptSaveFailure::Syntax(e.to_string()))?;
                ScriptRepo::save(&*repo, record.clone())
                    .await
                    .map_err(|e| ScriptSaveFailure::Storage(e.into()))?;
                let reload = registry
                    .reload(record.clone(), bus.as_ref())
                    .await
                    .map_err(RepoFailure::from);
                Ok((record, reload))
            },
            |this, result, cx| this.apply_saved(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_saved(
        &mut self,
        result: Result<(ScriptRecord, Result<(), RepoFailure>), ScriptSaveFailure>,
        cx: &mut Context<Self>,
    ) {
        self.saving = false;
        match result {
            Ok((record, reload)) => {
                if let Some(entry) = self.scripts.iter_mut().find(|e| e.id == record.id) {
                    entry.status_ok = true;
                }
                if let Some(open) = self.open.as_mut()
                    && open.id == record.id
                {
                    open.original_body = record.body.clone();
                    open.original_contract = record.contract.clone();
                    open.record = record;
                }
                self.push_console(LogTag::Ok, tr!("script_console_saved"));
                if let Err(e) = reload {
                    self.push_console(
                        LogTag::Err,
                        tr!("script_console_reload_failed", error = e.text()),
                    );
                }
            }
            Err(ScriptSaveFailure::Syntax(e)) => {
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_syntax_error", error = e.as_str()),
                );
                self.push_console(LogTag::Warn, tr!("script_editor_save_blocked"));
            }
            Err(ScriptSaveFailure::Storage(e)) => {
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_save_failed", error = e.text()),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn new_script(&mut self, cx: &mut Context<Self>) {
        if self.current_dirty(cx) {
            self.pending_nav = Some(PendingNav::NewScript);
            cx.notify();
            return;
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        async_bridge::run_async(
            &self.rt_handle,
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
                let body = record.body.clone();
                self.scripts.push(ScriptEntry {
                    id: record.id,
                    name: record.name.clone(),
                    status_ok: true,
                    enabled: record.enabled,
                    linked: None,
                });
                let id = record.id;
                self.selected = Some(id);
                self.code_input.update(cx, |area, cx| {
                    area.set_content(body.clone(), cx);
                });
                self.recompute_diagnostics(&body, cx);
                self.open = Some(OpenScript {
                    id,
                    original_body: body,
                    original_contract: record.contract.clone(),
                    record,
                });
                self.reset_contract_editing();
                self.load_telemetry(id, cx);
            }
            Err(e) => {
                tracing::warn!(error = %e, "new script creation failed");
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_create_failed", error = e.text()),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn lifecycle(&self) -> ScriptLifecycle {
        ScriptLifecycle::new(
            Arc::clone(&self.backend) as Arc<dyn ScriptRepo>,
            Arc::clone(&self.script_registry),
            Arc::clone(&self.bus),
        )
    }

    pub(super) fn request_delete(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.pending_delete.request(id);
        self.delete_referrers = None;
        self.row_menu = None;
        cx.notify();
        let Some(name) = self.find_entry(id).map(|e| e.name.clone()) else {
            return;
        };
        let lifecycle = self.lifecycle();
        async_bridge::run_async(
            &self.rt_handle,
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

    pub(super) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        self.delete_referrers = None;
        let name = self
            .find_entry(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let lifecycle = self.lifecycle();
        async_bridge::run_async(
            &self.rt_handle,
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
                    self.open = None;
                    self.reset_contract_editing();
                    self.code_input.update(cx, |area, cx| {
                        area.set_content("", cx);
                    });
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
        cx.push_toast_full(
            ToastKind::Undo,
            tr!("script_editor_deleted_toast", name = name),
            None,
            Some(ToastAction::new(
                tr!("common_undo"),
                move |_window, app: &mut App| {
                    view.update(app, |this, cx| this.restore_archived(id, cx));
                },
            )),
            Duration::from_millis(UNDO_TOAST_MS),
        );
    }

    fn restore_archived(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        let lifecycle = self.lifecycle();
        async_bridge::run_async(
            &self.rt_handle,
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
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        async_bridge::run_async(
            &self.rt_handle,
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
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_update_failed", error = e.text()),
                );
                cx.notify();
            }
        }
    }

    pub(super) fn format(&mut self, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        let source = self.code_input.read(cx).content().to_owned();
        let formatted = format_script(&source);
        self.code_input
            .update(cx, |area, cx| area.replace_content_in_place(&formatted, cx));
    }
}
