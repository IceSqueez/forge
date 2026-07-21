use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use forge_components::chip::ChipGlyph;
use forge_components::confirm::ConfirmTone;
use forge_components::tokens::ModalSize;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ColumnWidth, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, DataRow,
    Density, FONT_XS, FONT_XXS, ForgePalette, Icon, InlineEdit, InlineEditEvent, InputEvent,
    OverlayPosition, Radius, Spacing, TextArea, TextInput, ToastAction, ToastKind, badge,
    breadcrumb, chip, column, confirm_modal, context_menu, empty_state, fmt_relative_time,
    hover_reveal, icon, inline_edit, menu_divider, menu_item, modal, overlay, primary_button,
    primary_button_with_icon, radius, search_input, secondary_button, spacing, status_dot, toggle,
    toolbar_row, tr, virtual_table, with_alpha,
};
use std::path::PathBuf;

use forge_storage::{GlobalEntry, GlobalsExport, GlobalsRepo};
use forge_types::{Variant, VariantKind};
use gpui::{
    App, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Rgba, SharedString,
    Subscription, UniformListScrollHandle, Window, div, prelude::*, px, svg,
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

const EXPORT_CANCELLED: &str = "export cancelled";

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
                .map_err(|_| tr!("globals_error_invalid_int").into()),
            VariantKind::Float => {
                let raw = self.value_input.read(cx).content().trim().to_owned();
                let parsed = raw
                    .parse::<f64>()
                    .map_err(|_| tr!("globals_error_invalid_float"))?;
                Variant::float(parsed).map_err(|_| tr!("globals_error_invalid_float").into())
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
            .map_err(|_| tr!("globals_error_invalid_datetime").into()),
            VariantKind::Array => parse_json_variant(
                self.value_area.read(cx).content(),
                true,
                tr!("globals_error_invalid_json_array").into(),
            ),
            VariantKind::Object => parse_json_variant(
                self.value_area.read(cx).content(),
                false,
                tr!("globals_error_invalid_json_object").into(),
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
    editor: Entity<InlineEdit>,
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
    visible: Vec<Global>,
    editor: Option<EditorState>,
    pending_delete: Option<SharedString>,
    inspecting: Option<Global>,
    renaming: Option<RenameState>,
    row_menu: Option<RowMenu>,
    table_scroll: UniformListScrollHandle,
    _globals_obs: Subscription,
    _search_sub: Subscription,
}

struct RowMenu {
    name: SharedString,
    position: gpui::Point<gpui::Pixels>,
}

impl GlobalsView {
    pub fn new(
        globals: Entity<Globals>,
        backend: Arc<dyn GlobalsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = cx.new(|cx| search_input(tr!("globals_search_placeholder"), palette, cx));

        let globals_obs = cx.observe(&globals, Self::on_globals_changed);
        let search_sub = cx.subscribe(&search, Self::on_search_event);

        let view = Self {
            globals,
            backend,
            rt_handle,
            loading: true,
            filter: GlobalsFilter::default(),
            search,
            search_query: String::new(),
            visible: Vec::new(),
            editor: None,
            pending_delete: None,
            inspecting: None,
            renaming: None,
            row_menu: None,
            table_scroll: UniformListScrollHandle::new(),
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
                view.update(cx, |this, cx| this.apply_entries(entries, cx));
            }
            Ok(Err(message)) => {
                view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_entries(&mut self, entries: Vec<GlobalEntry>, cx: &mut Context<Self>) {
        let rows: Vec<Global> = entries.iter().map(global_from_entry).collect();
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
        cx.push_toast(
            ToastKind::Error,
            tr!("globals_toast_error", message = message),
        );
        cx.notify();
    }

    fn on_globals_changed(&mut self, _g: Entity<Globals>, cx: &mut Context<Self>) {
        self.rebuild_visible(cx);
        cx.notify();
    }

    fn rebuild_visible(&mut self, cx: &mut Context<Self>) {
        let query = self.search_query.trim().to_lowercase();
        let rows: Vec<Global> = self
            .globals
            .read(cx)
            .entries()
            .iter()
            .filter(|g| self.filter.keeps(g))
            .filter(|g| query.is_empty() || g.name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.visible = rows;
    }

    fn on_search_event(
        &mut self,
        _f: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search_query = text.to_string();
            self.rebuild_visible(cx);
            cx.notify();
        }
    }

    fn set_filter(&mut self, filter: GlobalsFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.rebuild_visible(cx);
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
        let message = tr!("globals_deleted_toast", name = name.as_ref());
        cx.push_toast_full(
            ToastKind::Undo,
            message,
            None,
            Some(ToastAction::new(
                tr!("common_undo"),
                move |_window, app: &mut App| {
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
                },
            )),
            Duration::from_millis(6000),
        );
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend);
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<PathBuf, String>>();
        self.rt_handle.spawn(async move {
            let _ = tx.send(export_globals_to_chosen_file(repo).await);
        });
        cx.spawn(async move |_this, _cx| match rx.await {
            Ok(Ok(path)) => {
                eprintln!("forge-desktop: globals exported to {}", path.display());
            }
            Ok(Err(reason)) => {
                if reason == EXPORT_CANCELLED {
                    return;
                }
                eprintln!("forge-desktop: globals export failed: {reason}");
            }
            Err(_) => {}
        })
        .detach();
    }

    fn start_rename(&mut self, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let editor = inline_edit(name.to_string(), palette, FONT_XS, window, cx);
        let sub = cx.subscribe(
            &editor,
            |this, _e, event: &InlineEditEvent, cx| match event {
                InlineEditEvent::Commit(next) => this.commit_rename(next.clone(), cx),
                InlineEditEvent::Cancel => this.cancel_rename(cx),
            },
        );
        self.renaming = Some(RenameState {
            original: name,
            editor,
            _sub: sub,
        });
        cx.notify();
    }

    fn commit_rename(&mut self, next: String, cx: &mut Context<Self>) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        cx.notify();
        let old = rename.original.to_string();
        let next = next.trim().to_owned();
        if next.is_empty() || next == old {
            return;
        }
        if self.globals.read(cx).contains(&next) {
            cx.push_toast(
                ToastKind::Error,
                tr!("globals_rename_taken", name = next.as_str()),
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
        editor.name_input.update(cx, |f, cx| f.focus(window, cx));
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
        editor.name_input.update(cx, |f, cx| f.focus(window, cx));
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
            let mut ti =
                TextInput::new(tr!("globals_editor_name_placeholder"), cx).with_palette(palette);
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
            let mut ta = TextArea::new("[1, 2, 3]", cx)
                .with_palette(palette)
                .mono()
                .json_highlight();
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
            self.set_editor_error_owned(tr!("globals_error_name_required").into(), cx);
            return;
        }
        let collides =
            self.globals.read(cx).contains(&name) && original.as_deref() != Some(name.as_str());
        if collides {
            self.set_editor_error_owned(tr!("globals_error_name_taken").into(), cx);
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

    fn set_editor_error_owned(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(ed) = self.editor.as_mut() {
            ed.error = Some(message);
        }
        cx.notify();
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

        let stat = |value: usize, label: SharedString, hue: Rgba| {
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
            .child(stat(
                total,
                tr!("globals_stat_total").into(),
                palette.text_primary,
            ))
            .child(dot())
            .child(stat(
                persisted,
                tr!("globals_stat_persisted").into(),
                palette.success,
            ))
            .child(dot())
            .child(stat(
                session,
                tr!("globals_stat_in_memory").into(),
                palette.warning,
            ));

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("globals_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("globals_breadcrumb_globals")),
            ],
            palette,
        )
        .right(cluster)
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
                tr!("globals_filter_all"),
                GlobalsFilter::All,
                palette.brand,
            ),
            (
                "globals-filter-persisted",
                tr!("globals_filter_persisted"),
                GlobalsFilter::Persisted,
                palette.success,
            ),
            (
                "globals-filter-session",
                tr!("globals_filter_session"),
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

        let export_hover = palette.surface_overlay;
        let export = div()
            .id("globals-export")
            .flex()
            .items_center()
            .justify_center()
            .p(px(5.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(export_hover))
            .child(icon(Icon::Download, px(14.0), palette.success))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.export(cx)));
        let new_btn =
            primary_button_with_icon(Icon::Plus, tr!("globals_editor_title_create"), palette)
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

        toolbar_row(left, right).attached(palette).density(density)
    }

    fn render_table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let rows = &self.visible;

        let count = rows.len();
        let body = if count == 0 {
            let caption: SharedString = if self.loading {
                tr!("globals_loading").into()
            } else {
                tr!("globals_empty_caption").into()
            };
            let mut state = empty_state(caption, palette).density(density);
            if self.loading {
                state = state.loading("globals-loading");
            }
            div()
                .w_full()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(state)
                .into_any_element()
        } else {
            let columns = vec![
                column("", ColumnWidth::Fixed(px(24.0))),
                column(tr!("globals_editor_section_name"), ColumnWidth::Flex(8.0)),
                column(
                    tr!("globals_editor_section_type"),
                    ColumnWidth::Fixed(px(80.0)),
                ),
                column(tr!("globals_editor_section_value"), ColumnWidth::Flex(8.0)),
                column(tr!("globals_col_modified"), ColumnWidth::Fixed(px(120.0))),
                column(
                    tr!("globals_col_reads_writes"),
                    ColumnWidth::Fixed(px(96.0)),
                ),
                column(tr!("globals_col_persist"), ColumnWidth::Fixed(px(64.0))),
                column(tr!("globals_col_actions"), ColumnWidth::Fixed(px(84.0))),
            ];
            let pal = *palette;
            virtual_table(
                "globals-scroll",
                palette,
                columns,
                count,
                &self.table_scroll,
                Density::Compact,
            )
            .build(
                move |this, ix, _window, cx| {
                    let g = this.visible[ix].clone();
                    this.build_row(&g, &pal, cx)
                },
                cx,
            )
        };

        div()
            .id("globals-table")
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(body)
    }

    fn build_row(&self, g: &Global, palette: &ForgePalette, cx: &mut Context<Self>) -> DataRow {
        let group: SharedString = format!("globals-row-{}", g.name).into();
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

        let kind_pill = div().flex().items_center().child(badge(
            palette.surface_overlay,
            variant_kind_color(kind, palette),
            kind_word(kind),
            true,
            px(9.5),
        ));

        let value_cell = self.value_cell(g, palette, cx);

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
                    (gpui::ElementId::from("globals-persist"), name.clone()),
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
                (gpui::ElementId::from("globals-edit"), name.clone()),
                Icon::Edit,
                palette.text_secondary,
                with_alpha(palette.brand, ACTION_HOVER_ALPHA),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_edit(edit_name.clone(), window, cx)
                }),
            ))
            .child(self.row_action(
                (gpui::ElementId::from("globals-delete"), name.clone()),
                Icon::X,
                palette.text_secondary,
                palette.random,
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
                .child(rename.editor.clone())
                .into_any_element();
        }

        let rename_target = name.clone();
        let menu_target = name.clone();
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.open_row_menu(menu_target.clone(), event.position, cx);
                }),
            )
            .child(name.clone())
            .into_any_element()
    }

    fn open_row_menu(
        &mut self,
        name: SharedString,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.row_menu = Some(RowMenu { name, position });
        cx.notify();
    }

    fn close_row_menu(&mut self, cx: &mut Context<Self>) {
        if self.row_menu.is_some() {
            self.row_menu = None;
            cx.notify();
        }
    }

    fn render_row_context_menu(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let menu = self.row_menu.as_ref()?;
        let name = menu.name.clone();
        let persisted = self
            .globals
            .read(cx)
            .entries()
            .iter()
            .any(|g| g.name == name && g.persisted);
        let view = cx.entity();

        let rename_name = name.clone();
        let rename_view = view.clone();
        let persist_name = name.clone();
        let persist_view = view.clone();
        let delete_name = name.clone();
        let delete_view = view.clone();
        let persist_label = if persisted {
            tr!("globals_menu_session_only")
        } else {
            tr!("globals_menu_persist")
        };

        let items = vec![
            menu_item(
                "globals-menu-rename",
                tr!("globals_menu_rename"),
                move |_e, window, cx| {
                    let name = rename_name.clone();
                    rename_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.start_rename(name, window, cx);
                    });
                },
            )
            .icon(Icon::Pencil)
            .into(),
            menu_item(
                "globals-menu-persist",
                persist_label,
                move |_e, _window, cx| {
                    let name = persist_name.clone();
                    persist_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.toggle_persist(name, cx);
                    });
                },
            )
            .icon(Icon::Pin)
            .into(),
            menu_divider(),
            menu_item(
                "globals-menu-delete",
                tr!("common_delete"),
                move |_e, _window, cx| {
                    let name = delete_name.clone();
                    delete_view.update(cx, |this, cx| {
                        this.close_row_menu(cx);
                        this.request_delete(name, cx);
                    });
                },
            )
            .icon(Icon::X)
            .color(palette.random)
            .into(),
        ];

        Some(
            context_menu(menu.position, palette)
                .items(items)
                .on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_row_menu(cx));
                })
                .into_any_element(),
        )
    }

    fn value_cell(
        &self,
        g: &Global,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let complex = matches!(g.kind(), VariantKind::Array | VariantKind::Object);
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
            let group: SharedString = format!("globals-inspect-{}", g.name).into();
            let hover_bg = palette.surface_overlay;
            let idle = palette.text_secondary;
            let active = palette.brand;
            let target = g.clone();
            cell = cell.child(
                div()
                    .id((gpui::ElementId::from("globals-inspect"), g.name.clone()))
                    .group(group.clone())
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(2.0))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_inspect(target.clone(), cx);
                    }))
                    .child(
                        svg()
                            .flex_none()
                            .size(VALUE_ICON)
                            .path(Icon::ExternalLink.path())
                            .text_color(idle)
                            .group_hover(group, move |s| s.text_color(active)),
                    ),
            );
        }
        cell.into_any_element()
    }

    fn open_inspect(&mut self, g: Global, cx: &mut Context<Self>) {
        self.inspecting = Some(g);
        cx.notify();
    }

    fn close_inspect(&mut self, cx: &mut Context<Self>) {
        self.inspecting = None;
        cx.notify();
    }

    fn row_action(
        &self,
        id: impl Into<gpui::ElementId>,
        glyph: Icon,
        idle: Rgba,
        hover_bg: Rgba,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id.into())
            .flex()
            .items_center()
            .justify_center()
            .p(px(4.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(handler)
            .child(icon(glyph, FONT_XS, idle))
    }

    fn render_editor(
        &self,
        ed: &EditorState,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = match ed.mode {
            EditorMode::Create => tr!("globals_editor_title_create"),
            EditorMode::Edit(_) => tr!("globals_editor_title_edit"),
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
        let name_section = section(palette, tr!("globals_editor_section_name"), name_row);

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
                    .child(tr!("globals_editor_type_locked_hint")),
            );
        }
        let type_section = section(palette, tr!("globals_editor_section_type"), type_children);

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
                            .child(tr!("globals_editor_persist_label")),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(tr!("globals_editor_persist_desc")),
                    ),
            )
            .child(toggle(ed.persisted, palette).on_click(
                "globals-editor-persist",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_editor_persist(cx)),
            ));
        let persist_section = section(
            palette,
            tr!("globals_editor_section_persistence"),
            persist_row,
        );

        let value_control = self.editor_value_control(ed, palette, cx);
        let value_section = section(palette, tr!("globals_editor_section_value"), value_control);

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
        let save_label: SharedString = if ed.saving {
            tr!("globals_editor_saving").into()
        } else {
            tr!("globals_editor_save").into()
        };
        let cancel = secondary_button(tr!("globals_editor_cancel"), palette).on_click(
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
            .kbd_hint(tr!("globals_editor_kbd_hint"))
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
            tr!("globals_delete_confirm_title"),
            tr!("globals_delete_confirm_body"),
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "globals-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "globals-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
    }

    fn render_inspect(
        &self,
        g: &Global,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let kind = g.kind();
        let (count_key, count) = inspect_count(&g.value);
        let subtitle = tr!(count_key, kind = kind_word(kind), count = count as i64);

        let json = serde_json::to_string_pretty(&g.value.to_plain_json()).unwrap_or_default();
        let mut listing = div().flex().flex_col();
        for (i, line) in json.lines().enumerate() {
            let mut code_line = div().flex_1().flex();
            if line.is_empty() {
                code_line = code_line.child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .child("\u{00A0}"),
                );
            } else {
                for (text, hue) in json_line_runs(line, palette) {
                    code_line = code_line.child(
                        div()
                            .flex_none()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(hue)
                            .whitespace_nowrap()
                            .child(SharedString::from(text)),
                    );
                }
            }
            listing = listing.child(
                div()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .w(px(34.0))
                            .pr(px(12.0))
                            .flex()
                            .justify_end()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_faint)
                            .child(format!("{}", i + 1)),
                    )
                    .child(code_line),
            );
        }
        let code = div()
            .w_full()
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(radius(Radius::Md))
            .py(px(10.0))
            .px(px(12.0))
            .child(listing);
        let body = div()
            .id("globals-inspect-scroll")
            .w_full()
            .max_h(px(400.0))
            .overflow_y_scroll()
            .child(code);

        let hint = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_faint)
            .child(tr!("globals_inspect_snapshot"));
        let close = secondary_button(tr!("globals_inspect_close"), palette).on_click(
            "globals-inspect-close-btn",
            cx.listener(|this, _: &ClickEvent, _, cx| this.close_inspect(cx)),
        );
        let edit_name = g.name.clone();
        let edit = primary_button_with_icon(Icon::Edit, tr!("globals_inspect_edit"), palette)
            .on_click(
                "globals-inspect-edit-btn",
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.inspecting = None;
                    this.open_edit(edit_name.clone(), window, cx);
                }),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(hint)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, Density::Cozy))
                    .child(close)
                    .child(edit),
            );

        let card = modal(g.name.clone(), body, palette)
            .subtitle(subtitle)
            .header_icon(Icon::Code, variant_kind_color(kind, palette))
            .size(ModalSize::Md)
            .footer(footer)
            .on_close(
                "globals-inspect-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_inspect(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("globals-inspect-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.close_inspect(cx));
            })
            .into_any_element()
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
        let inspect_overlay = self
            .inspecting
            .clone()
            .map(|g| self.render_inspect(&g, &palette, cx));

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
            .children(inspect_overlay)
            .children(self.render_row_context_menu(&palette, cx))
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
    label: impl Into<SharedString>,
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
                .child(label.into()),
        )
        .child(control)
}

