mod api_pane;
mod console;
mod contract_editing;
mod details_pane;
mod diagnostics;
mod file_list;
mod input_parsing;
mod overlays;
mod run_modal;
mod script_commands;
mod script_loading;
mod script_rename;
mod script_running;
mod signature;
#[cfg(test)]
mod tests;
mod toolbar;

use self::console::{ConsoleLine, ConsoleTab};
use self::contract_editing::{InputRename, KindMenu};
use self::diagnostics::EditorProblem;
use self::file_list::RowMenu;
use self::run_modal::RunModal;
use self::script_rename::{PendingRename, RenameState};
use crate::navigation::NavRequested;
use crate::screen_shortcuts::ScreenShortcuts;
use crate::unsaved_work::UnsavedWork;
use forge_components::highlight::Language;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, CodeEditor, Confirm, FONT_XS, ForgePalette, Icon,
    InputEvent, Radius, SaveCode, SearchState, TextInput, page_frame, tr,
};
use forge_runtime::{EventBus, ScriptRegistry};
use forge_storage::{
    DataProvider, SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND, ScriptRecord, ScriptReferences,
    ScriptTelemetry,
};
use forge_types::{Action, ActionId, IntegrationAvailability, ScriptContract, ScriptId};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, Pixels, SharedString, Subscription, Task,
    UniformListScrollHandle, Window, div, px,
};
use std::collections::VecDeque;
use std::sync::Arc;

const LEFT_PANE_W: Pixels = px(200.0);
const CODE_LINE_HEIGHT: Pixels = px(19.8);
const DETAILS_PANE_W: Pixels = px(232.0);
const CONSOLE_INIT_H: Pixels = px(160.0);
const GLYPH_ACTION: Pixels = px(12.0);

struct LinkedAction {
    id: ActionId,
    name: String,
}

struct ScriptEntry {
    id: ScriptId,
    name: String,
    status_ok: bool,
    enabled: bool,
    linked: Option<LinkedAction>,
}

