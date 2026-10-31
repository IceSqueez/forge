use super::console::LogTag;
use super::{OpenScript, PendingNav, ScriptEditorView, ScriptEntry, find_linked_action};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use forge_components::tr;
use forge_script::{EngineConfig, validate_syntax};
use forge_storage::{ScriptRecord, ScriptRepo, SettingsRepo};
use forge_types::ScriptId;
use gpui::{App, Context};
use std::sync::Arc;
use std::time::Duration;

const COMPILE_DEBOUNCE: Duration = Duration::from_millis(300);

impl ScriptEditorView {
    pub(super) fn load_engine_timeout(&mut self, cx: &mut Context<Self>) {
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { EngineConfig::read(settings.as_ref()).await.wall_time_ms },
            |this, wall_time_ms, cx| this.apply_engine_timeout(wall_time_ms, cx),
            cx,
        );
    }

    fn apply_engine_timeout(&mut self, wall_time_ms: u64, cx: &mut Context<Self>) {
        self.engine_timeout_ms = Some(wall_time_ms);
        cx.notify();
    }

    pub(super) fn find_entry(&self, id: ScriptId) -> Option<&ScriptEntry> {
        self.scripts.iter().find(|e| e.id == id)
    }

    pub(super) fn current_dirty(&self, cx: &App) -> bool {
        self.open.as_ref().is_some_and(|o| {
            self.code_input.read(cx).content() != o.original_body
                || o.record.contract != o.original_contract
        })
    }

    pub(super) fn load_scripts(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let action_repo = self.backend.action_repo();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let records = repo.list().await.map_err(RepoFailure::from)?;
                let actions = action_repo.list().await.map_err(RepoFailure::from)?;
                Ok(records
                    .into_iter()
                    .map(|r| ScriptEntry {
                        status_ok: validate_syntax(&r.body).is_ok(),
                        enabled: r.enabled,
                        linked: find_linked_action(&actions, &r.name),
                        id: r.id,
                        name: r.name,
                    })
                    .collect::<Vec<_>>())
            },
            |this, result, cx| this.apply_scripts_loaded(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_scripts_loaded(
        &mut self,
        result: Result<Vec<ScriptEntry>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        match result {
            Ok(entries) => {
                let first = entries.first().map(|e| e.id);
                self.scripts = entries;
                if let Some(id) = first
                    && self.open.is_none()
                {
                    self.open_script(id, cx);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "script list load failed");
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_load_failed", error = e.text()),
                );
            }
        }
        cx.notify();
    }

    fn open_script(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.list_keys.move_to(id);
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { ScriptRepo::get(&*repo, id).await.map_err(RepoFailure::from) },
            |this, result, cx| {
                let opened = result.and_then(|found| {
                    found.ok_or_else(|| RepoFailure::localized(tr!("script_console_missing")))
                });
                this.apply_script_opened(opened, cx);
            },
            cx,
        );
        cx.notify();
    }

    pub(super) fn apply_script_opened(
        &mut self,
        result: Result<ScriptRecord, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(record) => {
                let body = record.body.clone();
                self.code_input.update(cx, |area, cx| {
                    area.set_content(body.clone(), cx);
                });
                self.recompute_diagnostics(&body, cx);
                let id = record.id;
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
                tracing::warn!(error = %e, "script open failed");
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_open_failed", error = e.text()),
                );
            }
        }
        cx.notify();
    }

    pub(super) fn load_telemetry(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.telemetry = None;
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move { repo.telemetry(id).await.map_err(RepoFailure::from) },
            move |this, result, cx| {
                if !this.open.as_ref().is_some_and(|o| o.id == id) {
                    return;
                }
                match result {
                    Ok(telemetry) => this.telemetry = Some(telemetry),
                    Err(failure) => {
                        tracing::warn!(error = %failure, "script telemetry load failed");
                        this.push_console(
                            LogTag::Err,
                            tr!("script_editor_telemetry_failed", error = failure.text()),
                        );
                    }
                }
                cx.notify();
            },
            cx,
        );
    }

    pub(super) fn revert_current(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.record.contract = open.original_contract.clone();
        let original = open.original_body.clone();
        self.reset_contract_editing();
        self.code_input.update(cx, |area, cx| {
            area.set_content(original.clone(), cx);
        });
        self.recompute_diagnostics(&original, cx);
    }

    pub(super) fn confirm_discard(&mut self, cx: &mut Context<Self>) {
        let Some(nav) = self.pending_nav.take() else {
            return;
        };
        self.revert_current(cx);
        match nav {
            PendingNav::SelectScript(id) => self.open_script(id, cx),
            PendingNav::NewScript => self.new_script(cx),
        }
        cx.notify();
    }

    pub(super) fn cancel_discard(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        cx.notify();
    }

    pub(super) fn select(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            self.list_keys.move_to(id);
            cx.notify();
            return;
        }
        if self.current_dirty(cx) {
            self.pending_nav = Some(PendingNav::SelectScript(id));
            cx.notify();
            return;
        }
        self.open_script(id, cx);
    }

    pub(super) fn on_code_changed(&mut self, cx: &mut Context<Self>) {
        let content = self.code_input.read(cx).content().to_owned();
        self.refresh_annotations(&content, cx);
        self.schedule_compile_check(content, COMPILE_DEBOUNCE, cx);
        cx.notify();
    }
}