fn inspect_count(value: &Variant) -> (&'static str, usize) {
    match value {
        Variant::Array(items) => ("globals_inspect_subtitle_items", items.len()),
        Variant::Object(map) => ("globals_inspect_subtitle_keys", map.len()),
        _ => ("globals_inspect_subtitle_keys", 0),
    }
}

/// Splits one line of pretty-printed JSON into colored spans: object keys, string
/// values, numbers, and `true`/`false`/`null` literals each get their own hue;
/// punctuation and whitespace fall back to the muted default.
fn json_line_runs(line: &str, palette: &ForgePalette) -> Vec<(String, Rgba)> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut runs: Vec<(String, Rgba)> = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            let start = i;
            while i < n && chars[i].is_whitespace() {
                i += 1;
            }
            runs.push((chars[start..i].iter().collect(), palette.text_secondary));
        } else if c == '"' {
            let start = i;
            i += 1;
            while i < n {
                match chars[i] {
                    '\\' => i += 2,
                    '"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            let mut j = i;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            let is_key = j < n && chars[j] == ':';
            let hue = if is_key {
                palette.info
            } else {
                palette.success
            };
            runs.push((chars[start..i.min(n)].iter().collect(), hue));
        } else if c == '-' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < n
                && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-'))
            {
                i += 1;
            }
            runs.push((chars[start..i].iter().collect(), palette.bits));
        } else if let Some(word) = ["true", "false", "null"]
            .into_iter()
            .find(|w| chars[i..].starts_with(&w.chars().collect::<Vec<_>>()[..]))
        {
            runs.push((word.to_owned(), palette.brand));
            i += word.chars().count();
        } else {
            runs.push((c.to_string(), palette.text_secondary));
            i += 1;
        }
    }
    runs
}

