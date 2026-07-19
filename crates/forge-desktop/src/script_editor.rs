use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density,
    FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize, OverlayPosition, Radius,
    ResizeEdge, ResizeRange, Spacing, TextArea, TextInput, badge, breadcrumb, confirm_modal,
    fmt_relative_time, ghost_button, hover_reveal, icon, install_resize, modal, overlay,
    primary_button, radius, spacing, status_dot, tr, with_alpha,
};
use forge_events::{Event, EventPublisher, EventsError};
use forge_runtime::{EventBus, ScriptRegistry};
use forge_script::contract::collect_annotation_diagnostics;
use forge_script::{
    MethodDescriptor, RunResult, content_hash, format_script, parse_contract, run_inline,
    validate_syntax,
};
use forge_storage::{DataProvider, GlobalsRepo, ScriptRecord, ScriptRepo, SettingsRepo};
use forge_types::{
    Action, ActionId, ArgStack, ScriptContract, ScriptId, ScriptInput, Variant, VariantKind,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, MouseButton,
    MouseDownEvent, Pixels, Rgba, SharedString, Subscription, Window, div, prelude::*, px,
};
use time::OffsetDateTime;

use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const LEFT_PANE_W: Pixels = px(200.0);
const LIST_MIN_W: Pixels = px(180.0);
const LIST_MAX_W: Pixels = px(360.0);
const STRIPE_W: Pixels = px(2.0);
const RIGHT_PANE_W: Pixels = px(220.0);
const DETAILS_PANE_W: Pixels = px(232.0);
const DETAILS_MIN_W: Pixels = px(200.0);
const DETAILS_MAX_W: Pixels = px(420.0);
const CONSOLE_INIT_H: Pixels = px(160.0);
const CONSOLE_MIN_H: Pixels = px(80.0);
const CONSOLE_MAX_H: Pixels = px(400.0);
const DETAILS_PANE_PAD: Pixels = px(14.0);
const GLYPH_PIN: Pixels = px(12.0);
const GLYPH_RETURNS: Pixels = px(11.0);

const CODE_LINE_H_PX: f32 = 18.0;
const CODE_PAD_V_PX: f32 = 6.0;

const DIVIDER_W: Pixels = px(0.5);
const DIVIDER_H: Pixels = px(16.0);

const FILE_BAR_DIRTY_DOT: Pixels = px(5.0);
const FILE_BAR_STATUS_DOT: Pixels = px(6.0);

const GLYPH_RUN: Pixels = px(11.0);
const GLYPH_TOOLBAR: Pixels = px(13.0);
const GLYPH_STATUS: Pixels = px(12.0);
const GLYPH_FILE: Pixels = px(12.0);
const GLYPH_TAB: Pixels = px(12.0);
const GLYPH_ACTION: Pixels = px(12.0);

fn code_field_height(content: &str) -> Pixels {
    let lines = content.lines().count().max(1) as f32;
    px(lines * CODE_LINE_H_PX + CODE_PAD_V_PX * 2.0)
}

fn now_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second())
}

fn format_run_stats(duration_ms: f64, error_count: usize) -> String {
    format!("executed in {duration_ms:.2}ms · {error_count} errors")
}

fn parse_input_to_variant(name: &str, kind: VariantKind, raw: &str) -> Result<Variant, String> {
    match kind {
        VariantKind::Int => raw
            .trim()
            .parse::<i64>()
            .map(Variant::Int)
            .map_err(|_| format!("`{name}` must be an integer")),
        VariantKind::Float => raw
            .trim()
            .parse::<f64>()
            .map_err(|_| format!("`{name}` must be a float"))
            .and_then(|f| Variant::float(f).map_err(|e| e.to_string())),
        VariantKind::Bool => match raw.trim() {
            "true" => Ok(Variant::Bool(true)),
            "false" => Ok(Variant::Bool(false)),
            _ => Err(format!("`{name}` must be `true` or `false`")),
        },
        VariantKind::String => Ok(Variant::String(raw.to_owned())),
        other => Err(format!(
            "`{name}`: {other:?} inputs not supported in this run modal"
        )),
    }
}

struct ScriptsListResizeDrag;
struct ScriptDetailsResizeDrag;
struct ConsoleResizeDrag;

struct LinkedAction {
    id: ActionId,
    name: String,
}

struct ScriptEntry {
    id: ScriptId,
    name: String,
    status_ok: bool,
    linked: Option<LinkedAction>,
}

fn find_linked_action(actions: &[Action], script_name: &str) -> Option<LinkedAction> {
    actions.iter().find_map(|action| {
        let links = action.sub_actions.iter().any(|step| {
            step.kind_id == "script.run.named"
                && step.config.get("script_name").and_then(|v| v.as_str()) == Some(script_name)
        });
        links.then(|| LinkedAction {
            id: action.id,
            name: action.name.clone(),
        })
    })
}

struct OpenScript {
    id: ScriptId,
    record: ScriptRecord,
    original_body: String,
}

struct RenameState {
    target: ScriptId,
    input: Entity<TextInput>,
    _sub: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConsoleTab {
    Output,
    Problems,
}

#[derive(Clone, Copy)]
enum LogTag {
    Run,
    Ok,
    Stats,
    Log,
    Warn,
    Err,
}

impl LogTag {
    fn label(self) -> &'static str {
        match self {
            LogTag::Run => "[run]",
            LogTag::Ok => "[ok]",
            LogTag::Stats => "[stats]",
            LogTag::Log => "[log]",
            LogTag::Warn => "[warn]",
            LogTag::Err => "[error]",
        }
    }

    fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            LogTag::Run => palette.info,
            LogTag::Ok => palette.success,
            LogTag::Stats => palette.brand,
            LogTag::Log => palette.text_secondary,
            LogTag::Warn => palette.warning,
            LogTag::Err => palette.random,
        }
    }
}

struct ConsoleLine {
    time: SharedString,
    tag: LogTag,
    text: SharedString,
}

enum PendingNav {
    SelectScript(ScriptId),
    NewScript,
}

struct RunInput {
    name: SharedString,
    label: SharedString,
    kind: VariantKind,
    input: Entity<TextInput>,
    _sub: Subscription,
}

struct RunModalState {
    title: SharedString,
    script_id: ScriptId,
    script_name: String,
    inputs: Vec<RunInput>,
    error: Option<SharedString>,
    running: bool,
}

/// `None` = type-check passed; `Some(n)` = error count.
type TypeCheck = Option<u32>;

