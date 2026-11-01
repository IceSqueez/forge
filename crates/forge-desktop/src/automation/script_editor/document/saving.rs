use super::super::console::{LogTag, push_line};
use super::{ScriptDocument, ScriptDocumentEvent};
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use forge_components::tr;
use forge_script::{format_script, validate_syntax};
use forge_storage::{ScriptRecord, ScriptRepo};
use gpui::Context;
use std::sync::Arc;
use time::OffsetDateTime;

enum ScriptSaveFailure {
    Syntax(String),
    Storage(RepoFailure),
}

impl ScriptDocument {
    pub(in super::super) fn save(&mut self, cx: &mut Context<Self>) {
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

        let repo = self.deps.backend.script_repo();
        let registry = Arc::clone(&self.deps.script_catalog);
        let bus = Arc::clone(&self.deps.bus);
        self.saving = true;
        async_bridge::run_async(
            &self.deps.rt_handle,
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
                cx.emit(ScriptDocumentEvent::Saved(record.id));
                if let Some(open) = self.open.as_mut()
                    && open.id == record.id
                {
                    open.original_body = record.body.clone();
                    open.original_contract = record.contract.clone();
                    open.record = record;
                }
                push_line(&self.console, LogTag::Ok, tr!("script_console_saved"), cx);
                if let Err(e) = reload {
                    push_line(
                        &self.console,
                        LogTag::Err,
                        tr!("script_console_reload_failed", error = e.text()),
                        cx,
                    );
                }
            }
            Err(ScriptSaveFailure::Syntax(e)) => {
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_syntax_error", error = e.as_str()),
                    cx,
                );
                push_line(
                    &self.console,
                    LogTag::Warn,
                    tr!("script_editor_save_blocked"),
                    cx,
                );
            }
            Err(ScriptSaveFailure::Storage(e)) => {
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_save_failed", error = e.text()),
                    cx,
                );
            }
        }
        cx.notify();
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