async fn export_globals_to_chosen_file(repo: Arc<dyn GlobalsRepo>) -> Result<PathBuf, String> {
    let entries = repo.export_all().await.map_err(|e| e.to_string())?;
    let envelope = GlobalsExport::new(entries);
    let json = serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())?;
    let default_name = format!(
        "forge-globals-{}.json",
        time::OffsetDateTime::now_utc().unix_timestamp()
    );
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(&default_name)
        .save_file()
        .await
    else {
        return Err(EXPORT_CANCELLED.to_owned());
    };
    let path = handle.path().to_path_buf();
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path)
}

fn global_from_entry(entry: &GlobalEntry) -> Global {
    Global {
        name: entry.name.clone().into(),
        value: entry.value.clone(),
        persisted: entry.persisted,
        reads: entry.reads,
        writes: entry.writes,
        modified: fmt_relative_time(Some(entry.last_modified)).into(),
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
            serde_json::to_string_pretty(&value.to_plain_json()).ok()
        }
        _ => None,
    }
}

fn parse_json_variant(
    text: &str,
    want_array: bool,
    reason: SharedString,
) -> Result<Variant, SharedString> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| reason.clone())?;
    let shape_ok = if want_array {
        value.is_array()
    } else {
        value.is_object()
    };
    if !shape_ok {
        return Err(reason);
    }
    Variant::from_json(value).map_err(|_| reason)
}