pub struct ScriptEditorView {
    backend: Arc<dyn DataProvider>,
    script_registry: Arc<ScriptRegistry>,
    bus: Arc<EventBus>,
    rt_handle: tokio::runtime::Handle,

    scripts: Vec<ScriptEntry>,
    selected: Option<ScriptId>,
    open: Option<OpenScript>,
    loading: bool,

    list_width: Pixels,
    details_width: Pixels,
    console_height: Pixels,

    search: Entity<TextInput>,
    _search_sub: Subscription,

    rename: Option<RenameState>,
    pending_delete: Option<ScriptId>,
    pending_nav: Option<PendingNav>,

    code_input: Entity<TextArea>,
    _code_sub: Subscription,

    console: Vec<ConsoleLine>,
    console_tab: ConsoleTab,
    console_collapsed: bool,
    problems: Vec<SharedString>,
    problem_lines: Vec<usize>,
    type_check: TypeCheck,

    api_docs_open: bool,
    api_search: Entity<TextInput>,
    _api_search_sub: Subscription,

    run_modal: Option<RunModalState>,
}

impl EventEmitter<NavRequested> for ScriptEditorView {}

impl ScriptEditorView {
    pub fn new(
        backend: Arc<dyn DataProvider>,
        script_registry: Arc<ScriptRegistry>,
        bus: Arc<EventBus>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();

        let code_input = cx.new(|cx| {
            TextArea::new("// write your rhai script", cx)
                .with_palette(palette)
                .mono()
                .rhai_highlight()
                .with_gutter()
                .with_font_size(FONT_XS)
                .with_height(code_field_height(""))
        });
        let code_sub = cx.subscribe(&code_input, |this, _area, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.on_code_changed(cx);
            }
        });

        let search = cx.new(|cx| {
            TextInput::new(tr!("script_editor_search_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .leading_icon(Icon::Search, palette.text_faint)
                .on_surface()
                .static_chrome(palette.surface_overlay, Radius::Sm)
        });
        let search_sub = cx.subscribe(&search, |_this, _f, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        let api_search = cx.new(|cx| {
            TextInput::new(tr!("script_editor_api_search_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .leading_icon(Icon::Search, palette.text_faint)
                .on_surface()
                .static_chrome(palette.surface_overlay, Radius::Sm)
        });
        let api_search_sub = cx.subscribe(&api_search, |_this, _f, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                cx.notify();
            }
        });

        let mut view = Self {
            backend,
            script_registry,
            bus,
            rt_handle,
            scripts: Vec::new(),
            selected: None,
            open: None,
            loading: false,
            list_width: LEFT_PANE_W,
            details_width: DETAILS_PANE_W,
            console_height: CONSOLE_INIT_H,
            search,
            _search_sub: search_sub,
            rename: None,
            pending_delete: None,
            pending_nav: None,
            code_input,
            _code_sub: code_sub,
            console: Vec::new(),
            console_tab: ConsoleTab::Output,
            console_collapsed: false,
            problems: Vec::new(),
            problem_lines: Vec::new(),
            type_check: None,
            api_docs_open: false,
            api_search,
            _api_search_sub: api_search_sub,
            run_modal: None,
        };
        view.start_log_bridge(cx);
        view.load_scripts(cx);
        view
    }

    fn start_log_bridge(&self, cx: &mut Context<Self>) {
        let bus = Arc::clone(&self.bus);
        cx.spawn(async move |this, cx| {
            let mut subscription = bus.subscribe();
            loop {
                match subscription.recv().await {
                    Ok(event) => {
                        if event.kind == "script.log"
                            && this
                                .update(cx, |this, cx| this.on_script_log(&event, cx))
                                .is_err()
                        {
                            break;
                        }
                    }
                    Err(EventsError::LaggingReceiver) => {}
                    Err(_) => break,
                }
            }
        })
        .detach();
    }

    fn on_script_log(&mut self, event: &Event, cx: &mut Context<Self>) {
        let Some(open_id) = self.open.as_ref().map(|o| o.id) else {
            return;
        };
        let Some(sid) = event.payload.get("script_id").and_then(|v| v.as_str()) else {
            return;
        };
        if sid != open_id.to_string().as_str() {
            return;
        }
        let level = event
            .payload
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("info");
        let message = event
            .payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let tag = match level {
            "error" => LogTag::Err,
            "warn" => LogTag::Warn,
            _ => LogTag::Log,
        };
        self.push_console(tag, message.to_owned());
        cx.notify();
    }

    fn find_entry(&self, id: ScriptId) -> Option<&ScriptEntry> {
        self.scripts.iter().find(|e| e.id == id)
    }

    fn push_console(&mut self, tag: LogTag, text: impl Into<SharedString>) {
        self.console.push(ConsoleLine {
            time: now_timestamp().into(),
            tag,
            text: text.into(),
        });
    }

    fn recompute_diagnostics(&mut self, body: &str, cx: &mut Context<Self>) {
        let diags = collect_annotation_diagnostics(body);
        self.type_check = if diags.is_empty() {
            None
        } else {
            Some(diags.len() as u32)
        };
        self.problem_lines = diags.iter().map(|d| d.line).collect();
        self.problems = diags
            .into_iter()
            .map(|d| SharedString::from(format!("Ln {} · {}", d.line + 1, d.message)))
            .collect();
        let marks = self.problem_lines.clone();
        self.code_input
            .update(cx, |area, cx| area.set_gutter_marks(marks, cx));
    }

    fn current_dirty(&self, cx: &App) -> bool {
        self.open
            .as_ref()
            .is_some_and(|o| self.code_input.read(cx).content() != o.original_body)
    }

    fn load_scripts(&mut self, cx: &mut Context<Self>) {
        self.loading = true;
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let action_repo = self.backend.action_repo();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = match repo.list().await {
                Ok(records) => {
                    let actions = action_repo.list().await.unwrap_or_default();
                    let entries = records
                        .into_iter()
                        .map(|r| ScriptEntry {
                            status_ok: collect_annotation_diagnostics(&r.body).is_empty(),
                            linked: find_linked_action(&actions, &r.name),
                            id: r.id,
                            name: r.name,
                        })
                        .collect::<Vec<_>>();
                    Ok(entries)
                }
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_scripts_loaded(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_scripts_loaded(
        &mut self,
        result: Result<Vec<ScriptEntry>, String>,
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
                self.push_console(LogTag::Err, format!("Could not load scripts: {e}"));
            }
        }
        cx.notify();
    }

    fn open_script(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = ScriptRepo::get(&*repo, id)
                .await
                .map_err(|e| e.to_string())
                .and_then(|opt| opt.ok_or_else(|| format!("script {id} not found")));
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_script_opened(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_script_opened(
        &mut self,
        result: Result<ScriptRecord, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(record) => {
                let body = record.body.clone();
                let height = code_field_height(&body);
                self.code_input.update(cx, |area, cx| {
                    area.set_content(body.clone(), cx);
                    area.set_height(height, cx);
                });
                self.recompute_diagnostics(&body, cx);
                self.open = Some(OpenScript {
                    id: record.id,
                    original_body: body,
                    record,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "script open failed");
                self.push_console(LogTag::Err, format!("Could not open script: {e}"));
            }
        }
        cx.notify();
    }

    fn revert_current(&mut self, cx: &mut Context<Self>) {
        let Some(original) = self.open.as_ref().map(|o| o.original_body.clone()) else {
            return;
        };
        let height = code_field_height(&original);
        self.code_input.update(cx, |area, cx| {
            area.set_content(original.clone(), cx);
            area.set_height(height, cx);
        });
        self.recompute_diagnostics(&original, cx);
    }

    fn confirm_discard(&mut self, cx: &mut Context<Self>) {
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

    fn cancel_discard(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        cx.notify();
    }

    fn select(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        if self.current_dirty(cx) {
            self.pending_nav = Some(PendingNav::SelectScript(id));
            cx.notify();
            return;
        }
        self.open_script(id, cx);
    }

    fn on_code_changed(&mut self, cx: &mut Context<Self>) {
        let content = self.code_input.read(cx).content().to_owned();
        let height = code_field_height(&content);
        self.code_input
            .update(cx, |area, cx| area.set_height(height, cx));
        self.recompute_diagnostics(&content, cx);
        if let Some(open) = self.open.as_mut() {
            open.record.contract = parse_contract(&content).unwrap_or_default();
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if !self.current_dirty(cx) {
            return;
        }
        let Some(record) = self.open.as_ref().map(|o| o.record.clone()) else {
            return;
        };
        let body = self.code_input.read(cx).content().to_owned();
        let contract = match parse_contract(&body) {
            Ok(c) => c,
            Err(e) => {
                self.push_console(LogTag::Err, format!("contract parse error: {e}"));
                cx.notify();
                return;
            }
        };
        if let Err(e) = validate_syntax(&body) {
            self.push_console(LogTag::Err, format!("syntax error: {e}"));
            self.push_console(LogTag::Warn, tr!("script_editor_save_blocked"));
            cx.notify();
            return;
        }

        let mut record = record;
        record.body = body.clone();
        record.body_hash = content_hash(&body);
        record.contract = contract;
        record.last_modified = OffsetDateTime::now_utc();

        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let registry = Arc::clone(&self.script_registry);
        let bus = Arc::clone(&self.bus);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let saved = ScriptRepo::save(&*repo, record.clone())
                .await
                .map_err(|e| e.to_string());
            let outcome = match saved {
                Ok(()) => {
                    let reload = registry
                        .reload(record.clone(), bus.as_ref())
                        .await
                        .map_err(|e| e.to_string());
                    Ok((record, reload))
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_saved(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    #[allow(clippy::type_complexity)]
    fn apply_saved(
        &mut self,
        result: Result<(ScriptRecord, Result<(), String>), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((record, reload)) => {
                if let Some(open) = self.open.as_mut()
                    && open.id == record.id
                {
                    open.original_body = record.body.clone();
                    open.record = record;
                }
                self.push_console(LogTag::Ok, "script saved");
                if let Err(e) = reload {
                    self.push_console(LogTag::Err, format!("hot-reload failed: {e}"));
                }
            }
            Err(e) => {
                self.push_console(LogTag::Err, format!("save failed: {e}"));
            }
        }
        cx.notify();
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let Some((script_id, name)) = self.open.as_ref().map(|o| (o.id, o.record.name.clone()))
        else {
            return;
        };
        let body = self.code_input.read(cx).content().to_owned();
        let contract = parse_contract(&body).unwrap_or_default();
        if contract.inputs.is_empty() {
            self.push_console(LogTag::Run, format!("running {name}"));
            self.console_tab = ConsoleTab::Output;
            self.console_collapsed = false;
            self.run_inline_exec(body, ArgStack::new(), script_id, cx);
            cx.notify();
        } else {
            let palette = cx.palette();
            let fields: Vec<(String, VariantKind)> = contract
                .inputs
                .iter()
                .map(|i| (i.name.clone(), i.kind))
                .collect();
            let inputs = fields
                .iter()
                .map(|(n, k)| self.build_run_input(n, *k, palette, cx))
                .collect();
            self.run_modal = Some(RunModalState {
                title: tr!("script_editor_run_modal_title", name = name.as_str()).into(),
                script_id,
                script_name: name,
                inputs,
                error: None,
                running: false,
            });
            cx.notify();
        }
    }

    fn build_run_input(
        &self,
        name: &str,
        kind: VariantKind,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> RunInput {
        let label = kind.label().to_lowercase();
        let placeholder = tr!(
            "script_editor_run_input_placeholder",
            label = label.as_str()
        );
        let input = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .with_font_size(FONT_SM)
                .on_surface()
                .static_chrome(palette.border_input, Radius::Sm)
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event
                && let Some(modal) = this.run_modal.as_mut()
            {
                modal.error = None;
                cx.notify();
            }
        });
        RunInput {
            name: name.to_owned().into(),
            label: label.into(),
            kind,
            input,
            _sub: sub,
        }
    }

    fn run_inline_exec(
        &mut self,
        body: String,
        args: ArgStack,
        script_id: ScriptId,
        cx: &mut Context<Self>,
    ) {
        let globals = Arc::clone(&self.backend) as Arc<dyn GlobalsRepo>;
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let publisher = Arc::clone(&self.bus) as Arc<dyn EventPublisher>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = run_inline(body, args, globals, settings, publisher, script_id)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_run_finished(result, cx));
            }
        })
        .detach();
    }

    fn apply_run_finished(&mut self, result: Result<RunResult, String>, cx: &mut Context<Self>) {
        match result {
            Ok(r) => {
                self.run_modal = None;
                self.push_console(LogTag::Ok, format!("returned: {}", r.output_display));
                self.push_console(
                    LogTag::Stats,
                    format_run_stats(r.duration_ms, r.error_count),
                );
            }
            Err(e) => {
                if let Some(modal) = self.run_modal.as_mut() {
                    modal.running = false;
                    modal.error = Some(e.clone().into());
                }
                self.push_console(LogTag::Err, format!("run error: {e}"));
            }
        }
        cx.notify();
    }

    fn submit_run(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.run_modal.as_ref() else {
            return;
        };
        let script_id = modal.script_id;
        let name = modal.script_name.clone();
        let raws: Vec<(String, VariantKind, String)> = modal
            .inputs
            .iter()
            .map(|f| {
                (
                    f.name.to_string(),
                    f.kind,
                    f.input.read(cx).content().trim().to_owned(),
                )
            })
            .collect();

        let mut args = ArgStack::new();
        for (fname, kind, raw) in &raws {
            match parse_input_to_variant(fname, *kind, raw) {
                Ok(v) => args = args.set(fname.clone(), v),
                Err(e) => {
                    if let Some(modal) = self.run_modal.as_mut() {
                        modal.error = Some(e.into());
                    }
                    cx.notify();
                    return;
                }
            }
        }

        if self.open.is_none() {
            return;
        }
        let body = self.code_input.read(cx).content().to_owned();
        if let Some(modal) = self.run_modal.as_mut() {
            modal.running = true;
            modal.error = None;
        }
        self.push_console(LogTag::Run, format!("running {name} with inputs"));
        self.console_tab = ConsoleTab::Output;
        self.console_collapsed = false;
        self.run_inline_exec(body, args, script_id, cx);
        cx.notify();
    }

    fn cancel_run(&mut self, cx: &mut Context<Self>) {
        self.run_modal = None;
        cx.notify();
    }

    fn new_script(&mut self, cx: &mut Context<Self>) {
        if self.current_dirty(cx) {
            self.pending_nav = Some(PendingNav::NewScript);
            cx.notify();
            return;
        }
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let now = OffsetDateTime::now_utc();
            let name = format!("script_{}", now.unix_timestamp());
            let body = "// @return string\n\n\"hello from forge\"".to_owned();
            let record = ScriptRecord {
                id: ScriptId::new(),
                name,
                body_hash: content_hash(&body),
                body,
                contract: ScriptContract::default(),
                enabled: true,
                created_at: now,
                last_modified: now,
            };
            let result = ScriptRepo::save(&*repo, record.clone())
                .await
                .map(|_| record)
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_new_script(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_new_script(&mut self, result: Result<ScriptRecord, String>, cx: &mut Context<Self>) {
        match result {
            Ok(record) => {
                let body = record.body.clone();
                self.scripts.push(ScriptEntry {
                    id: record.id,
                    name: record.name.clone(),
                    status_ok: collect_annotation_diagnostics(&body).is_empty(),
                    linked: None,
                });
                let id = record.id;
                self.selected = Some(id);
                let height = code_field_height(&body);
                self.code_input.update(cx, |area, cx| {
                    area.set_content(body.clone(), cx);
                    area.set_height(height, cx);
                });
                self.recompute_diagnostics(&body, cx);
                self.open = Some(OpenScript {
                    id,
                    original_body: body,
                    record,
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "new script creation failed");
                self.push_console(LogTag::Err, format!("Could not create script: {e}"));
            }
        }
        cx.notify();
    }

    fn request_delete(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.pending_delete = Some(id);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_delete.take() else {
            return;
        };
        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let result = ScriptRepo::delete(&*repo, id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_deleted(id, result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_deleted(&mut self, id: ScriptId, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.scripts.retain(|e| e.id != id);
                if self.selected == Some(id) {
                    self.selected = None;
                    self.open = None;
                    self.code_input.update(cx, |area, cx| {
                        area.set_content("", cx);
                        area.set_height(code_field_height(""), cx);
                    });
                }
                self.load_scripts(cx);
            }
            Err(e) => {
                tracing::warn!(error = %e, "script delete failed");
                self.push_console(LogTag::Err, format!("Could not delete script: {e}"));
                cx.notify();
            }
        }
    }

    fn start_rename(&mut self, id: ScriptId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.find_entry(id).map(|e| e.name.clone()) else {
            return;
        };
        let palette = cx.palette();
        let input = cx.new(|cx| {
            let mut ti =
                TextInput::new(tr!("script_editor_rename_placeholder"), cx).with_palette(palette);
            ti.set_content(current, cx);
            ti
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| match event {
            InputEvent::Submitted(_) => this.commit_rename(cx),
            InputEvent::Cancelled => this.cancel_rename(cx),
            InputEvent::Changed(_) => cx.notify(),
        });
        input.update(cx, |f, cx| f.focus(window, cx));
        self.rename = Some(RenameState {
            target: id,
            input,
            _sub: sub,
        });
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.rename.take() else {
            return;
        };
        let id = state.target;
        let name = state.input.read(cx).content().trim().to_owned();
        if name.is_empty() {
            cx.notify();
            return;
        }
        let taken = self
            .scripts
            .iter()
            .any(|e| e.id != id && e.name.eq_ignore_ascii_case(&name));
        if taken {
            self.push_console(LogTag::Err, format!("Name \"{name}\" is already taken"));
            cx.notify();
            return;
        }

        let repo = Arc::clone(&self.backend) as Arc<dyn ScriptRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        let name_for_task = name.clone();
        self.rt_handle.spawn(async move {
            let result = async {
                let mut record = ScriptRepo::get(&*repo, id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "script not found".to_owned())?;
                record.name = name_for_task.clone();
                ScriptRepo::save(&*repo, record)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok::<(ScriptId, String), String>((id, name_for_task))
            }
            .await;
            let _ = tx.send(result);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(result) = rx.await {
                let _ = this.update(cx, |this, cx| this.apply_renamed(result, cx));
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_renamed(
        &mut self,
        result: Result<(ScriptId, String), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((id, new_name)) => {
                for entry in &mut self.scripts {
                    if entry.id == id {
                        entry.name = new_name.clone();
                    }
                }
                if let Some(open) = self.open.as_mut()
                    && open.id == id
                {
                    open.record.name = new_name;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "script rename failed");
            }
        }
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        cx.notify();
    }

    fn format(&mut self, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        let source = self.code_input.read(cx).content().to_owned();
        let formatted = format_script(&source);
        if formatted != source {
            let height = code_field_height(&formatted);
            self.code_input.update(cx, |area, cx| {
                area.set_content(formatted.clone(), cx);
                area.set_height(height, cx);
            });
            self.recompute_diagnostics(&formatted, cx);
        }
        cx.notify();
    }

    fn toggle_api_docs(&mut self, cx: &mut Context<Self>) {
        self.api_docs_open = !self.api_docs_open;
        cx.notify();
    }

    fn set_console_tab(&mut self, tab: ConsoleTab, cx: &mut Context<Self>) {
        self.console_tab = tab;
        self.console_collapsed = false;
        cx.notify();
    }

    fn clear_console(&mut self, cx: &mut Context<Self>) {
        self.console.clear();
        cx.notify();
    }

    fn toggle_console(&mut self, cx: &mut Context<Self>) {
        self.console_collapsed = !self.console_collapsed;
        cx.notify();
    }

    fn set_list_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let clamped = width.clamp(LIST_MIN_W, LIST_MAX_W);
        if self.list_width != clamped {
            self.list_width = clamped;
            cx.notify();
        }
    }

    fn set_details_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let clamped = width.clamp(DETAILS_MIN_W, DETAILS_MAX_W);
        if self.details_width != clamped {
            self.details_width = clamped;
            cx.notify();
        }
    }

    fn set_console_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        let clamped = height.clamp(CONSOLE_MIN_H, CONSOLE_MAX_H);
        if self.console_height != clamped {
            self.console_height = clamped;
            cx.notify();
        }
    }

    fn page_header(&self, palette: &ForgePalette) -> AnyElement {
        let crumbs = vec![
            BreadcrumbCrumb::leaf(tr!("script_editor_breadcrumb_automation")),
            BreadcrumbCrumb::leaf(tr!("nav_script_editor")),
        ];

        let total = self.scripts.len();
        let ok = self.scripts.iter().filter(|e| e.status_ok).count();

        let mut right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy));

        if total > 0 {
            let health = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(icon(Icon::CircleCheck, GLYPH_STATUS, palette.success))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(tr!(
                            "script_editor_health",
                            ok = ok as i64,
                            total = total as i64
                        )),
                );
            right = right.child(health).child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("·"),
            );
        }

        let right = right.child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child("Rhai 1.25"),
        );

        breadcrumb(crumbs, palette).right(right).into_any_element()
    }

    fn file_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.open.as_ref()?;
        let name = open.record.name.clone();
        let edited = format!(
            "{} {}",
            tr!("script_editor_edited_prefix"),
            fmt_relative_time(Some(open.record.last_modified))
        );

        let (status_color, status_text): (Rgba, String) = match self.type_check {
            None => (palette.success, tr!("script_editor_type_check_passed")),
            Some(n) => (
                palette.warning,
                tr!("script_editor_type_check_errors", count = n as i64),
            ),
        };

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(name),
            );
        if let Some(linked) = self.find_entry(open.id).and_then(|e| e.linked.as_ref()) {
            name_row = name_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .py(px(1.0))
                    .px(px(6.0))
                    .rounded(px(8.0))
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::ExternalLink, px(10.0), palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(px(9.5))
                            .text_color(palette.brand)
                            .child(linked.name.clone()),
                    ),
            );
        }
        if self.current_dirty(cx) {
            name_row = name_row.child(status_dot(palette.warning, FILE_BAR_DIRTY_DOT));
        }

        let status = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(status_dot(status_color, FILE_BAR_STATUS_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_text),
            );

        Some(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Sm, density))
                .py(spacing(Spacing::Xs, density))
                .px(spacing(Spacing::Md, density))
                .bg(palette.base)
                .border_b(BORDER_THIN)
                .border_color(palette.surface_overlay)
                .child(icon(Icon::FileCode, GLYPH_FILE, palette.brand))
                .child(name_row)
                .child(status)
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(edited),
                )
                .into_any_element(),
        )
    }

    fn toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let run = div()
            .id("script-run")
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.success)
            .cursor_pointer()
            .hover(|s| s.bg(with_alpha(palette.success, 0.92)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.run(cx)))
            .child(icon(Icon::PlayerPlay, GLYPH_RUN, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child(tr!("script_editor_run")),
            );

        let save = self.toolbar_button(
            "script-save",
            Icon::Download,
            tr!("script_editor_save"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx)),
        );
        let format = self.toolbar_button(
            "script-format",
            Icon::Refresh,
            tr!("script_editor_format"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.format(cx)),
        );
        let debug =
            self.disabled_toolbar_button(Icon::Bolt, tr!("script_editor_debug"), palette, density);
        let api = self.toolbar_button(
            "script-api-docs",
            Icon::Notebook,
            tr!("script_editor_api_docs"),
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_api_docs(cx)),
        );

        let open_action = self
            .open
            .as_ref()
            .and_then(|o| self.find_entry(o.id))
            .and_then(|e| e.linked.as_ref())
            .map(|linked| {
                let action_id = linked.id;
                let hover = palette.elevated;
                div()
                    .id("script-open-action")
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .py(spacing(Spacing::Xxs, density))
                    .px(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |_this, _: &ClickEvent, _, cx| {
                        cx.emit(NavRequested(Screen::Actions(Some(action_id))));
                    }))
                    .child(icon(Icon::ExternalLink, GLYPH_TOOLBAR, palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.brand)
                            .child(tr!("script_editor_open_action")),
                    )
            });

        let divider = div()
            .w(DIVIDER_W)
            .h(DIVIDER_H)
            .mx(spacing(Spacing::Xs, density))
            .bg(palette.surface_overlay);

        let left = div()
            .flex()
            .items_center()
            .child(run)
            .child(save)
            .child(debug)
            .child(format)
            .child(divider)
            .child(api)
            .children(open_action);

        let sandbox = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_sandbox_label")),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.success)
                    .child(tr!("script_editor_sandbox_enabled")),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(sandbox)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Timeout: 500 ms"),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn toolbar_button(
        &self,
        id: &'static str,
        glyph: Icon,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let hover = palette.elevated;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(handler)
            .child(icon(glyph, GLYPH_TOOLBAR, palette.text_secondary))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn disabled_toolbar_button(
        &self,
        glyph: Icon,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .child(icon(glyph, GLYPH_TOOLBAR, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn left_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = self.search.read(cx).content().trim().to_lowercase();

        let title_row = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(13.0))
                    .text_color(palette.text_primary)
                    .child(tr!("nav_script_editor")),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(self.scripts.len().to_string()),
            );

        let header = div()
            .w_full()
            .flex()
            .flex_col()
            .flex_none()
            .gap(spacing(Spacing::Sm, density))
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(title_row)
            .child(self.search.clone());

        let mut scripts = div().flex().flex_col();
        if self.scripts.is_empty() {
            scripts = scripts.child(
                div()
                    .py(spacing(Spacing::Xxs, Density::Cozy))
                    .px(spacing(Spacing::Xs, Density::Cozy))
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("script_editor_no_scripts")),
            );
        } else {
            let filtered: Vec<&ScriptEntry> = self
                .scripts
                .iter()
                .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
                .collect();
            let action: Vec<&ScriptEntry> = filtered
                .iter()
                .copied()
                .filter(|e| e.linked.is_some())
                .collect();
            let standalone: Vec<&ScriptEntry> = filtered
                .iter()
                .copied()
                .filter(|e| e.linked.is_none())
                .collect();

            if !action.is_empty() {
                let mut group = div()
                    .flex()
                    .flex_col()
                    .mb(spacing(Spacing::Sm, density))
                    .child(self.group_header(
                        Icon::Bolt,
                        palette.brand,
                        tr!("script_editor_group_action"),
                        action.len(),
                        palette,
                    ));
                for entry in &action {
                    group = group.child(self.file_row(entry, palette.brand, palette, cx));
                }
                scripts = scripts.child(group);
            }

            if !standalone.is_empty() {
                let mut group = div()
                    .flex()
                    .flex_col()
                    .mb(spacing(Spacing::Sm, density))
                    .child(self.group_header(
                        Icon::FileCode,
                        palette.bits,
                        tr!("script_editor_group_standalone"),
                        standalone.len(),
                        palette,
                    ));
                for entry in &standalone {
                    group = group.child(self.file_row(entry, palette.bits, palette, cx));
                }
                scripts = scripts.child(group);
            }
        }

        let scroll = div()
            .id("script-left-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .child(scripts);

        let pane = div()
            .flex_none()
            .w(self.list_width)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(header)
            .child(scroll)
            .child(self.new_script_button(palette, density, cx));

        install_resize(
            pane,
            ScriptsListResizeDrag,
            "scripts-list-resize",
            ResizeEdge::Right,
            ResizeRange {
                min: LIST_MIN_W,
                max: LIST_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_list_width(*width, cx)),
        )
        .into_any_element()
    }

    fn new_script_button(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let brand = palette.brand;
        let hover_bg = palette.surface_overlay;
        div()
            .flex_none()
            .p(spacing(Spacing::Sm, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(
                div()
                    .id("script-new")
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(spacing(Spacing::Xs, density))
                    .py(spacing(Spacing::Xs, density))
                    .px(spacing(Spacing::Sm, density))
                    .rounded(px(8.0))
                    .border(BORDER_THIN)
                    .border_color(palette.border_input)
                    .bg(palette.base)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(brand).bg(hover_bg))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_script(cx)))
                    .child(icon(Icon::Plus, GLYPH_ACTION, palette.brand))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(FONT_XS)
                            .text_color(palette.brand)
                            .child(tr!("script_editor_new_script")),
                    ),
            )
            .into_any_element()
    }

    fn group_header(
        &self,
        group_icon: Icon,
        color: Rgba,
        label: impl Into<SharedString>,
        count: usize,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pt(px(2.0))
            .px(px(6.0))
            .pb(px(6.0))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(icon(group_icon, px(11.0), color))
            .child(div().child(label.into()))
            .child(div().flex_1())
            .child(
                div()
                    .text_color(palette.text_faint)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn file_row(
        &self,
        entry: &ScriptEntry,
        group_color: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = entry.id;
        let selected = self.selected == Some(id);
        let renaming = self.rename.as_ref().is_some_and(|r| r.target == id);
        let group: SharedString = format!("script-row-{id}").into();

        let icon_color = if selected {
            group_color
        } else {
            palette.text_faint
        };
        let text_color = if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let stripe = if selected {
            group_color
        } else {
            with_alpha(group_color, 0.0)
        };
        let status_color = if entry.status_ok {
            palette.success
        } else {
            palette.warning
        };
        let (subtitle, subtitle_color): (SharedString, Rgba) = match &entry.linked {
            Some(linked) => (linked.name.clone().into(), group_color),
            None => (tr!("script_editor_manual_run").into(), palette.text_faint),
        };

        let middle: AnyElement = if renaming {
            div()
                .flex_1()
                .min_w_0()
                .child(
                    self.rename
                        .as_ref()
                        .map(|r| r.input.clone().into_any_element())
                        .unwrap_or_else(|| div().into_any_element()),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(text_color)
                        .truncate()
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                if event.click_count >= 2 {
                                    this.start_rename(id, window, cx);
                                }
                            }),
                        )
                        .child(entry.name.clone()),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(subtitle_color)
                        .truncate()
                        .child(subtitle),
                )
                .into_any_element()
        };

        let mut trailing = div().flex_none().flex().items_center().gap(px(4.0));
        if !renaming {
            let delete_bg = palette.random;
            trailing = trailing.child(hover_reveal(
                div()
                    .id(SharedString::from(format!("script-delete-{id}")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(2.0))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(delete_bg))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.request_delete(id, cx);
                    }))
                    .child(icon(Icon::X, GLYPH_ACTION, palette.text_faint)),
                group.clone(),
            ));
        }
        trailing = trailing.child(status_dot(status_color, px(6.0)));

        let mut row = div()
            .id(SharedString::from(format!("script-file-{id}")))
            .group(group.clone())
            .flex()
            .items_center()
            .gap(px(9.0))
            .py(px(7.0))
            .px(px(8.0))
            .rounded(radius(Radius::Sm))
            .border_l(STRIPE_W)
            .border_color(stripe)
            .when(selected, |d| d.bg(palette.surface_overlay))
            .child(icon(Icon::FileCode, px(13.0), icon_color))
            .child(middle)
            .child(trailing);

        if !renaming {
            let hover_bg = palette.base;
            row = row
                .cursor_pointer()
                .when(!selected, |d| d.hover(move |s| s.bg(hover_bg)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)));
        }

        row.into_any_element()
    }

    fn code_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        _cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .id("script-code-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .py(spacing(Spacing::Xs, density))
                    .child(self.code_input.clone()),
            )
            .into_any_element()
    }

    fn console(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self.console_header(palette, density, cx);

        let console = div()
            .w_full()
            .flex()
            .flex_col()
            .flex_none()
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(header);

        if self.console_collapsed {
            return console.into_any_element();
        }

        let console = console.child(self.console_body(palette, density));

        install_resize(
            console,
            ConsoleResizeDrag,
            "scripts-console-resize",
            ResizeEdge::Top,
            ResizeRange {
                min: CONSOLE_MIN_H,
                max: CONSOLE_MAX_H,
            },
            palette,
            cx.listener(|this, height: &Pixels, _, cx| this.set_console_height(*height, cx)),
        )
        .into_any_element()
    }

    fn console_header(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let output_active = self.console_tab == ConsoleTab::Output;
        let output = div()
            .id("console-tab-output")
            .flex()
            .items_center()
            .gap(px(5.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_console_tab(ConsoleTab::Output, cx)
            }))
            .child(icon(
                Icon::Terminal,
                GLYPH_TAB,
                if output_active {
                    palette.text_primary
                } else {
                    palette.text_muted
                },
            ))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(if output_active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_size(FONT_XS)
                    .text_color(if output_active {
                        palette.text_primary
                    } else {
                        palette.text_muted
                    })
                    .child(tr!("script_editor_output_header")),
            );

        let problems_active = self.console_tab == ConsoleTab::Problems;
        let problems = (!self.problems.is_empty()).then(|| {
            div()
                .id("console-tab-problems")
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .cursor_pointer()
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_console_tab(ConsoleTab::Problems, cx)
                }))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(if problems_active {
                            palette.text_primary
                        } else {
                            palette.text_muted
                        })
                        .child(tr!("script_editor_problems_tab")),
                )
                .child(badge(
                    palette.surface_overlay,
                    palette.warning,
                    self.problems.len().to_string(),
                    true,
                    FONT_XXS,
                ))
        });

        let tabs = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(output)
            .children(problems);

        let collapse_glyph = if self.console_collapsed {
            Icon::ChevronUp
        } else {
            Icon::X
        };
        let actions = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .id("console-clear")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_console(cx)))
                    .child(icon(Icon::Eraser, GLYPH_ACTION, palette.text_faint)),
            )
            .child(
                div()
                    .id("console-collapse")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_console(cx)))
                    .child(icon(collapse_glyph, GLYPH_ACTION, palette.text_faint)),
            );

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(tabs)
            .child(actions)
            .into_any_element()
    }

    fn console_body(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let body = div()
            .id("script-console-scroll")
            .w_full()
            .flex()
            .flex_col()
            .h(self.console_height)
            .overflow_y_scroll()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS);

        match self.console_tab {
            ConsoleTab::Output => {
                if self.console.is_empty() {
                    body.child(muted_line(tr!("script_editor_console_cleared"), palette))
                } else {
                    let mut b = body;
                    for line in &self.console {
                        b = b.child(console_row(line, palette));
                    }
                    b
                }
            }
            ConsoleTab::Problems => {
                if self.problems.is_empty() {
                    body.child(muted_line(tr!("script_editor_no_problems"), palette))
                } else {
                    let mut b = body;
                    for problem in &self.problems {
                        b = b.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(spacing(Spacing::Xs, Density::Cozy))
                                .child(icon(Icon::AlertTriangle, GLYPH_ACTION, palette.warning))
                                .child(div().text_color(palette.text_muted).child(problem.clone())),
                        );
                    }
                    b
                }
            }
        }
        .into_any_element()
    }

    fn delete_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.pending_delete?;
        let name = self
            .find_entry(id)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        let kind = tr!("widget_confirm_delete_kind_script");
        let card = confirm_modal(
            tr!("widget_confirm_delete_title", kind = kind.as_str()),
            tr!("widget_confirm_delete_hint"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "script-delete-cancel",
            tr!("widget_confirm_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "script-delete-confirm",
            tr!("script_editor_delete_action"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-delete-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_delete(cx));
                })
                .into_any_element(),
        )
    }

    fn right_pane(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let query = self.api_search.read(cx).content().trim().to_lowercase();

        let header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pb(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("script_editor_api_reference")),
            )
            .child(icon(Icon::Pin, GLYPH_PIN, palette.text_faint));

        let search = div()
            .pb(spacing(Spacing::Sm, Density::Cozy))
            .child(self.api_search.clone());

        let mut pane = div()
            .id("script-api-pane")
            .flex_none()
            .w(RIGHT_PANE_W)
            .h_full()
            .overflow_y_scroll()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .p(spacing(Spacing::Sm, Density::Cozy))
            .child(header)
            .child(search);

        let matches: Vec<&'static MethodDescriptor> = forge_script::catalog()
            .iter()
            .filter(|entry| {
                if query.is_empty() {
                    return true;
                }
                entry.name.to_lowercase().contains(&query)
                    || entry
                        .namespace
                        .is_some_and(|ns| ns.to_lowercase().contains(&query))
            })
            .collect();

        let mut current_ns: Option<Option<&'static str>> = None;
        for entry in &matches {
            if current_ns != Some(entry.namespace) {
                current_ns = Some(entry.namespace);
                pane = pane.child(
                    div()
                        .pt(spacing(Spacing::Sm, Density::Cozy))
                        .pb(spacing(Spacing::Xxs, Density::Cozy))
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_muted)
                        .child(entry.namespace.unwrap_or("core").to_uppercase()),
                );
            }
            pane = pane.child(api_fn_row(entry, palette));
        }

        if matches.is_empty() {
            pane = pane.child(
                div()
                    .pt(spacing(Spacing::Sm, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(tr!("script_editor_api_no_matches")),
            );
        }

        pane.into_any_element()
    }

    fn details_pane(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let Some(open) = self.open.as_ref() else {
            return div().into_any_element();
        };
        let record = &open.record;
        let line_count = record.body.lines().count();
        let edited = fmt_relative_time(Some(record.last_modified));

        let linked = self.find_entry(open.id).and_then(|e| e.linked.as_ref());
        let (type_label, type_color): (SharedString, Rgba) = match linked {
            Some(_) => (tr!("script_editor_type_action").into(), palette.brand),
            None => (tr!("script_editor_type_standalone").into(), palette.bits),
        };

        let mut inner = div()
            .id("script-details-pane")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(DETAILS_PANE_PAD)
            .child(details_heading(
                tr!("script_editor_details_heading"),
                false,
                palette,
            ))
            .child(detail_row(
                tr!("script_editor_details_type"),
                div().text_color(type_color).child(type_label),
                palette,
            ));

        if let Some(linked) = linked {
            inner = inner.child(detail_row(
                tr!("script_editor_details_linked"),
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_color(palette.text_primary)
                    .child(SharedString::from(linked.name.clone())),
                palette,
            ));
        }

        inner = inner
            .child(detail_row(
                tr!("script_editor_details_lines"),
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .child(line_count.to_string()),
                palette,
            ))
            .child(detail_row(
                tr!("script_editor_details_edited"),
                SharedString::from(edited),
                palette,
            ));

        let contract = &record.contract;
        if !contract.inputs.is_empty() || contract.returns.is_some() {
            inner = inner.child(details_heading(
                tr!("script_editor_signature_heading"),
                true,
                palette,
            ));
            for input in &contract.inputs {
                inner = inner.child(signature_input_row(input, palette));
            }
            if let Some(kind) = contract.returns {
                inner = inner.child(signature_returns_row(kind, palette));
            }
        }

        let pane = div()
            .flex_none()
            .w(self.details_width)
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(inner);

        install_resize(
            pane,
            ScriptDetailsResizeDrag,
            "scripts-details-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: DETAILS_MIN_W,
                max: DETAILS_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_details_width(*width, cx)),
        )
        .into_any_element()
    }

    fn run_modal_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let modal_state = self.run_modal.as_ref()?;

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy));
        for field in &modal_state.inputs {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, Density::Cozy))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(format!("{}: {}", field.name, field.label)),
                    )
                    .child(field.input.clone()),
            );
        }
        if let Some(err) = &modal_state.error {
            body = body.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.random)
                    .child(err.clone()),
            );
        }

        let submit: AnyElement = if modal_state.running {
            div()
                .flex()
                .items_center()
                .py(spacing(Spacing::Xs, Density::Cozy))
                .px(spacing(Spacing::Md, Density::Cozy))
                .rounded(radius(Radius::Sm))
                .bg(with_alpha(palette.success, 0.5))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(FONT_XS)
                        .text_color(palette.shell)
                        .child(tr!("script_editor_running")),
                )
                .into_any_element()
        } else {
            primary_button(tr!("script_editor_run"), palette)
                .on_click(
                    "script-run-submit",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.submit_run(cx)),
                )
                .into_any_element()
        };

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(
                ghost_button(tr!("script_editor_run_modal_cancel"), palette).on_click(
                    "script-run-cancel",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_run(cx)),
                ),
            )
            .child(submit);

        let card = modal(modal_state.title.clone(), body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "script-run-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_run(cx)),
            );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-run-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_run(cx));
                })
                .into_any_element(),
        )
    }

    fn discard_overlay(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.pending_nav.as_ref()?;

        let card = confirm_modal(
            tr!("script_editor_discard_title"),
            tr!("script_editor_discard_body"),
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint(tr!("script_editor_discard_esc_hint"))
        .on_cancel(
            "script-discard-cancel",
            tr!("script_editor_discard_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_discard(cx)),
        )
        .on_confirm(
            "script-discard-confirm",
            tr!("script_editor_discard_confirm"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_discard(cx)),
        );

        let view = cx.entity();
        Some(
            overlay(card, palette)
                .position(OverlayPosition::Center)
                .on_dismiss("script-discard-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_discard(cx));
                })
                .into_any_element(),
        )
    }
}

