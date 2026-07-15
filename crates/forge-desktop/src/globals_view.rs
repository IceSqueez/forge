use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_components::chip::ChipGlyph;
use forge_components::confirm::ConfirmTone;
use forge_components::tokens::ModalSize;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ColumnWidth, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, DataRow,
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing,
    TextArea, TextInput, ToastAction, ToastKind, badge, breadcrumb, chip, confirm_modal,
    data_table, ghost_button_with_icon, hover_reveal, icon, modal, overlay, primary_button,
    primary_button_with_icon, radius, search_input, secondary_button, spacing, status_dot, toggle,
    with_alpha,
};
use forge_storage::{GlobalEntry, GlobalsRepo};
use forge_types::{Variant, VariantKind};
use gpui::{
    App, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::globals::{Global, Globals, GlobalsFilter, variant_kind_color};
use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;

const EDITOR_KINDS: [VariantKind; 7] = [
    VariantKind::Int,
    VariantKind::Float,
    VariantKind::Bool,
    VariantKind::String,
    VariantKind::Datetime,
    VariantKind::Array,
    VariantKind::Object,
];

const NAME_LIMIT: usize = 64;

const ROW_DOT: gpui::Pixels = px(6.0);
const VALUE_ICON: gpui::Pixels = px(11.0);
const ACTION_HOVER_ALPHA: f32 = 0.14;

#[derive(Clone)]
enum EditorMode {
    Create,
    Edit(SharedString),
}

struct EditorState {
    mode: EditorMode,
    kind: VariantKind,
    persisted: bool,
    bool_value: bool,
    name_input: Entity<TextInput>,
    value_input: Entity<TextInput>,
    value_area: Entity<TextArea>,
    error: Option<SharedString>,
    saving: bool,
    _name_sub: Subscription,
    _value_sub: Subscription,
    _area_sub: Subscription,
}

impl EditorState {
    fn build_variant(&self, cx: &gpui::App) -> Result<Variant, SharedString> {
        match self.kind {
            VariantKind::Int => self
                .value_input
                .read(cx)
                .content()
                .trim()
                .parse::<i64>()
                .map(Variant::Int)
                .map_err(|_| "Invalid integer".into()),
            VariantKind::Float => {
                let raw = self.value_input.read(cx).content().trim().to_owned();
                let parsed = raw.parse::<f64>().map_err(|_| "Invalid float")?;
                Variant::float(parsed).map_err(|_| "Invalid float".into())
            }
            VariantKind::Bool => Ok(Variant::Bool(self.bool_value)),
            VariantKind::String => Ok(Variant::String(
                self.value_input.read(cx).content().to_owned(),
            )),
            VariantKind::Datetime => time::OffsetDateTime::parse(
                self.value_input.read(cx).content().trim(),
                &time::format_description::well_known::Rfc3339,
            )
            .map(Variant::Datetime)
            .map_err(|_| "Invalid ISO 8601 datetime (e.g. 2026-05-18T14:23:00Z)".into()),
            VariantKind::Array => parse_json_variant(
                self.value_area.read(cx).content(),
                true,
                "Invalid JSON array",
            ),
            VariantKind::Object => parse_json_variant(
                self.value_area.read(cx).content(),
                false,
                "Invalid JSON object",
            ),
        }
    }

    fn name(&self, cx: &gpui::App) -> String {
        self.name_input.read(cx).content().trim().to_owned()
    }

    fn original_name(&self) -> Option<&str> {
        match &self.mode {
            EditorMode::Create => None,
            EditorMode::Edit(original) => Some(original.as_ref()),
        }
    }
}

struct RenameState {
    original: SharedString,
    input: Entity<TextInput>,
    _sub: Subscription,
}

pub struct GlobalsView {
    globals: Entity<Globals>,
    backend: Arc<dyn GlobalsRepo>,
    rt_handle: tokio::runtime::Handle,
    loading: bool,
    filter: GlobalsFilter,
    search: Entity<TextInput>,
    search_query: String,
    editor: Option<EditorState>,
    pending_delete: Option<SharedString>,
    renaming: Option<RenameState>,
    _globals_obs: Subscription,
    _search_sub: Subscription,
}

impl GlobalsView {
    pub fn new(
        globals: Entity<Globals>,
        backend: Arc<dyn GlobalsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = cx.new(|cx| search_input("Search variables…", palette, cx));

        let globals_obs = cx.observe(&globals, |_, _, cx| cx.notify());
        let search_sub = cx.subscribe(&search, Self::on_search_event);

        let view = Self {
            globals,
            backend,
            rt_handle,
            loading: true,
            filter: GlobalsFilter::default(),
            search,
            search_query: String::new(),
            editor: None,
            pending_delete: None,
            renaming: None,
            _globals_obs: globals_obs,
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.backend);
        self.spawn_reload(
            async move { backend.list().await.map_err(|e| e.to_string()) },
            cx,
        );
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<GlobalEntry>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    fn reload_entity(
        view: Entity<GlobalsView>,
        rt_handle: tokio::runtime::Handle,
        work: impl Future<Output = Result<Vec<GlobalEntry>, String>> + Send + 'static,
        app: &mut App,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        app.spawn(async move |cx| match rx.await {
            Ok(Ok(entries)) => {
                let _ = view.update(cx, |this, cx| this.apply_entries(entries, cx));
            }
            Ok(Err(message)) => {
                let _ = view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_entries(&mut self, entries: Vec<GlobalEntry>, cx: &mut Context<Self>) {
        let now = time::OffsetDateTime::now_utc();
        let rows: Vec<Global> = entries.iter().map(|e| global_from_entry(e, now)).collect();
        self.globals.update(cx, |g, cx| {
            g.set_all(rows);
            cx.notify();
        });
        self.loading = false;
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: globals operation failed: {message}");
        self.loading = false;
        cx.push_toast(ToastKind::Error, format!("Globals: {message}"));
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _f: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search_query = text.to_string();
            cx.notify();
        }
    }

    fn set_filter(&mut self, filter: GlobalsFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    fn toggle_persist(&mut self, name: SharedString, cx: &mut Context<Self>) {
        let next = !self
            .globals
            .read(cx)
            .entries()
            .iter()
            .any(|g| g.name == name && g.persisted);
        let backend = Arc::clone(&self.backend);
        let key = name.to_string();
        self.spawn_reload(
            async move {
                backend
                    .set_persisted(&key, next)
                    .await
                    .map_err(|e| e.to_string())?;
                backend.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn request_delete(&mut self, name: SharedString, cx: &mut Context<Self>) {
        self.pending_delete = Some(name);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.pending_delete.take() else {
            return;
        };
        cx.notify();

        let backend = Arc::clone(&self.backend);
        let key = name.to_string();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<GlobalEntry>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                backend.archive(&key).await.map_err(|e| e.to_string())?;
                backend.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });

        let restore_backend = Arc::clone(&self.backend);
        let restore_rt = self.rt_handle.clone();
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(entries)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_entries(entries, cx);
                    this.raise_undo_toast(name, restore_backend, restore_rt, cx);
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn raise_undo_toast(
        &self,
        name: SharedString,
        backend: Arc<dyn GlobalsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        let message = format!("Deleted \u{201c}{name}\u{201d}");
        cx.push_toast_full(
            ToastKind::Undo,
            message,
            None,
            Some(ToastAction::new("Undo", move |_window, app: &mut App| {
                let backend = Arc::clone(&backend);
                let rt_handle = rt_handle.clone();
                let key = name.to_string();
                Self::reload_entity(
                    view.clone(),
                    rt_handle,
                    async move {
                        backend.restore(&key).await.map_err(|e| e.to_string())?;
                        backend.list().await.map_err(|e| e.to_string())
                    },
                    app,
                );
            })),
            Duration::from_millis(6000),
        );
    }

    fn export(&mut self, _cx: &mut Context<Self>) {}

    fn start_rename(&mut self, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let seed = name.clone();
        let input = cx.new(|cx| {
            let mut ti = TextInput::new("", cx).with_palette(palette);
            ti.set_content(seed.to_string(), cx);
            ti
        });
        let sub = cx.subscribe(&input, |this, _f, event: &InputEvent, cx| match event {
            InputEvent::Submitted(_) => this.commit_rename(cx),
            InputEvent::Cancelled => this.cancel_rename(cx),
            InputEvent::Changed(_) => {}
        });
        input.read(cx).focus(window);
        self.renaming = Some(RenameState {
            original: name,
            input,
            _sub: sub,
        });
        cx.notify();
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        cx.notify();
        let old = rename.original.to_string();
        let next = rename.input.read(cx).content().trim().to_owned();
        if next.is_empty() || next == old {
            return;
        }
        if self.globals.read(cx).contains(&next) {
            cx.push_toast(
                ToastKind::Error,
                format!("Name \u{201c}{next}\u{201d} is already taken"),
            );
            return;
        }
        let backend = Arc::clone(&self.backend);
        self.spawn_reload(
            async move {
                backend
                    .rename(&old, &next)
                    .await
                    .map_err(|e| e.to_string())?;
                backend.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.build_editor(EditorMode::Create, VariantKind::Int, false, None, cx);
        editor.name_input.read(cx).focus(window);
        self.editor = Some(editor);
        cx.notify();
    }

    fn open_edit(&mut self, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let Some(global) = self
            .globals
            .read(cx)
            .entries()
            .iter()
            .find(|g| g.name == name)
            .cloned()
        else {
            return;
        };
        let kind = global.kind();
        let editor = self.build_editor(
            EditorMode::Edit(name),
            kind,
            global.persisted,
            Some(&global),
            cx,
        );
        editor.name_input.read(cx).focus(window);
        self.editor = Some(editor);
        cx.notify();
    }

    fn build_editor(
        &self,
        mode: EditorMode,
        kind: VariantKind,
        persisted: bool,
        prefill: Option<&Global>,
        cx: &mut Context<Self>,
    ) -> EditorState {
        let palette = cx.palette();
        let name_seed = prefill.map(|g| g.name.to_string()).unwrap_or_default();
        let name_input = cx.new(|cx| {
            let mut ti = TextInput::new("my_variable", cx).with_palette(palette);
            ti.set_content(name_seed, cx);
            ti
        });

        let single_seed = prefill.and_then(|g| single_line_seed(&g.value));
        let value_input = cx.new(|cx| {
            let mut ti = TextInput::new(single_line_placeholder(kind), cx).with_palette(palette);
            if let Some(seed) = single_seed {
                ti.set_content(seed, cx);
            }
            ti
        });

        let area_seed = prefill.and_then(|g| json_seed(&g.value));
        let value_area = cx.new(|cx| {
            let mut ta = TextArea::new("[1, 2, 3]", cx).with_palette(palette);
            if let Some(seed) = area_seed {
                ta.set_content(seed, cx);
            }
            ta
        });

        let bool_value = matches!(prefill.map(|g| &g.value), Some(Variant::Bool(true)));

        let name_sub = cx.subscribe(
            &name_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit_editor(cx),
                InputEvent::Cancelled => this.cancel_editor(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit_editor(cx),
                InputEvent::Cancelled => this.cancel_editor(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );
        let area_sub = cx.subscribe(&value_area, |_this, _f, _event: &InputEvent, cx| {
            cx.notify()
        });

        EditorState {
            mode,
            kind,
            persisted,
            bool_value,
            name_input,
            value_input,
            value_area,
            error: None,
            saving: false,
            _name_sub: name_sub,
            _value_sub: value_sub,
            _area_sub: area_sub,
        }
    }

    fn cancel_editor(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        cx.notify();
    }

    fn select_kind(&mut self, kind: VariantKind, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let is_create = matches!(
            self.editor.as_ref().map(|e| &e.mode),
            Some(EditorMode::Create)
        );
        if !is_create {
            return;
        }
        let value_input =
            cx.new(|cx| TextInput::new(single_line_placeholder(kind), cx).with_palette(palette));
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit_editor(cx),
                InputEvent::Cancelled => this.cancel_editor(cx),
                InputEvent::Changed(_) => cx.notify(),
            },
        );
        if let Some(ed) = self.editor.as_mut() {
            ed.kind = kind;
            ed.error = None;
            ed.value_input = value_input;
            ed._value_sub = value_sub;
        }
        cx.notify();
    }

    fn toggle_editor_persist(&mut self, cx: &mut Context<Self>) {
        if let Some(ed) = self.editor.as_mut() {
            ed.persisted = !ed.persisted;
        }
        cx.notify();
    }

    fn toggle_bool_value(&mut self, cx: &mut Context<Self>) {
        if let Some(ed) = self.editor.as_mut() {
            ed.bool_value = !ed.bool_value;
        }
        cx.notify();
    }

    fn submit_editor(&mut self, cx: &mut Context<Self>) {
        let Some(ed) = self.editor.as_ref() else {
            return;
        };
        if ed.saving {
            return;
        }
        let name = ed.name(cx);
        let original = ed.original_name().map(str::to_owned);
        let build = ed.build_variant(cx);

        if name.is_empty() {
            self.set_editor_error("Name is required", cx);
            return;
        }
        let collides =
            self.globals.read(cx).contains(&name) && original.as_deref() != Some(name.as_str());
        if collides {
            self.set_editor_error("A global with this name already exists", cx);
            return;
        }
        let variant = match build {
            Ok(v) => v,
            Err(reason) => {
                self.set_editor_error_owned(reason, cx);
                return;
            }
        };
        let persisted = ed.persisted;
        let rename_from = match &original {
            Some(old) if old.as_str() != name.as_str() => Some(old.clone()),
            _ => None,
        };
        if let Some(ed) = self.editor.as_mut() {
            ed.saving = true;
            ed.error = None;
        }
        cx.notify();

        let backend = Arc::clone(&self.backend);
        let target = name.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<Vec<GlobalEntry>, String>>();
        self.rt_handle.spawn(async move {
            let outcome = async {
                if let Some(old) = rename_from {
                    backend
                        .rename(&old, &target)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                backend
                    .set(&target, variant, persisted)
                    .await
                    .map_err(|e| e.to_string())?;
                backend.list().await.map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(outcome);
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(entries)) => {
                let _ = this.update(cx, |this, cx| {
                    this.apply_entries(entries, cx);
                    this.editor = None;
                    cx.notify();
                });
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| {
                    if let Some(ed) = this.editor.as_mut() {
                        ed.saving = false;
                        ed.error = Some(message.into());
                    }
                    cx.notify();
                });
            }
            Err(_) => {}
        })
        .detach();
    }

    fn set_editor_error(&mut self, message: &'static str, cx: &mut Context<Self>) {
        self.set_editor_error_owned(message.into(), cx);
    }

    fn set_editor_error_owned(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(ed) = self.editor.as_mut() {
            ed.error = Some(message);
        }
        cx.notify();
    }

    fn visible_rows(&self, cx: &Context<Self>) -> Vec<Global> {
        let query = self.search_query.trim().to_lowercase();
        self.globals
            .read(cx)
            .entries()
            .iter()
            .filter(|g| self.filter.keeps(g))
            .filter(|g| query.is_empty() || g.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    fn editor_saveable(&self, ed: &EditorState, cx: &Context<Self>) -> bool {
        let name = ed.name(cx);
        if name.is_empty() {
            return false;
        }
        if self.globals.read(cx).contains(&name) && ed.original_name() != Some(name.as_str()) {
            return false;
        }
        ed.build_variant(cx).is_ok()
    }

    fn render_header(
        &self,
        palette: &ForgePalette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let globals = self.globals.read(cx);
        let total = globals.total();
        let persisted = globals.persisted_count();
        let session = globals.session_count();

        let stat = |value: usize, label: &'static str, hue: Rgba| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(hue)
                        .child(format!("{value}")),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(label),
                )
        };
        let dot = || {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("·")
        };

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(stat(total, "total", palette.text_primary))
            .child(dot())
            .child(stat(persisted, "persisted", palette.success))
            .child(dot())
            .child(stat(session, "in-memory", palette.warning));

        breadcrumb(vec![BreadcrumbCrumb::leaf("Global variables")], palette).right(cluster)
    }

    fn render_action_bar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let chips = [
            (
                "globals-filter-all",
                "All",
                GlobalsFilter::All,
                palette.brand,
            ),
            (
                "globals-filter-persisted",
                "Persisted",
                GlobalsFilter::Persisted,
                palette.success,
            ),
            (
                "globals-filter-session",
                "Session",
                GlobalsFilter::Session,
                palette.warning,
            ),
        ];
        let mut chip_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density));
        for (id, label, filter, dot) in chips {
            let active = self.filter == filter;
            chip_row = chip_row.child(
                chip(label, ChipGlyph::Dot(dot), active, palette)
                    .density(density)
                    .on_click(
                        id,
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.set_filter(filter, cx)),
                    ),
            );
        }

        let search = div().w(px(200.0)).child(self.search.clone());

        let left = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(search)
            .child(chip_row);

        let export = ghost_button_with_icon(Icon::Download, "Export JSON", palette)
            .density(density)
            .on_click(
                "globals-export",
                cx.listener(|this, _: &ClickEvent, _, cx| this.export(cx)),
            );
        let new_btn = primary_button_with_icon(Icon::Plus, "New variable", palette)
            .density(density)
            .on_click(
                "globals-new",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_create(window, cx)),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(export)
            .child(new_btn);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(right)
    }

    fn render_table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let rows = self.visible_rows(cx);

        let body = if rows.is_empty() {
            let caption = if self.loading {
                "Loading variables…"
            } else {
                "No variables match this filter."
            };
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .p(spacing(Spacing::Md, density))
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child(caption)
                .into_any_element()
        } else {
            let headers: Vec<SharedString> = vec![
                "".into(),
                "NAME".into(),
                "TYPE".into(),
                "VALUE".into(),
                "LAST MODIFIED".into(),
                "READS · WRITES".into(),
                "PERSIST".into(),
                "ACTIONS".into(),
            ];
            let widths = vec![
                ColumnWidth::Fixed(px(24.0)),
                ColumnWidth::Flex(8.0),
                ColumnWidth::Fixed(px(80.0)),
                ColumnWidth::Flex(8.0),
                ColumnWidth::Fixed(px(120.0)),
                ColumnWidth::Fixed(px(96.0)),
                ColumnWidth::Fixed(px(64.0)),
                ColumnWidth::Fixed(px(84.0)),
            ];
            let data_rows: Vec<DataRow> = rows
                .iter()
                .enumerate()
                .map(|(idx, g)| self.build_row(idx, g, palette, cx))
                .collect();
            data_table(palette, headers, widths, data_rows)
                .density(density)
                .into_any_element()
        };

        div()
            .id("globals-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .child(body)
    }

    fn build_row(
        &self,
        idx: usize,
        g: &Global,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> DataRow {
        let group: SharedString = format!("globals-row-{idx}").into();
        let kind = g.kind();
        let name = g.name.clone();

        let dot_color = if g.persisted {
            palette.brand
        } else {
            palette.warning
        };
        let dot = div()
            .flex()
            .justify_center()
            .child(status_dot(dot_color, ROW_DOT));

        let name_cell = self.name_cell(&name, palette, cx);

        let kind_pill = div().child(badge(
            palette.surface_overlay,
            variant_kind_color(kind, palette),
            kind_word(kind),
            true,
            FONT_XXS,
        ));

        let value_cell = value_preview(g, palette);

        let modified_cell = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(g.modified.clone());

        let rw_cell = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(format!("{} · {}", g.reads, g.writes));

        let toggle_name = name.clone();
        let persist_cell =
            div()
                .flex()
                .justify_center()
                .child(toggle(g.persisted, palette).on_click(
                    ("globals-persist", idx),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.toggle_persist(toggle_name.clone(), cx)
                    }),
                ));

        let edit_name = name.clone();
        let delete_name = name.clone();
        let actions = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.0))
            .child(self.row_action(
                ("globals-edit", idx),
                Icon::Pencil,
                palette.brand,
                palette,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_edit(edit_name.clone(), window, cx)
                }),
            ))
            .child(self.row_action(
                ("globals-delete", idx),
                Icon::X,
                palette.random,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.request_delete(delete_name.clone(), cx)
                }),
            ));
        let actions_cell = hover_reveal(actions, group.clone());

        DataRow::with_reveal_group(
            vec![
                dot.into_any_element(),
                name_cell,
                kind_pill.into_any_element(),
                value_cell.into_any_element(),
                modified_cell.into_any_element(),
                rw_cell.into_any_element(),
                persist_cell.into_any_element(),
                actions_cell.into_any_element(),
            ],
            group,
        )
    }

    fn name_cell(
        &self,
        name: &SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if let Some(rename) = self.renaming.as_ref().filter(|r| &r.original == name) {
            return div()
                .w_full()
                .child(rename.input.clone())
                .into_any_element();
        }

        let rename_target = name.clone();
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        this.start_rename(rename_target.clone(), window, cx);
                    }
                }),
            )
            .child(name.clone())
            .into_any_element()
    }

    fn row_action(
        &self,
        id: impl Into<gpui::ElementId>,
        glyph: Icon,
        hover: Rgba,
        palette: &ForgePalette,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        let wash = with_alpha(hover, ACTION_HOVER_ALPHA);
        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_center()
            .p(px(4.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(wash))
            .on_click(handler)
            .child(icon(glyph, FONT_XS, palette.text_secondary))
    }

    fn render_editor(
        &self,
        ed: &EditorState,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = match ed.mode {
            EditorMode::Create => "New variable",
            EditorMode::Edit(_) => "Edit variable",
        };
        let locked = matches!(ed.mode, EditorMode::Edit(_));

        let name_len = ed
            .name_input
            .read(cx)
            .content()
            .chars()
            .count()
            .min(NAME_LIMIT);
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(div().flex_1().child(ed.name_input.clone()))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_LIMIT}")),
            );
        let name_section = section(palette, "NAME", name_row);

        let mut chips = div()
            .flex()
            .flex_wrap()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (i, &kind) in EDITOR_KINDS.iter().enumerate() {
            let active = ed.kind == kind;
            let mut c = chip(
                kind_word(kind),
                ChipGlyph::Dot(variant_kind_color(kind, palette)),
                active,
                palette,
            );
            if !locked {
                c = c.on_click(
                    ("globals-kind", i),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_kind(kind, cx)),
                );
            }
            chips = chips.child(c);
        }
        let mut type_children = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(chips);
        if locked {
            type_children = type_children.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Type is fixed once a variable exists."),
            );
        }
        let type_section = section(palette, "TYPE", type_children);

        let persist_row = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child("Persist to storage"),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child("Survives restarts. Off keeps it in memory only."),
                    ),
            )
            .child(toggle(ed.persisted, palette).on_click(
                "globals-editor-persist",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_editor_persist(cx)),
            ));
        let persist_section = section(palette, "PERSISTENCE", persist_row);

        let value_control = self.editor_value_control(ed, palette, cx);
        let value_section = section(palette, "VALUE", value_control);

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(name_section)
            .child(type_section)
            .child(persist_section)
            .child(value_section);
        if let Some(err) = ed.error.clone() {
            body = body.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, Density::Cozy))
                    .p(spacing(Spacing::Xs, Density::Cozy))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.random, 0.10))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.random, 0.30))
                    .child(icon(Icon::AlertTriangle, FONT_XS, palette.random))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(err),
                    ),
            );
        }

        let saveable = self.editor_saveable(ed, cx) && !ed.saving;
        let save_label = if ed.saving { "Saving…" } else { "Save" };
        let cancel = secondary_button("Cancel", palette).on_click(
            "globals-editor-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_editor(cx)),
        );
        let save = primary_button(save_label, palette)
            .disabled(!saveable)
            .on_click(
                "globals-editor-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_editor(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(cancel)
            .child(save);

        let card = modal(title, body, palette)
            .header_icon(Icon::Variable, palette.warning)
            .size(ModalSize::Md)
            .footer(footer)
            .kbd_hint("Enter to save · Esc to cancel")
            .on_close(
                "globals-editor-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_editor(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-editor-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_editor(cx));
            })
    }

    fn editor_value_control(
        &self,
        ed: &EditorState,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        match ed.kind {
            VariantKind::Bool => {
                let (label, hue) = if ed.bool_value {
                    ("true", palette.success)
                } else {
                    ("false", palette.random)
                };
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(toggle(ed.bool_value, palette).on_click(
                        "globals-editor-bool",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_bool_value(cx)),
                    ))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(hue)
                            .child(label),
                    )
                    .into_any_element()
            }
            VariantKind::Array | VariantKind::Object => {
                div().child(ed.value_area.clone()).into_any_element()
            }
            _ => div().child(ed.value_input.clone()).into_any_element(),
        }
    }

    fn render_delete_confirm(
        &self,
        name: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            "Delete global variable",
            "This permanently removes the variable and its value.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "globals-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "globals-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
    }
}