fn find_linked_action(actions: &[Action], script_name: &str) -> Option<LinkedAction> {
    actions.iter().find_map(|action| {
        let links = action.sub_actions.iter().any(|step| {
            step.kind_id == SCRIPT_RUN_NAMED_KIND
                && step.config.get(SCRIPT_NAME_KEY).and_then(|v| v.as_str()) == Some(script_name)
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
    original_contract: ScriptContract,
}

enum PendingNav {
    SelectScript(ScriptId),
    NewScript,
}

pub struct ScriptEditorView {
    backend: Arc<dyn DataProvider>,
    script_registry: Arc<ScriptRegistry>,
    bus: Arc<EventBus>,
    integrations: Arc<dyn IntegrationAvailability>,
    rt_handle: tokio::runtime::Handle,

    scripts: Vec<ScriptEntry>,
    selected: Option<ScriptId>,
    open: Option<OpenScript>,
    telemetry: Option<ScriptTelemetry>,
    loading: bool,
    saving: bool,
    run_in_flight: bool,
    engine_timeout_ms: Option<u64>,

    list_width: Pixels,
    details_width: Pixels,
    console_height: Pixels,

    search: SearchState,
    _search_sub: Subscription,

    rename: Option<RenameState>,
    row_menu: Option<RowMenu>,
    pending_delete: Confirm<ScriptId>,
    delete_referrers: Option<ScriptReferences>,
    pending_rename: Option<PendingRename>,
    pending_nav: Option<PendingNav>,

    input_rename: Option<InputRename>,
    kind_menu: Option<KindMenu>,
    contract_error: Option<SharedString>,

    code_input: Entity<CodeEditor>,
    _code_sub: Subscription,

    console: VecDeque<ConsoleLine>,
    console_scroll: UniformListScrollHandle,
    console_tab: ConsoleTab,
    console_collapsed: bool,
    problems: Vec<EditorProblem>,
    problem_lines: Vec<usize>,
    compile_error: Option<SharedString>,
    _compile_check: Option<Task<()>>,

    api_docs_open: bool,
    api_search: SearchState,
    _api_search_sub: Subscription,

    run_modal: Option<Entity<RunModal>>,
    _run_sub: Option<Subscription>,
}

impl EventEmitter<NavRequested> for ScriptEditorView {}

impl UnsavedWork for ScriptEditorView {
    fn has_unsaved_work(&self, cx: &App) -> bool {
        self.current_dirty(cx)
    }

    fn unsaved_work_name(&self) -> Option<SharedString> {
        self.open
            .as_ref()
            .map(|open| SharedString::from(open.record.name.clone()))
    }

    fn start_saving_unsaved_work(&mut self, cx: &mut Context<Self>) -> bool {
        self.save(cx);
        self.saving
    }

    fn is_saving_unsaved_work(&self) -> bool {
        self.saving
    }

    fn discard_unsaved_work(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        self.revert_current(cx);
        cx.notify();
    }
}

impl ScreenShortcuts for ScriptEditorView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if !self.prompt_open() {
            self.start_saving_unsaved_work(cx);
        }
    }

    fn run_shortcut(&mut self, cx: &mut Context<Self>) {
        if !self.prompt_open() {
            self.run(cx);
        }
    }
}

impl ScriptEditorView {
    fn prompt_open(&self) -> bool {
        self.run_modal.is_some()
            || self.pending_delete.get().is_some()
            || self.pending_rename.is_some()
            || self.pending_nav.is_some()
    }

    pub fn new(
        backend: Arc<dyn DataProvider>,
        script_registry: Arc<ScriptRegistry>,
        bus: Arc<EventBus>,
        integrations: Arc<dyn IntegrationAvailability>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();

        let code_input = cx.new(|cx| {
            CodeEditor::new(Language::Rhai, "// write your rhai script", cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .with_line_height(CODE_LINE_HEIGHT)
        });
        let code_sub = cx.subscribe(&code_input, |this, _area, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.on_code_changed(cx);
            }
        });

        let search = SearchState::from_field(cx.new(|cx| {
            TextInput::new(tr!("script_editor_search_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .leading_icon(Icon::Search, palette.text_faint)
                .on_surface()
                .static_chrome(palette.surface_overlay, Radius::Sm)
        }));
        let search_sub = cx.subscribe(search.field(), |this: &mut Self, _f, event, cx| {
            if this.search.on_changed(event) {
                cx.notify();
            }
        });

        let api_search = SearchState::from_field(cx.new(|cx| {
            TextInput::new(tr!("script_editor_api_search_placeholder"), cx)
                .with_palette(palette)
                .with_font_size(FONT_XS)
                .leading_icon(Icon::Search, palette.text_faint)
                .on_surface()
                .static_chrome(palette.surface_overlay, Radius::Sm)
        }));
        let api_search_sub = cx.subscribe(api_search.field(), |this: &mut Self, _f, event, cx| {
            if this.api_search.on_changed(event) {
                cx.notify();
            }
        });

        let mut view = Self {
            backend,
            script_registry,
            bus,
            integrations,
            rt_handle,
            scripts: Vec::new(),
            selected: None,
            open: None,
            telemetry: None,
            loading: false,
            saving: false,
            run_in_flight: false,
            engine_timeout_ms: None,
            list_width: LEFT_PANE_W,
            details_width: DETAILS_PANE_W,
            console_height: CONSOLE_INIT_H,
            search,
            _search_sub: search_sub,
            rename: None,
            row_menu: None,
            pending_delete: Confirm::default(),
            delete_referrers: None,
            pending_rename: None,
            pending_nav: None,
            input_rename: None,
            kind_menu: None,
            contract_error: None,
            code_input,
            _code_sub: code_sub,
            console: VecDeque::new(),
            console_scroll: UniformListScrollHandle::new(),
            console_tab: ConsoleTab::Output,
            console_collapsed: false,
            problems: Vec::new(),
            problem_lines: Vec::new(),
            compile_error: None,
            _compile_check: None,
            api_docs_open: false,
            api_search,
            _api_search_sub: api_search_sub,
            run_modal: None,
            _run_sub: None,
        };
        view.start_log_bridge(cx);
        view.load_scripts(cx);
        view.load_engine_timeout(cx);
        view
    }

    fn code_area(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .on_action(cx.listener(|this, _: &SaveCode, _, cx| {
                this.start_saving_unsaved_work(cx);
            }))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.code_input.clone())
            .into_any_element()
    }
}

impl Render for ScriptEditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header_right = self.render_header_right(&palette, density);
        let file_bar = self.file_bar(&palette, density, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let left = self.left_pane(&palette, density, cx);
        let code = self.code_area(&palette, cx);
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
            Some(self.right_pane(&palette, density))
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

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("script_editor_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("nav_script_editor")),
            ],
            &palette,
        )
        .header_right(header_right)
        .body(body);

        let overlay = if let Some(modal) = self.run_modal.clone() {
            Some(modal.into_any_element())
        } else if self.pending_delete.is_pending() {
            self.delete_overlay(&palette, cx)
        } else if self.pending_rename.is_some() {
            self.rename_overlay(&palette, cx)
        } else if self.pending_nav.is_some() {
            self.discard_overlay(&palette, cx)
        } else {
            None
        };

        let row_menu = self.render_row_context_menu(&palette, cx);
        let kind_menu = self.render_kind_menu(&palette, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(overlay)
            .children(row_menu)
            .children(kind_menu)
    }
}