impl Render for ScriptEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.page_header(&palette);
        let file_bar = self.file_bar(&palette, density, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let left = self.left_pane(&palette, density, cx);
        let code = self.code_area(&palette, density, cx);
        let console = self.console(&palette, density, cx);

        let centre = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .children(file_bar)
            .child(toolbar)
            .child(code)
            .child(console);

        let right = if self.api_docs_open {
            Some(self.right_pane(&palette, cx))
        } else if self.open.is_some() {
            Some(self.details_pane(&palette, cx))
        } else {
            None
        };

        let body = div()
            .w_full()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(left)
            .child(centre)
            .children(right);

        let overlay = if self.run_modal.is_some() {
            self.run_modal_overlay(&palette, cx)
        } else if self.pending_delete.is_some() {
            self.delete_overlay(&palette, cx)
        } else if self.pending_nav.is_some() {
            self.discard_overlay(&palette, cx)
        } else {
            None
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(body)
            .children(overlay)
    }
}

fn variant_kind_display(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Int => "Int",
        VariantKind::Float => "Float",
        VariantKind::Bool => "Bool",
        VariantKind::String => "String",
        VariantKind::Datetime => "Datetime",
        VariantKind::Array => "Array",
        VariantKind::Object => "Object",
    }
}

fn details_heading(
    label: impl Into<SharedString>,
    signature: bool,
    palette: &ForgePalette,
) -> impl IntoElement {
    let heading = div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label.into());
    if signature {
        heading
            .pt(spacing(Spacing::Md, Density::Cozy))
            .pb(spacing(Spacing::Xs, Density::Cozy))
    } else {
        heading.pb(spacing(Spacing::Sm, Density::Cozy))
    }
}