impl Render for GlobalsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.render_header(&palette, cx);
        let action_bar = self.render_action_bar(&palette, density, cx);
        let table = self.render_table(&palette, density, cx);

        let delete_overlay = self
            .pending_delete
            .clone()
            .map(|name| self.render_delete_confirm(name, &palette, cx));
        let editor_overlay = self
            .editor
            .as_ref()
            .map(|ed| self.render_editor_boxed(ed, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(action_bar)
            .child(table)
            .children(delete_overlay)
            .children(editor_overlay)
    }
}

impl GlobalsView {
    fn render_editor_boxed(
        &self,
        ed: &EditorState,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.render_editor(ed, palette, cx).into_any_element()
    }
}

fn section(
    palette: &ForgePalette,
    label: &'static str,
    control: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label),
        )
        .child(control)
}

fn value_preview(g: &Global, palette: &ForgePalette) -> impl IntoElement + use<> {
    let kind = g.kind();
    let complex = matches!(kind, VariantKind::Array | VariantKind::Object);
    let text = match &g.value {
        Variant::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    };
    let mut cell = div().flex().items_center().gap(px(4.0)).child(
        div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(text),
    );
    if complex {
        cell = cell.child(icon(Icon::ExternalLink, VALUE_ICON, palette.text_faint));
    }
    cell
}

