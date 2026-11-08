use super::super::console::{LogTag, push_line};
use super::ScriptDocument;
use super::run_modal::{RunDraft, RunModal, RunModalEvent};
use crate::app::async_bridge;
use crate::shared::repo_failure::RepoFailure;
use forge_components::tr;
use forge_events::EventPublisher;
use forge_runtime::{RuntimeScriptGlobals, SchedulingContext};
use forge_script::{RunResult, ScriptCapabilities, ScriptHost, run_inline};
use forge_types::{ArgStack, ScriptId, VariantKind};
use gpui::prelude::*;
use gpui::{Context, Entity, SharedString};
use std::sync::Arc;

fn format_run_stats(duration_ms: f64, error_count: usize) -> String {
    let duration = format!("{duration_ms:.2}");
    tr!(
        "script_console_run_stats",
        duration = duration.as_str(),
        errors = i64::try_from(error_count).unwrap_or(i64::MAX)
    )
}

impl ScriptDocument {
    pub(in super::super) fn run(&mut self, cx: &mut Context<Self>) {
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
            push_line(
                &self.console,
                LogTag::Run,
                tr!("script_console_running", name = name.as_str()),
                cx,
            );
            self.console
                .update(cx, |console, cx| console.reveal_output(cx));
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
                self.deps.backend.globals_repo(),
                Arc::clone(&self.deps.bus) as Arc<dyn EventPublisher>,
            )),
            settings: self.deps.backend.settings_repo(),
            bus: Arc::clone(&self.deps.bus) as Arc<dyn EventPublisher>,
            capabilities: ScriptCapabilities {
                speak: self.deps.script_catalog.speak_requester(),
                integrations: Some(Arc::clone(&self.deps.integrations)),
                latest_values: self.deps.script_catalog.latest_values(),
                scheduler: self
                    .deps
                    .script_catalog
                    .action_scheduler(SchedulingContext::outside_any_run(args.clone())),
            },
        };
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                run_inline(body, contract, args, host, script_id)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| this.apply_run_finished(result, cx),
            cx,
        );
    }

    pub(in super::super) fn apply_run_finished(
        &mut self,
        result: Result<RunResult, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        self.run_in_flight = false;
        match result {
            Ok(r) => {
                self.run_modal = None;
                self._run_sub = None;
                let value = r
                    .output_display
                    .unwrap_or_else(|| tr!("script_console_no_value"));
                push_line(
                    &self.console,
                    LogTag::Ok,
                    tr!("script_console_returned", value = value.as_str()),
                    cx,
                );
                push_line(
                    &self.console,
                    LogTag::Stats,
                    format_run_stats(r.duration_ms, r.error_count),
                    cx,
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
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_run_failed", error = e.text()),
                    cx,
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
        push_line(
            &self.console,
            LogTag::Run,
            tr!("script_console_running_with_inputs", name = name.as_str()),
            cx,
        );
        self.console
            .update(cx, |console, cx| console.reveal_output(cx));
        self.run_inline_exec(body, args, script_id, cx);
        cx.notify();
    }

    fn cancel_run(&mut self, cx: &mut Context<Self>) {
        self.run_modal = None;
        self._run_sub = None;
        cx.notify();
    }
}
