use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density,
    FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, ModalSize, OverlayPosition, Radius,
    Spacing, TextArea, TextInput, badge, breadcrumb, confirm_modal, ghost_button, icon, modal,
    overlay, primary_button, radius, spacing, with_alpha,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const LEFT_PANE_W: Pixels = px(200.0);
const STRIPE_W: Pixels = px(2.0);
const FILE_INDENT: Pixels = px(14.0);
const RIGHT_PANE_W: Pixels = px(220.0);
const GLYPH_PIN: Pixels = px(12.0);

const CODE_LINE_H_PX: f32 = 18.0;
const CODE_LINE_H: Pixels = px(CODE_LINE_H_PX);
const CODE_PAD_V_PX: f32 = 6.0;
const GUTTER_W: Pixels = px(38.0);
const GUTTER_PAD_R: Pixels = px(14.0);

const DIVIDER_W: Pixels = px(0.5);
const DIVIDER_H: Pixels = px(16.0);

const GLYPH_RUN: Pixels = px(11.0);
const GLYPH_TOOLBAR: Pixels = px(13.0);
const GLYPH_STATUS: Pixels = px(12.0);
const GLYPH_FOLDER: Pixels = px(13.0);
const GLYPH_FILE: Pixels = px(12.0);
const GLYPH_TAB: Pixels = px(12.0);
const GLYPH_ACTION: Pixels = px(12.0);

fn code_field_height(content: &str) -> Pixels {
    let lines = content.lines().count().max(1) as f32;
    px(lines * CODE_LINE_H_PX + CODE_PAD_V_PX * 2.0)
}

struct ScriptFile {
    id: u64,
    name: String,
    content: String,
    dirty: bool,
}

struct ScriptFolder {
    name: String,
    expanded: bool,
    files: Vec<ScriptFile>,
}

struct RenameState {
    target: u64,
    input: Entity<TextInput>,
    _sub: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConsoleTab {
    Output,
    Problems,
    TestRun,
}

#[derive(Clone, Copy)]
enum LogTag {
    Run,
    Info,
    Ok,
    Stats,
}

impl LogTag {
    fn label(self) -> &'static str {
        match self {
            LogTag::Run => "[run]",
            LogTag::Info => "[info]",
            LogTag::Ok => "[ok]",
            LogTag::Stats => "[stats]",
        }
    }

    fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            LogTag::Run => palette.info,
            LogTag::Info | LogTag::Ok => palette.success,
            LogTag::Stats => palette.brand,
        }
    }
}

struct ConsoleLine {
    time: SharedString,
    tag: LogTag,
    text: SharedString,
}

enum PendingNav {
    SelectScript(u64),
    GoBack,
}

struct RunInput {
    name: SharedString,
    label: SharedString,
    input: Entity<TextInput>,
    _sub: Subscription,
}

struct RunModalState {
    title: SharedString,
    script_name: String,
    inputs: Vec<RunInput>,
    error: Option<SharedString>,
}

struct ApiGroup {
    label: &'static str,
    fns: &'static [&'static str],
}

/// `None` = type-check passed; `Some(n)` = error count.
type TypeCheck = Option<u32>;

pub struct ScriptEditorView {
    folders: Vec<ScriptFolder>,
    shared: Vec<ScriptFile>,
    variables: Vec<(SharedString, SharedString)>,
    selected: Option<u64>,
    open_original: String,
    rename: Option<RenameState>,
    pending_delete: Option<u64>,
    pending_nav: Option<PendingNav>,
    code_input: Entity<TextArea>,
    _code_sub: Subscription,
    console: Vec<ConsoleLine>,
    console_tab: ConsoleTab,
    console_collapsed: bool,
    problems: Vec<SharedString>,
    type_check: TypeCheck,
    api_docs_open: bool,
    api_search: Entity<TextInput>,
    _api_search_sub: Subscription,
    run_modal: Option<RunModalState>,
    next_id: u64,
}

impl EventEmitter<NavRequested> for ScriptEditorView {}