fn global_from_entry(entry: &GlobalEntry, now: time::OffsetDateTime) -> Global {
    Global {
        name: entry.name.clone().into(),
        value: entry.value.clone(),
        persisted: entry.persisted,
        reads: entry.reads,
        writes: entry.writes,
        modified: format_time_ago(entry.last_modified, now).into(),
    }
}

fn format_time_ago(dt: time::OffsetDateTime, now: time::OffsetDateTime) -> String {
    let secs = (now - dt).whole_seconds().max(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours}h ago")
        } else {
            format!("{hours}h {mins}m ago")
        }
    }
}

fn kind_word(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Int => "int",
        VariantKind::Float => "float",
        VariantKind::Bool => "bool",
        VariantKind::String => "string",
        VariantKind::Datetime => "datetime",
        VariantKind::Array => "array",
        VariantKind::Object => "object",
    }
}

fn single_line_placeholder(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Int => "0",
        VariantKind::Float => "0.0",
        VariantKind::Datetime => "2026-05-18T14:23:00Z",
        _ => "",
    }
}

fn single_line_seed(value: &Variant) -> Option<String> {
    match value {
        Variant::Int(n) => Some(n.to_string()),
        Variant::Float(f) => Some(f.to_string()),
        Variant::String(s) => Some(s.clone()),
        Variant::Datetime(dt) => Some(
            dt.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

fn json_seed(value: &Variant) -> Option<String> {
    match value {
        Variant::Array(_) | Variant::Object(_) => {
            serde_json::to_string_pretty(&variant_to_json(value)).ok()
        }
        _ => None,
    }
}

fn parse_json_variant(
    text: &str,
    want_array: bool,
    reason: &'static str,
) -> Result<Variant, SharedString> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| reason)?;
    let shape_ok = if want_array {
        value.is_array()
    } else {
        value.is_object()
    };
    if !shape_ok {
        return Err(reason.into());
    }
    Variant::from_json(value).map_err(|_| reason.into())
}

fn variant_to_json(value: &Variant) -> serde_json::Value {
    match value {
        Variant::Int(n) => serde_json::Value::from(*n),
        Variant::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Variant::Bool(b) => serde_json::Value::Bool(*b),
        Variant::String(s) => serde_json::Value::String(s.clone()),
        Variant::Datetime(dt) => serde_json::Value::String(
            dt.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ),
        Variant::Array(items) => {
            serde_json::Value::Array(items.iter().map(variant_to_json).collect())
        }
        Variant::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), variant_to_json(v)))
                .collect(),
        ),
    }
}
