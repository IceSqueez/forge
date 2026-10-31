use super::ScriptEditorView;
use super::console::{ConsoleTab, LogTag};
use super::run_modal::{RunDraft, RunModal, RunModalEvent};
use crate::async_bridge;
use crate::repo_failure::RepoFailure;
use forge_components::tr;
use forge_events::EventPublisher;
use forge_runtime::{RuntimeScriptGlobals, SchedulingContext};
use forge_script::{RunResult, ScriptCapabilities, ScriptHost, run_inline};
use forge_storage::{ExecutionStatus, GlobalsRepo, ScriptRepo, SettingsRepo};
use forge_types::{ArgStack, ScriptId, VariantKind};
use gpui::prelude::*;
use gpui::{Context, Entity, SharedString};
use std::sync::Arc;
use time::OffsetDateTime;

fn format_run_stats(duration_ms: f64, error_count: usize) -> String {
    let duration = format!("{duration_ms:.2}");
    tr!(
        "script_console_run_stats",
        duration = duration.as_str(),
        errors = i64::try_from(error_count).unwrap_or(i64::MAX)
    )
}

impl ScriptEditorView {
    pub(super) fn run(&mut self, cx: &mut Context<Self>) {
        if self.run_in_flight {
            return;
        }
        let Some((script_id, name, contract)) = self
            .open
            .as_ref()
            .map(|o| (o.id, o.record.name.clone(), o.record.contract.clone()))
        else {
            return;
        };
        let body = self.code_input.read(cx).content().to_owned();
        if contract.inputs.is_empty() {
            self.push_console(
                LogTag::Run,
                tr!("script_console_running", name = name.as_str()),
            );
            self.console_tab = ConsoleTab::Output;
            self.console_collapsed = false;
            self.run_inline_exec(body, ArgStack::new(), script_id, cx);
            cx.notify();
        } else {
            let fields: Vec<(String, VariantKind)> = contract
                .inputs
                .iter()
                .map(|i| (i.name.clone(), i.kind))
                .collect();
            let title: SharedString =
                tr!("script_editor_run_modal_title", name = name.as_str()).into();
            let modal = cx.new(|cx| RunModal::new(title, script_id, name, fields, cx));
            self._run_sub = Some(cx.subscribe(&modal, Self::on_run_event));
            self.run_modal = Some(modal);
            cx.notify();
        }
    }

    fn run_inline_exec(
        &mut self,
        body: String,
        args: ArgStack,
        script_id: ScriptId,
        cx: &mut Context<Self>,
    ) {
        self.run_in_flight = true;
        let contract = self
            .open
            .as_ref()
            .map(|o| o.record.contract.clone())
            .unwrap_or_default();
        let host = ScriptHost {
            globals: Arc::new(RuntimeScriptGlobals::new(
                Arc::clone(&self.backend) as Arc<dyn GlobalsRepo>,
                Arc::clone(&self.bus) as Arc<dyn EventPublisher>,
            )),
            settings: Arc::clone(&self.backend) as Arc<dyn SettingsRepo>,
            bus: Arc::clone(&self.bus) as Arc<dyn EventPublisher>,
            capabilities: ScriptCapabilities {
                speak: self.script_registry.speak_requester(),
                integrations: Some(Arc::clone(&self.integrations)),
                latest_values: self.script_registry.latest_values(),
                scheduler: self
                    .script_registry
                    .action_scheduler(SchedulingContext::outside_any_run(args.clone())),
            },
        };
        let scripts = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let started_at = OffsetDateTime::now_utc();
                let result = run_inline(body, contract, args, host, script_id)
                    .await
                    .map_err(RepoFailure::from);
                if let Ok(r) = &result {
                    let status = if r.error_count == 0 {
                        ExecutionStatus::Success
                    } else {
                        ExecutionStatus::Error
                    };
                    if let Err(error) = scripts
                        .record_execution(script_id, started_at, r.duration_ms as u64, status)
                        .await
                    {
                        tracing::warn!(%error, "script execution could not be recorded");
                    }
                }
                result
            },
            |this, result, cx| this.apply_run_finished(result, cx),
            cx,
        );
    }

    pub(super) fn apply_run_finished(
        &mut self,
        result: Result<RunResult, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.run_in_flight = false;
        match result {
            Ok(r) => {
                self.run_modal = None;
                self._run_sub = None;
                self.push_console(
                    LogTag::Ok,
                    tr!("script_console_returned", value = r.output_display.as_str()),
                );
                self.push_console(
                    LogTag::Stats,
                    format_run_stats(r.duration_ms, r.error_count),
                );
                if let Some(id) = self.open.as_ref().map(|o| o.id) {
                    self.load_telemetry(id, cx);
                }
            }
            Err(e) => {
                if let Some(modal) = self.run_modal.as_ref() {
                    modal.update(cx, |m, cx| {
                        m.running = false;
                        m.error = Some(e.text().into());
                        cx.notify();
                    });
                }
                self.push_console(
                    LogTag::Err,
                    tr!("script_console_run_failed", error = e.text()),
                );
            }
        }
        cx.notify();
    }

    fn on_run_event(
        &mut self,
        _modal: Entity<RunModal>,
        event: &RunModalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RunModalEvent::Submit(draft) => self.run_with_inputs(draft, cx),
            RunModalEvent::Cancel => self.cancel_run(cx),
        }
    }

    fn run_with_inputs(&mut self, draft: &RunDraft, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        let script_id = draft.script_id;
        let name = draft.script_name.clone();
        let args = draft.args.clone();
        let body = self.code_input.read(cx).content().to_owned();
        if let Some(modal) = self.run_modal.as_ref() {
            modal.update(cx, |m, cx| {
                m.running = true;
                m.error = None;
                cx.notify();
            });
        }
        self.push_console(
            LogTag::Run,
            tr!("script_console_running_with_inputs", name = name.as_str()),
        );
        self.console_tab = ConsoleTab::Output;
        self.console_collapsed = false;
        self.run_inline_exec(body, args, script_id, cx);
        cx.notify();
    }

    fn cancel_run(&mut self, cx: &mut Context<Self>) {
        self.run_modal = None;
        self._run_sub = None;
        cx.notify();
    }
}