impl ScriptEditorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let (folders, shared) = seed_scripts();

        let selected = folders.first().and_then(|f| f.files.first()).map(|f| f.id);
        let seed_content = folders
            .first()
            .and_then(|f| f.files.first())
            .map(|f| f.content.clone())
            .unwrap_or_default();

        let code_input = cx.new(|cx| {
            let mut area = TextArea::new("// write your rhai script", cx)
                .with_palette(palette)
                .mono()
                .with_font_size(FONT_XS)
                .with_height(code_field_height(&seed_content));
            area.set_content(seed_content.clone(), cx);
            area
        });
        let code_sub = cx.subscribe(&code_input, |this, _area, event: &InputEvent, cx| {
            if let InputEvent::Changed(_) = event {
                this.on_code_changed(cx);
            }
        });

        let next_id = folders
            .iter()
            .flat_map(|f| f.files.iter())
            .chain(shared.iter())
            .map(|f| f.id)
            .max()
            .map_or(0, |m| m + 1);

        let api_search = cx.new(|cx| {
            TextInput::new("Search modules\u{2026}", cx)
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

        Self {
            folders,
            shared,
            variables: vec![
                ("%lines%".into(), "array".into()),
                ("%idx%".into(), "int".into()),
                ("%user%".into(), "User".into()),
            ],
            selected,
            open_original: seed_content,
            rename: None,
            pending_delete: None,
            pending_nav: None,
            code_input,
            _code_sub: code_sub,
            console: seed_console(),
            console_tab: ConsoleTab::Output,
            console_collapsed: false,
            problems: vec!["Ln 8 · unused binding `parts` before reassignment".into()],
            type_check: None,
            api_docs_open: false,
            api_search,
            _api_search_sub: api_search_sub,
            run_modal: None,
            next_id,
        }
    }

    fn find_file(&self, id: u64) -> Option<&ScriptFile> {
        self.folders
            .iter()
            .flat_map(|f| f.files.iter())
            .chain(self.shared.iter())
            .find(|f| f.id == id)
    }

    fn find_file_mut(&mut self, id: u64) -> Option<&mut ScriptFile> {
        self.folders
            .iter_mut()
            .flat_map(|f| f.files.iter_mut())
            .chain(self.shared.iter_mut())
            .find(|f| f.id == id)
    }

    fn folder_of(&self, id: u64) -> Option<&str> {
        self.folders
            .iter()
            .find(|folder| folder.files.iter().any(|f| f.id == id))
            .map(|folder| folder.name.as_str())
    }

    fn current_dirty(&self) -> bool {
        self.selected
            .and_then(|id| self.find_file(id))
            .is_some_and(|f| f.dirty)
    }

    fn open_file(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(content) = self.find_file(id).map(|f| f.content.clone()) else {
            return;
        };
        self.selected = Some(id);
        self.open_original = content.clone();
        let height = code_field_height(&content);
        self.code_input.update(cx, |area, cx| {
            area.set_content(content, cx);
            area.set_height(height, cx);
        });
    }

    fn revert_current(&mut self, cx: &mut Context<Self>) {
        let original = self.open_original.clone();
        if let Some(id) = self.selected
            && let Some(file) = self.find_file_mut(id)
        {
            file.content = original.clone();
            file.dirty = false;
        }
        let height = code_field_height(&original);
        self.code_input.update(cx, |area, cx| {
            area.set_content(original, cx);
            area.set_height(height, cx);
        });
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.current_dirty() {
            self.pending_nav = Some(PendingNav::GoBack);
            cx.notify();
            return;
        }
        cx.emit(NavRequested(Screen::Actions));
    }

    fn confirm_discard(&mut self, cx: &mut Context<Self>) {
        let Some(nav) = self.pending_nav.take() else {
            return;
        };
        self.revert_current(cx);
        match nav {
            PendingNav::SelectScript(id) => self.open_file(id, cx),
            PendingNav::GoBack => cx.emit(NavRequested(Screen::Actions)),
        }
        cx.notify();
    }

    fn cancel_discard(&mut self, cx: &mut Context<Self>) {
        self.pending_nav = None;
        cx.notify();
    }

    fn toggle_folder(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(folder) = self.folders.get_mut(index) {
            folder.expanded = !folder.expanded;
        }
        cx.notify();
    }

    fn select(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            return;
        }
        if self.current_dirty() {
            self.pending_nav = Some(PendingNav::SelectScript(id));
            cx.notify();
            return;
        }
        self.open_file(id, cx);
        cx.notify();
    }

    fn on_code_changed(&mut self, cx: &mut Context<Self>) {
        let content = self.code_input.read(cx).content().to_owned();
        let height = code_field_height(&content);
        if let Some(id) = self.selected
            && let Some(file) = self.find_file_mut(id)
        {
            file.content = content;
            file.dirty = true;
        }
        self.code_input
            .update(cx, |area, cx| area.set_height(height, cx));
        cx.notify();
    }

    fn new_script(&mut self, cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        let name = format!("script_{id}.rhai");
        let content = format!("// {name}\n\nfn main() {{\n    \n}}\n");

        let target_folder = self
            .selected
            .and_then(|sel| {
                self.folders
                    .iter()
                    .position(|folder| folder.files.iter().any(|f| f.id == sel))
            })
            .or_else(|| (!self.folders.is_empty()).then_some(0));

        let file = ScriptFile {
            id,
            name,
            content: content.clone(),
            dirty: false,
        };
        match target_folder {
            Some(idx) => {
                self.folders[idx].expanded = true;
                self.folders[idx].files.push(file);
            }
            None => self.shared.push(file),
        }

        self.selected = Some(id);
        self.open_original = content.clone();
        let height = code_field_height(&content);
        self.code_input.update(cx, |area, cx| {
            area.set_content(content, cx);
            area.set_height(height, cx);
        });
        cx.notify();
    }

    fn request_delete(&mut self, id: u64, cx: &mut Context<Self>) {
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
        for folder in &mut self.folders {
            folder.files.retain(|f| f.id != id);
        }
        self.shared.retain(|f| f.id != id);

        if self.selected == Some(id) {
            let next = self
                .folders
                .iter()
                .flat_map(|f| f.files.iter())
                .chain(self.shared.iter())
                .map(|f| f.id)
                .next();
            match next {
                Some(next_id) => self.open_file(next_id, cx),
                None => {
                    self.selected = None;
                    self.open_original = String::new();
                    self.code_input.update(cx, |area, cx| {
                        area.set_content("", cx);
                        area.set_height(code_field_height(""), cx);
                    });
                }
            }
        }
        cx.notify();
    }

    fn start_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.find_file(id).map(|f| f.name.clone()) else {
            return;
        };
        let palette = cx.palette();
        let input = cx.new(|cx| {
            let mut ti = TextInput::new("Script name", cx).with_palette(palette);
            ti.set_content(current, cx);
            ti
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| match event {
            InputEvent::Submitted(_) => this.commit_rename(cx),
            InputEvent::Cancelled => this.cancel_rename(cx),
            InputEvent::Changed(_) => cx.notify(),
        });
        input.read(cx).focus(window);
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
        let name = state.input.read(cx).content().trim().to_owned();
        if !name.is_empty()
            && let Some(file) = self.find_file_mut(state.target)
        {
            file.name = name;
        }
        cx.notify();
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.rename = None;
        cx.notify();
    }

    fn open_run_modal(&mut self, cx: &mut Context<Self>) {
        let (script_name, title) = match self.selected.and_then(|id| self.find_file(id)) {
            Some(f) => (f.name.clone(), format!("Run {}", f.name)),
            None => ("script".to_owned(), "Run script".to_owned()),
        };
        let palette = cx.palette();
        let mut inputs = Vec::new();
        for (name, label) in [("lines", "array"), ("idx", "int")] {
            inputs.push(self.build_run_input(name, label, palette, cx));
        }
        self.run_modal = Some(RunModalState {
            title: title.into(),
            script_name,
            inputs,
            error: None,
        });
        cx.notify();
    }

    fn build_run_input(
        &self,
        name: &str,
        label: &str,
        palette: ForgePalette,
        cx: &mut Context<Self>,
    ) -> RunInput {
        let placeholder = format!("Enter {label} value\u{2026}");
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
            label: label.to_owned().into(),
            input,
            _sub: sub,
        }
    }

    fn submit_run(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = self.run_modal.as_ref() else {
            return;
        };
        let missing = modal
            .inputs
            .iter()
            .find(|f| f.input.read(cx).content().trim().is_empty())
            .map(|f| f.name.clone());
        if let Some(name) = missing {
            if let Some(modal) = self.run_modal.as_mut() {
                modal.error = Some(format!("Enter a value for {name}").into());
            }
            cx.notify();
            return;
        }

        let name = modal.script_name.clone();
        self.run_modal = None;
        self.console.extend(seed_run_result(&name));
        self.console_tab = ConsoleTab::Output;
        self.console_collapsed = false;
        cx.notify();
    }

    fn cancel_run(&mut self, cx: &mut Context<Self>) {
        self.run_modal = None;
        cx.notify();
    }

    fn format(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected
            && let Some(file) = self.find_file_mut(id)
        {
            file.dirty = false;
        }
        self.console.push(ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Info,
            text: "formatted (stub)".into(),
        });
        cx.notify();
    }

    fn debug(&mut self, cx: &mut Context<Self>) {
        self.console.push(ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Info,
            text: "debug session unavailable (stub - no runtime wired)".into(),
        });
        self.console_tab = ConsoleTab::Output;
        self.console_collapsed = false;
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

    fn page_header(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut crumbs = vec![BreadcrumbCrumb::link(
            "Actions",
            "script-crumb-actions",
            cx.listener(|this, _: &ClickEvent, _, cx| this.go_back(cx)),
        )];
        match self.selected.and_then(|id| self.find_file(id)) {
            Some(file) => {
                let folder = self.folder_of(file.id).unwrap_or("Shared").to_owned();
                crumbs.push(BreadcrumbCrumb::leaf(folder));
                let label = if file.dirty {
                    format!("{} ●", file.name)
                } else {
                    file.name.clone()
                };
                crumbs.push(BreadcrumbCrumb::leaf(label));
            }
            None => crumbs.push(BreadcrumbCrumb::leaf("-")),
        }

        let (status_icon, status_color, status_text): (Icon, Rgba, String) = match self.type_check {
            None => (
                Icon::CircleCheck,
                palette.success,
                "Type-check passed".to_owned(),
            ),
            Some(n) => (Icon::AlertTriangle, palette.warning, format!("{n} errors")),
        };
        let status = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .text_color(status_color)
            .child(icon(status_icon, GLYPH_STATUS, status_color))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(status_color)
                    .child(status_text),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(status)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("·"),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child("Rhai 1.19"),
            );

        breadcrumb(crumbs, palette).right(right).into_any_element()
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_run_modal(cx)))
            .child(icon(Icon::PlayerPlay, GLYPH_RUN, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child("Run"),
            );

        let debug = self.toolbar_button(
            "script-debug",
            Icon::Bolt,
            "Debug",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.debug(cx)),
        );
        let format = self.toolbar_button(
            "script-format",
            Icon::Refresh,
            "Format",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.format(cx)),
        );
        let api = self.toolbar_button(
            "script-api-docs",
            Icon::Notebook,
            "API docs",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_api_docs(cx)),
        );

        let divider = div()
            .w(DIVIDER_W)
            .h(DIVIDER_H)
            .mx(spacing(Spacing::Xs, density))
            .bg(palette.surface_overlay);

        let left = div()
            .flex()
            .items_center()
            .child(run)
            .child(debug)
            .child(format)
            .child(divider)
            .child(api);

        let sandbox = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Sandbox:"),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.success)
                    .child("enabled"),
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
                    .child("Timeout: 500ms"),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Ln 1, Col 1"),
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
        label: &'static str,
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
                    .child(label),
            )
            .into_any_element()
    }

    fn left_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut scripts = div().flex().flex_col().child(scripts_header(palette, cx));

        for (index, folder) in self.folders.iter().enumerate() {
            scripts = scripts.child(self.folder_header(index, folder, palette, cx));
            if folder.expanded {
                for file in &folder.files {
                    scripts = scripts.child(self.file_row(file, true, palette, cx));
                }
            }
        }

        let mut shared = div()
            .flex()
            .flex_col()
            .child(section_label("SHARED", palette));
        for file in &self.shared {
            shared = shared.child(self.file_row(file, false, palette, cx));
        }

        let mut vars = div()
            .flex()
            .flex_col()
            .child(section_label("VARIABLES IN SCOPE", palette));
        for (name, ty) in &self.variables {
            vars = vars.child(variable_row(name.clone(), ty.clone(), palette));
        }

        div()
            .id("script-left-pane")
            .flex_none()
            .w(LEFT_PANE_W)
            .h_full()
            .overflow_y_scroll()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .child(scripts)
            .child(shared)
            .child(vars)
            .into_any_element()
    }

    fn folder_header(
        &self,
        index: usize,
        folder: &ScriptFolder,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let glyph = if folder.expanded {
            Icon::FolderOpen
        } else {
            Icon::Folder
        };
        div()
            .id(("script-folder", index))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(|s| s.bg(palette.elevated))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_folder(index, cx)))
            .child(icon(glyph, GLYPH_FOLDER, palette.warning))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_secondary)
                    .child(folder.name.clone()),
            )
            .into_any_element()
    }

    fn file_row(
        &self,
        file: &ScriptFile,
        in_folder: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = file.id;
        let selected = self.selected == Some(id);
        let renaming = self.rename.as_ref().is_some_and(|r| r.target == id);

        let base_icon = if in_folder {
            palette.brand
        } else {
            palette.info
        };
        let icon_color = if selected { palette.brand } else { base_icon };
        let text_color = if selected {
            palette.brand
        } else {
            palette.text_secondary
        };

        let label: AnyElement = if renaming {
            self.rename
                .as_ref()
                .map(|r| r.input.clone().into_any_element())
                .unwrap_or_else(|| div().into_any_element())
        } else {
            div()
                .flex_1()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(text_color)
                .child(file.name.clone())
                .into_any_element()
        };

        let mut row = div()
            .id(("script-file", id as usize))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .py(spacing(Spacing::Xxs, Density::Cozy))
            .px(spacing(Spacing::Xs, Density::Cozy))
            .rounded(radius(Radius::Sm))
            .when(in_folder, |d| d.ml(FILE_INDENT))
            .when(selected, |d| {
                d.bg(palette.elevated)
                    .border_l(STRIPE_W)
                    .border_color(palette.brand)
            })
            .child(icon(Icon::FileCode, GLYPH_FILE, icon_color))
            .child(label);

        if !renaming {
            row = row
                .cursor_pointer()
                .hover(|s| s.bg(palette.elevated))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)));
        }

        if selected && !renaming {
            row = row
                .child(
                    div()
                        .id(("script-rename", id as usize))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.start_rename(id, window, cx)
                        }))
                        .child(icon(Icon::Pencil, GLYPH_ACTION, palette.text_faint)),
                )
                .child(
                    div()
                        .id(("script-delete", id as usize))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.request_delete(id, cx)
                        }))
                        .child(icon(Icon::CircleX, GLYPH_ACTION, palette.text_faint)),
                );
        }

        row.into_any_element()
    }

    fn code_area(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let content = self.code_input.read(cx).content();
        let line_count = content.lines().count().max(1);

        let mut gutter = div()
            .flex_none()
            .flex()
            .flex_col()
            .w(GUTTER_W)
            .pt(px(CODE_PAD_V_PX))
            .pr(GUTTER_PAD_R);
        for n in 1..=line_count {
            gutter = gutter.child(
                div()
                    .h(CODE_LINE_H)
                    .flex()
                    .justify_end()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(n.to_string()),
            );
        }

        let editor = div().flex_1().min_w_0().child(self.code_input.clone());

        div()
            .id("script-code-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .py(spacing(Spacing::Xs, density))
                    .child(gutter)
                    .child(editor),
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

        let mut console = div()
            .w_full()
            .flex()
            .flex_col()
            .flex_none()
            .bg(palette.shell)
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .child(header);

        if !self.console_collapsed {
            console = console.child(self.console_body(palette, density));
        }
        console.into_any_element()
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
                    .child("Output"),
            );

        let problems_active = self.console_tab == ConsoleTab::Problems;
        let problems = div()
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
                    .child("Problems"),
            )
            .child(badge(
                palette.surface_overlay,
                palette.warning,
                self.problems.len().to_string(),
                true,
                FONT_XXS,
            ));

        let test_active = self.console_tab == ConsoleTab::TestRun;
        let test = div()
            .id("console-tab-test")
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_console_tab(ConsoleTab::TestRun, cx)
            }))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(if test_active {
                        palette.text_primary
                    } else {
                        palette.text_muted
                    })
                    .child("Test run"),
            );

        let tabs = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(output)
            .child(problems)
            .child(test);

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
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS);

        match self.console_tab {
            ConsoleTab::Output => {
                if self.console.is_empty() {
                    body.child(muted_line("Console cleared.", palette))
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
                    body.child(muted_line("No problems.", palette))
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
            ConsoleTab::TestRun => body.child(muted_line("No test run yet.", palette)),
        }
        .into_any_element()
    }

    fn delete_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.pending_delete?;
        let name = self
            .find_file(id)
            .map(|f| f.name.clone())
            .unwrap_or_default();

        let card = confirm_modal(
            "Delete script?",
            "This permanently removes the script file. This cannot be undone.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "script-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "script-delete-confirm",
            "Delete",
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
                    .child("API reference"),
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

        let mut any = false;
        for group in api_catalog() {
            let matches: Vec<&&str> = group
                .fns
                .iter()
                .filter(|sig| query.is_empty() || sig.to_lowercase().contains(&query))
                .collect();
            if matches.is_empty() {
                continue;
            }
            any = true;

            let mut section = div().flex().flex_col().child(
                div()
                    .pt(spacing(Spacing::Sm, Density::Cozy))
                    .pb(spacing(Spacing::Xxs, Density::Cozy))
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(group.label),
            );
            for sig in matches {
                section = section.child(api_fn_row(sig, palette));
            }
            pane = pane.child(section);
        }

        if !any {
            pane = pane.child(
                div()
                    .pt(spacing(Spacing::Sm, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("No matching methods."),
            );
        }

        pane.into_any_element()
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

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(ghost_button("Cancel", palette).on_click(
                "script-run-cancel",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_run(cx)),
            ))
            .child(primary_button("Run", palette).on_click(
                "script-run-submit",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_run(cx)),
            ));

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
            "Discard unsaved changes?",
            "The open script has unsaved edits. Leaving now discards them.",
            ConfirmTone::Warning,
            palette,
        )
        .esc_hint("to keep editing")
        .on_cancel(
            "script-discard-cancel",
            "Keep editing",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_discard(cx)),
        )
        .on_confirm(
            "script-discard-confirm",
            "Discard",
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

        let header = self.page_header(&palette, cx);
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
            .child(code)
            .child(console);

        let right = self.api_docs_open.then(|| self.right_pane(&palette, cx));

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
            .child(toolbar)
            .child(body)
            .children(overlay)
    }
}