fn detail_row(
    label: impl Into<SharedString>,
    value: impl IntoElement,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_secondary)
                .child(value),
        )
}

fn signature_input_row(input: &ScriptInput, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .child(
            div()
                .text_color(palette.code_var)
                .child(SharedString::from(input.name.clone())),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(variant_kind_display(input.kind)),
        )
}

fn signature_returns_row(kind: VariantKind, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .pt(spacing(Spacing::Xs, Density::Cozy))
        .border_t(BORDER_THIN)
        .border_color(palette.surface_overlay)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .child(icon(Icon::ArrowBackUp, GLYPH_RETURNS, palette.text_faint))
        .child(
            div()
                .text_color(palette.text_faint)
                .child(tr!("script_editor_details_returns")),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .text_color(palette.code_str)
                .child(variant_kind_display(kind)),
        )
}

fn console_row(line: &ConsoleLine, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .text_color(palette.text_muted)
        .child(
            div()
                .text_color(palette.text_faint)
                .child(line.time.clone()),
        )
        .child(
            div()
                .text_color(line.tag.color(palette))
                .child(line.tag.label()),
        )
        .child(div().child(line.text.clone()))
}

fn muted_line(text: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    div().text_color(palette.text_faint).child(text.into())
}

fn api_fn_row(entry: &MethodDescriptor, palette: &ForgePalette) -> impl IntoElement {
    let params = entry
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let sig = format!("{}({}) -> {}", entry.name, params, entry.return_type);

    let mut row = div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(badge(palette.brand, palette.shell, "fn", true, FONT_XXS))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_primary)
                        .child(sig),
                ),
        );

    if let Some(doc) = entry.doc {
        row = row.child(
            div()
                .pl(spacing(Spacing::Lg, Density::Cozy))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(doc),
        );
    }
    row
}
