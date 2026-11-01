mod api_pane;
mod contract_editing;
mod details_pane;
pub(super) mod diagnostics;
pub(super) mod input_parsing;
mod loading;
mod run_modal;
mod running;
mod saving;
mod signature;
mod toolbar;

use self::contract_editing::{InputRename, KindMenu};
use self::run_modal::RunModal;
use super::ScriptDeps;
use super::console::ScriptConsole;
use super::list::LinkedAction;
use forge_components::{
    ActivePresentation, CodeEditor, FONT_XS, ForgePalette, Icon, InputEvent, Language, Radius,
    SaveCode, SearchState, TextInput, tr,
};
use forge_storage::{ScriptRecord, ScriptTelemetry};
use forge_types::{ActionId, ScriptContract, ScriptId};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, EventEmitter, Pixels, SharedString, Subscription, Task, Window,
    div, px,
};

const CODE_LINE_HEIGHT: Pixels = px(19.8);
const DETAILS_PANE_W: Pixels = px(232.0);

struct OpenScript {
    id: ScriptId,
    record: ScriptRecord,
    original_body: String,
    original_contract: ScriptContract,
}

pub(super) enum ScriptDocumentEvent {
    Opened,
    Saved(ScriptId),
    KindMenuOpened,
    OpenAction(ActionId),
}

pub(super) struct ScriptDocument {
    deps: ScriptDeps,
    console: Entity<ScriptConsole>,

    open: Option<OpenScript>,
    linked: Option<LinkedAction>,
    telemetry: Option<ScriptTelemetry>,
    saving: bool,
    run_in_flight: bool,
    engine_timeout_ms: Option<u64>,
    details_width: Pixels,

    input_rename: Option<InputRename>,
    kind_menu: Option<KindMenu>,
    contract_error: Option<SharedString>,

    pub(super) code_input: Entity<CodeEditor>,
    _code_sub: Subscription,

    problem_lines: Vec<usize>,
    compile_error: Option<SharedString>,
    _compile_check: Option<Task<()>>,

    api_docs_open: bool,
    api_search: SearchState,
    _api_search_sub: Subscription,

    run_modal: Option<Entity<RunModal>>,
    _run_sub: Option<Subscription>,
}

impl EventEmitter<ScriptDocumentEvent> for ScriptDocument {}

impl ScriptDocument {
    pub(super) fn new(
        deps: ScriptDeps,
        console: Entity<ScriptConsole>,
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

        let mut document = Self {
            deps,
            console,
            open: None,
            linked: None,
            telemetry: None,
            saving: false,
            run_in_flight: false,
            engine_timeout_ms: None,
            details_width: DETAILS_PANE_W,
            input_rename: None,
            kind_menu: None,
            contract_error: None,
            code_input,
            _code_sub: code_sub,
            problem_lines: Vec::new(),
            compile_error: None,
            _compile_check: None,
            api_docs_open: false,
            api_search,
            _api_search_sub: api_search_sub,
            run_modal: None,
            _run_sub: None,
        };
        document.load_engine_timeout(cx);
        document
    }

    #[cfg(test)]
    pub(super) fn console(&self) -> Entity<ScriptConsole> {
        self.console.clone()
    }

    pub(super) fn open_id(&self) -> Option<ScriptId> {
        self.open.as_ref().map(|open| open.id)
    }

    pub(super) fn has_open(&self) -> bool {
        self.open.is_some()
    }

    pub(super) fn open_name(&self) -> Option<SharedString> {
        self.open
            .as_ref()
            .map(|open| SharedString::from(open.record.name.clone()))
    }

    pub(super) fn is_saving(&self) -> bool {
        self.saving
    }

    pub(super) fn kind_menu_open(&self) -> bool {
        self.kind_menu.is_some()
    }

    pub(super) fn run_modal_open(&self) -> bool {
        self.run_modal.is_some()
    }

    pub(super) fn run_modal_layer(&self) -> Option<AnyElement> {
        self.run_modal.clone().map(IntoElement::into_any_element)
    }

    pub(super) fn set_linked(&mut self, linked: Option<LinkedAction>, cx: &mut Context<Self>) {
        self.linked = linked;
        cx.notify();
    }

    fn code_area(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .on_action(cx.listener(|this, _: &SaveCode, _, cx| this.save(cx)))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.code_input.clone())
            .into_any_element()
    }
}

impl Render for ScriptDocument {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let centre = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .children(self.file_bar(&palette, density, cx))
            .child(self.toolbar(&palette, density, cx))
            .child(self.code_area(&palette, cx))
            .child(self.console.clone());

        let right = if self.api_docs_open {
            Some(self.right_pane(&palette, density, cx))
        } else if self.open.is_some() {
            Some(self.details_pane(&palette, cx))
        } else {
            None
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_row()
            .child(centre)
            .children(right)
    }
}