fn section_label(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .pt(spacing(Spacing::Sm, Density::Cozy))
        .pb(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XXS)
        .text_color(palette.text_muted)
        .child(label)
}

fn scripts_header(palette: &ForgePalette, cx: &mut Context<ScriptEditorView>) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(section_label("SCRIPTS", palette))
        .child(
            div()
                .id("script-new")
                .cursor_pointer()
                .pr(spacing(Spacing::Xs, Density::Cozy))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_script(cx)))
                .child(icon(Icon::Plus, GLYPH_ACTION, palette.text_faint)),
        )
}

fn variable_row(name: SharedString, ty: SharedString, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .child(div().text_color(palette.warning).child(name))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(ty),
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

fn muted_line(text: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div().text_color(palette.text_faint).child(text)
}

fn api_fn_row(sig: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .child(badge(palette.brand, palette.shell, "fn", true, FONT_XXS))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_primary)
                .child(sig),
        )
}

fn api_catalog() -> &'static [ApiGroup] {
    &[
        ApiGroup {
            label: "FORGE :: CORE",
            fns: &["log(msg)", "warn(msg)", "sleep(ms)"],
        },
        ApiGroup {
            label: "FORGE :: CHAT",
            fns: &["send(text)", "reply(to, text)", "whisper(user, msg)"],
        },
        ApiGroup {
            label: "FORGE :: GLOBALS",
            fns: &["get(key)", "set(key, val)", "incr(key)"],
        },
        ApiGroup {
            label: "FORGE :: OBS",
            fns: &["set_scene(n)", "toggle_source(n)", "set_mute(n, b)"],
        },
        ApiGroup {
            label: "FORGE :: HTTP",
            fns: &["get(url)", "post(url, b)"],
        },
    ]
}

fn seed_run_result(name: &str) -> Vec<ConsoleLine> {
    vec![
        ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Run,
            text: format!("{name} with inputs").into(),
        },
        ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Info,
            text: "quote #2 by GLaDOS".into(),
        },
        ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Ok,
            text: "returned: \"The cake is a lie.\" - GLaDOS".into(),
        },
        ConsoleLine {
            time: "--:--:--".into(),
            tag: LogTag::Stats,
            text: "executed in 1.84ms · 0 errors".into(),
        },
    ]
}

fn seed_scripts() -> (Vec<ScriptFolder>, Vec<ScriptFile>) {
    let format_quote = "\
// Pick a random quote and format with author
// @input  lines: Array<string>
// @input  idx: int
// @return string

fn format_quote(lines, idx) {
    let raw = lines[idx];
    let parts = raw.split(\"|\");

    if parts.len() < 2 {
        return raw.trim();
    }

    let quote = parts[0].trim();
    let author = parts[1].trim();

    forge::log(`quote #${idx} by ${author}`);
    return `\"${quote}\" - ${author}`;
}
";

    let shoutout = "\
// Shout out a raider
fn shoutout(user) {
    forge::chat::send(`Go follow ${user}! <3`);
}
";

    let remind = "\
// Post a periodic social reminder
fn remind() {
    forge::chat::send(\"Follow on socials - links in panels!\");
}
";

    let utils = "\
// Shared helpers
fn clamp(n, lo, hi) {
    if n < lo { return lo; }
    if n > hi { return hi; }
    n
}
";

    let api_helpers = "\
// Shared API helpers
fn json_get(url) {
    forge::http::get(url)
}
";

    let folders = vec![
        ScriptFolder {
            name: "!quote".to_owned(),
            expanded: true,
            files: vec![ScriptFile {
                id: 0,
                name: "format_quote.rhai".to_owned(),
                content: format_quote.to_owned(),
                dirty: false,
            }],
        },
        ScriptFolder {
            name: "!so".to_owned(),
            expanded: false,
            files: vec![ScriptFile {
                id: 1,
                name: "shoutout.rhai".to_owned(),
                content: shoutout.to_owned(),
                dirty: false,
            }],
        },
        ScriptFolder {
            name: "SocialReminder".to_owned(),
            expanded: false,
            files: vec![ScriptFile {
                id: 2,
                name: "remind.rhai".to_owned(),
                content: remind.to_owned(),
                dirty: false,
            }],
        },
    ];

    let shared = vec![
        ScriptFile {
            id: 3,
            name: "utils.rhai".to_owned(),
            content: utils.to_owned(),
            dirty: false,
        },
        ScriptFile {
            id: 4,
            name: "api_helpers.rhai".to_owned(),
            content: api_helpers.to_owned(),
            dirty: false,
        },
    ];

    (folders, shared)
}

fn seed_console() -> Vec<ConsoleLine> {
    vec![
        ConsoleLine {
            time: "14:23:14".into(),
            tag: LogTag::Run,
            text: "format_quote.rhai with sample inputs".into(),
        },
        ConsoleLine {
            time: "14:23:14".into(),
            tag: LogTag::Info,
            text: "quote #2 by GLaDOS".into(),
        },
        ConsoleLine {
            time: "14:23:14".into(),
            tag: LogTag::Ok,
            text: "returned: \"The cake is a lie.\" - GLaDOS".into(),
        },
        ConsoleLine {
            time: "14:23:14".into(),
            tag: LogTag::Stats,
            text: "executed in 1.84ms · 3 allocations · 0 errors".into(),
        },
    ]
}
