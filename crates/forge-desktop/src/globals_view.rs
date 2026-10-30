use forge_components::NAME_FIELD_LIMIT;
use forge_components::TINT_BORDER_ALPHA;
use forge_components::TINT_FILL_ALPHA;
use forge_components::UNDO_TOAST_DURATION;
use std::future::Future;
use std::sync::Arc;

use forge_components::chip::ChipGlyph;
use forge_components::confirm::ConfirmTone;
use forge_components::highlight::Language;
use forge_components::highlighted_text;
use forge_components::tokens::ModalSize;
use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, CodeEditor, ColumnWidth, DataRow, Density, FONT_XS, FONT_XXS,
    ForgePalette, Icon, InlineEdit, InlineEditEvent, InputEvent, OverlayPosition, Radius, Spacing,
    TextInput, ToastAction, ToastKind, badge, body_family, chip, column, confirm_modal,
    context_menu, empty_state, header_stat, header_stats, hover_reveal, icon, inline_edit,
    menu_divider, menu_item, modal, mono_family, overlay, page_frame, primary_button,
    primary_button_with_icon, radius, scaled, secondary_button, spacing, status_dot, toggle, tr,
    virtual_table, with_alpha,
};
use forge_components::{Confirm, SearchState};
use std::path::PathBuf;

use forge_runtime::EventBus;
use forge_storage::{GlobalEntry, GlobalsExport, GlobalsRepo};
use forge_types::{Variant, VariantKind};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, MouseButton, MouseDownEvent, Rgba,
    SharedString, Subscription, Task, UniformListScrollHandle, Window, div, prelude::*, px, svg,
};

use crate::async_bridge;
use crate::error::DesktopError;
use crate::globals::{Global, Globals, GlobalsFilter, variant_kind_color};
use crate::repo_failure::RepoFailure;
use forge_components::ActivePresentation;
use forge_components::PushToast;

const EDITOR_KINDS: [VariantKind; 7] = [
    VariantKind::Int,
    VariantKind::Float,
    VariantKind::Bool,
    VariantKind::String,
    VariantKind::Datetime,
    VariantKind::Array,
    VariantKind::Object,
];

const VALUE_AREA_ROWS: std::ops::RangeInclusive<usize> = 3..=12;

const ROW_DOT: gpui::Pixels = px(6.0);
const VALUE_ICON: gpui::Pixels = px(11.0);
const ACTION_HOVER_ALPHA: f32 = 0.14;

#[derive(Clone)]
enum EditorMode {
    Create,
    Edit(SharedString),
}

struct GlobalDraft {
    name: String,
    variant: Variant,
    persisted: bool,
    rename_from: Option<String>,
}

enum GlobalEditorEvent {
    Submit(GlobalDraft),
    Cancel,
}

struct GlobalEditor {
    mode: EditorMode,
    kind: VariantKind,
    persisted: bool,
    bool_value: bool,
    name_input: Entity<TextInput>,
    value_input: Entity<TextInput>,
    value_area: Entity<CodeEditor>,
    error: Option<SharedString>,
    saving: bool,
    globals: Entity<Globals>,
    _name_sub: Subscription,
    _value_sub: Subscription,
    _area_sub: Subscription,
}

impl EventEmitter<GlobalEditorEvent> for GlobalEditor {}

impl GlobalEditor {
    fn new(
        mode: EditorMode,
        kind: VariantKind,
        persisted: bool,
        prefill: Option<&Global>,
        globals: Entity<Globals>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            let mut ta = CodeEditor::new(Language::Json, json_placeholder(kind), cx)
                .with_palette(palette)
                .with_field_rows(VALUE_AREA_ROWS);
            if let Some(seed) = area_seed {
                ta.set_content(seed, cx);
            }
            ta
        });

        let bool_value = matches!(prefill.map(|g| &g.value), Some(Variant::Bool(true)));

        let name_sub = cx.subscribe(
            &name_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let area_sub = cx.subscribe(&value_area, |_this, _f, _event: &InputEvent, cx| {
            cx.notify()
        });

        GlobalEditor {
            mode,
            kind,
            persisted,
            bool_value,
            name_input,
            value_input,
            value_area,
            error: None,
            saving: false,
            globals,
            _name_sub: name_sub,
            _value_sub: value_sub,
            _area_sub: area_sub,
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input.update(cx, |f, cx| f.focus(window, cx));
    }

    fn select_kind(&mut self, kind: VariantKind, cx: &mut Context<Self>) {
        if !matches!(self.mode, EditorMode::Create) {
            return;
        }
        let palette = cx.palette();
        let value_input =
            cx.new(|cx| TextInput::new(single_line_placeholder(kind), cx).with_palette(palette));
        let value_sub = cx.subscribe(
            &value_input,
            |this, _f, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.submit(cx),
                InputEvent::Cancelled => this.cancel(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Blurred(_) => {}
            },
        );
        let value_area = cx.new(|cx| {
            CodeEditor::new(Language::Json, json_placeholder(kind), cx)
                .with_palette(palette)
                .with_field_rows(VALUE_AREA_ROWS)
        });
        let area_sub = cx.subscribe(&value_area, |_this, _f, _event: &InputEvent, cx| {
            cx.notify()
        });
        self.kind = kind;
        self.error = None;
        self.value_input = value_input;
        self._value_sub = value_sub;
        self.value_area = value_area;
        self._area_sub = area_sub;
        cx.notify();
    }

    fn toggle_persist(&mut self, cx: &mut Context<Self>) {
        self.persisted = !self.persisted;
        cx.notify();
    }

    fn toggle_bool_value(&mut self, cx: &mut Context<Self>) {
        self.bool_value = !self.bool_value;
        cx.notify();
    }

    fn set_error(&mut self, message: SharedString, cx: &mut Context<Self>) {
        self.error = Some(message);
        cx.notify();
    }

    fn is_saveable(&self, cx: &App) -> bool {
        let name = self.name(cx);
        if name.is_empty() {
            return false;
        }
        if self.globals.read(cx).contains(&name) && self.original_name() != Some(name.as_str()) {
            return false;
        }
        self.build_variant(cx).is_ok()
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let name = self.name(cx);
        let original = self.original_name().map(str::to_owned);
        let build = self.build_variant(cx);

        if name.is_empty() {
            self.set_error(tr!("globals_error_name_required").into(), cx);
            return;
        }
        let collides =
            self.globals.read(cx).contains(&name) && original.as_deref() != Some(name.as_str());
        if collides {
            self.set_error(tr!("globals_error_name_taken").into(), cx);
            return;
        }
        let variant = match build {
            Ok(v) => v,
            Err(reason) => {
                self.set_error(reason, cx);
                return;
            }
        };
        let persisted = self.persisted;
        let rename_from = match &original {
            Some(old) if old.as_str() != name.as_str() => Some(old.clone()),
            _ => None,
        };
        self.error = None;
        cx.emit(GlobalEditorEvent::Submit(GlobalDraft {
            name,
            variant,
            persisted,
            rename_from,
        }));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(GlobalEditorEvent::Cancel);
    }

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
            VariantKind::Datetime => {
                forge_types::datetime_from_text(self.value_input.read(cx).content())
                    .map(Variant::Datetime)
                    .ok_or_else(|| tr!("globals_error_invalid_datetime").into())
            }
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
    search: SearchState,
    visible: Vec<Global>,
    editor: Option<Entity<GlobalEditor>>,
    _editor_sub: Option<Subscription>,
    pending_delete: Confirm<SharedString>,
    inspecting: Option<Global>,
    renaming: Option<RenameState>,
    row_menu: Option<RowMenu>,
    table_scroll: UniformListScrollHandle,
    _globals_obs: Subscription,
    _search_sub: Subscription,
    reload_generation: async_bridge::Generation,
    _runtime_writes: Option<Task<()>>,
}

struct RowMenu {
    name: SharedString,
    position: gpui::Point<gpui::Pixels>,
}

const GLOBAL_EVENT_PREFIX: &str = "global.";

impl GlobalsView {
    pub fn new(
        globals: Entity<Globals>,
        backend: Arc<dyn GlobalsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("globals_search_placeholder"));

        let globals_obs = cx.observe(&globals, Self::on_globals_changed);
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);

        let view = Self {
            globals,
            backend,
            rt_handle,
            loading: true,
            filter: GlobalsFilter::default(),
            search,
            visible: Vec::new(),
            editor: None,
            _editor_sub: None,
            pending_delete: Confirm::default(),
            inspecting: None,
            renaming: None,
            row_menu: None,
            table_scroll: UniformListScrollHandle::new(),
            _globals_obs: globals_obs,
            _search_sub: search_sub,
            reload_generation: async_bridge::Generation::default(),
            _runtime_writes: None,
        };
        view.reload(cx);
        view
    }

    pub fn follow_runtime_writes(&mut self, bus: Arc<EventBus>, cx: &mut Context<Self>) {
        self._runtime_writes = Some(cx.spawn(async move |this, cx| {
            async_bridge::drain_events(&bus, cx, move |batch, cx| {
                if !batch
                    .iter()
                    .any(|event| event.kind.starts_with(GLOBAL_EVENT_PREFIX))
                {
                    return async_bridge::BridgeFlow::Continue;
                }
                match this.update(cx, |this, cx| this.reload(cx)) {
                    Ok(()) => async_bridge::BridgeFlow::Continue,
                    Err(_) => async_bridge::BridgeFlow::Stop,
                }
            })
            .await;
        }));
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let backend = Arc::clone(&self.backend);
        self.spawn_reload(
            async move { backend.list().await.map_err(RepoFailure::from) },
            cx,
        );
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<GlobalEntry>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let ticket = self.reload_generation.next();
        async_bridge::run_async(
            &self.rt_handle,
            work,
            move |this, result, cx| match result {
                _ if !this.reload_generation.is_current(ticket) => {}
                Ok(entries) => this.apply_entries(entries, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn spawn_write<T: Send + 'static>(
        &self,
        write: impl Future<Output = Result<T, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        async_bridge::run_async(
            &self.rt_handle,
            write,
            |this, written, cx| this.on_write_settled(written.map(drop), cx),
            cx,
        );
    }

    fn on_write_settled(&mut self, written: Result<(), RepoFailure>, cx: &mut Context<Self>) {
        match written {
            Ok(()) => self.reload(cx),
            Err(failure) => self.on_repo_error(&failure, cx),
        }
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

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::error!(error = %failure, "globals operation failed");
        let message = failure.text();
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("globals_toast_error", message = message.as_str()),
        );
        cx.notify();
    }

    fn on_globals_changed(&mut self, _g: Entity<Globals>, cx: &mut Context<Self>) {
        self.rebuild_visible(cx);
        cx.notify();
    }

    fn empty_caption_key(&self, cx: &App) -> &'static str {
        if self.filter == GlobalsFilter::All && self.globals.read(cx).entries().is_empty() {
            "globals_empty_none"
        } else {
            "globals_empty_caption"
        }
    }

    fn rebuild_visible(&mut self, cx: &mut Context<Self>) {
        let rows: Vec<Global> = self
            .globals
            .read(cx)
            .entries()
            .iter()
            .filter(|g| self.filter.keeps(g))
            .filter(|g| self.search.matches(&g.name))
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
        if self.search.on_changed(event) {
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
        self.spawn_write(
            async move {
                backend
                    .set_persisted(&key, next)
                    .await
                    .map_err(RepoFailure::from)
            },
            cx,
        );
    }

    fn request_delete(&mut self, name: SharedString, cx: &mut Context<Self>) {
        self.pending_delete.request(name);
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete.cancel();
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.pending_delete.take() else {
            return;
        };
        cx.notify();

        let backend = Arc::clone(&self.backend);
        let key = name.to_string();
        let restore_backend = Arc::clone(&self.backend);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                backend.archive(&key).await.map_err(RepoFailure::from)?;
                backend.list().await.map_err(RepoFailure::from)
            },
            move |this, result, cx| match result {
                Ok(entries) => {
                    this.apply_entries(entries, cx);
                    this.raise_undo_toast(name, restore_backend, cx);
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn raise_undo_toast(
        &self,
        name: SharedString,
        backend: Arc<dyn GlobalsRepo>,
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
                    let key = name.to_string();
                    view.update(app, |this, cx| {
                        this.spawn_write(
                            async move { backend.restore(&key).await.map_err(RepoFailure::from) },
                            cx,
                        );
                    });
                },
            )),
            UNDO_TOAST_DURATION,
        );
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.backend);
        async_bridge::run_async(
            &self.rt_handle,
            export_globals_to_chosen_file(repo),
            |_this, result, cx| match result {
                Ok(None) => {}
                Ok(Some(path)) => {
                    let shown = path.display().to_string();
                    cx.push_toast(
                        ToastKind::Success,
                        tr!("globals_export_done", path = shown.as_str()),
                    );
                }
                Err(error) => {
                    tracing::warn!(error = %error, "globals export failed");
                    let shown = error.user_message();
                    cx.push_toast(
                        ToastKind::Error,
                        tr!("globals_export_failed", error = shown.as_str()),
                    );
                }
            },
            cx,
        );
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
        self.spawn_write(
            async move { backend.rename(&old, &next).await.map_err(RepoFailure::from) },
            cx,
        );
    }

    fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        self.renaming = None;
        cx.notify();
    }

    fn open_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let globals = self.globals.clone();
        let editor = cx.new(|cx| {
            GlobalEditor::new(
                EditorMode::Create,
                VariantKind::Int,
                false,
                None,
                globals,
                cx,
            )
        });
        editor.update(cx, |e, cx| e.focus(window, cx));
        self._editor_sub = Some(cx.subscribe(&editor, Self::on_editor_event));
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
        let globals = self.globals.clone();
        let editor = cx.new(|cx| {
            GlobalEditor::new(
                EditorMode::Edit(name),
                kind,
                global.persisted,
                Some(&global),
                globals,
                cx,
            )
        });
        editor.update(cx, |e, cx| e.focus(window, cx));
        self._editor_sub = Some(cx.subscribe(&editor, Self::on_editor_event));
        self.editor = Some(editor);
        cx.notify();
    }

    fn cancel_editor(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        self._editor_sub = None;
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        _editor: Entity<GlobalEditor>,
        event: &GlobalEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GlobalEditorEvent::Submit(draft) => self.persist_editor(draft, cx),
            GlobalEditorEvent::Cancel => self.cancel_editor(cx),
        }
    }

    fn persist_editor(&mut self, draft: &GlobalDraft, cx: &mut Context<Self>) {
        let name = draft.name.clone();
        let variant = draft.variant.clone();
        let persisted = draft.persisted;
        let rename_from = draft.rename_from.clone();
        if let Some(editor) = self.editor.as_ref() {
            editor.update(cx, |e, cx| {
                e.saving = true;
                e.error = None;
                cx.notify();
            });
        }
        cx.notify();

        let backend = Arc::clone(&self.backend);
        let target = name.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                if let Some(old) = rename_from {
                    backend
                        .rename(&old, &target)
                        .await
                        .map_err(RepoFailure::from)?;
                }
                backend
                    .set(&target, variant, persisted)
                    .await
                    .map_err(RepoFailure::from)?;
                backend.list().await.map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(entries) => {
                    this.apply_entries(entries, cx);
                    this.cancel_editor(cx);
                }
                Err(failure) => {
                    if let Some(editor) = this.editor.as_ref() {
                        editor.update(cx, |e, cx| {
                            e.saving = false;
                            e.error = Some(failure.text().into());
                            cx.notify();
                        });
                    }
                    cx.notify();
                }
            },
            cx,
        );
    }

    fn render_stats<'a>(
        &self,
        palette: &'a ForgePalette,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<'a> {
        let density = cx.density();
        let globals = self.globals.read(cx);

        header_stats(
            vec![
                header_stat(
                    globals.total().to_string(),
                    palette.text_primary,
                    tr!("globals_stat_total"),
                ),
                header_stat(
                    globals.persisted_count().to_string(),
                    palette.success,
                    tr!("globals_stat_persisted"),
                ),
                header_stat(
                    globals.session_count().to_string(),
                    palette.warning,
                    tr!("globals_stat_in_memory"),
                ),
            ],
            palette,
            density,
        )
    }

    fn render_filter_left(
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
            chip_row = chip_row.child(chip(label, ChipGlyph::Dot(dot), active, palette).on_click(
                id,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.set_filter(filter, cx)),
            ));
        }

        let search = div().w(px(200.0)).child(self.search.field().clone());

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .child(search)
            .child(chip_row)
    }

    fn render_filter_right(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(export)
            .child(new_btn)
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
                tr!(self.empty_caption_key(cx)).into()
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
                density.tighter(),
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
        let density = cx.density();
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
            .font_family(body_family())
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(g.modified_label());

        let rw_cell = div()
            .font_family(mono_family())
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
            .gap(scaled(px(2.0), density))
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
                palette.danger,
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
            .font_family(mono_family())
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
            .color(palette.danger)
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
        let density = cx.density();
        let complex = matches!(g.kind(), VariantKind::Array | VariantKind::Object);
        let text = match &g.value {
            Variant::String(s) => format!("\"{s}\""),
            other => other.to_string(),
        };
        let mut cell = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(mono_family())
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
}

impl Render for GlobalEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();
        let palette = &palette;
        let ed = &*self;
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
            .min(NAME_FIELD_LIMIT);
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(div().flex_1().child(ed.name_input.clone()))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_FIELD_LIMIT}")),
            );
        let name_section = section(
            palette,
            tr!("globals_editor_section_name"),
            name_row,
            density,
        );

        let mut chips = div().flex().flex_wrap().gap(spacing(Spacing::Xxs, density));
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
            .gap(spacing(Spacing::Xxs, density))
            .child(chips);
        if locked {
            type_children = type_children.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("globals_editor_type_locked_hint")),
            );
        }
        let type_section = section(
            palette,
            tr!("globals_editor_section_type"),
            type_children,
            density,
        );

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
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(tr!("globals_editor_persist_label")),
                    )
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(tr!("globals_editor_persist_desc")),
                    ),
            )
            .child(toggle(ed.persisted, palette).on_click(
                "globals-editor-persist",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_persist(cx)),
            ));
        let persist_section = section(
            palette,
            tr!("globals_editor_section_persistence"),
            persist_row,
            density,
        );

        let value_control = self.editor_value_control(palette, cx);
        let value_section = section(
            palette,
            tr!("globals_editor_section_value"),
            value_control,
            density,
        );

        let mut body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, density))
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
                    .gap(spacing(Spacing::Xs, density))
                    .p(spacing(Spacing::Xs, density))
                    .rounded(radius(Radius::Sm))
                    .bg(with_alpha(palette.danger, TINT_FILL_ALPHA))
                    .border(BORDER_THIN)
                    .border_color(with_alpha(palette.danger, TINT_BORDER_ALPHA))
                    .child(icon(Icon::AlertTriangle, FONT_XS, palette.danger))
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(err),
                    ),
            );
        }

        let saveable = self.is_saveable(cx) && !ed.saving;
        let save_label: SharedString = if ed.saving {
            tr!("globals_editor_saving").into()
        } else {
            tr!("globals_editor_save").into()
        };
        let cancel = secondary_button(tr!("globals_editor_cancel"), palette).on_click(
            "globals-editor-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
        );
        let save = primary_button(save_label, palette)
            .disabled(!saveable)
            .on_click(
                "globals-editor-save",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)),
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
            .on_close(
                "globals-editor-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .busy(self.saving)
            .on_dismiss("globals-editor-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel(cx));
            })
    }
}

impl GlobalEditor {
    fn editor_value_control(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let ed = self;
        match ed.kind {
            VariantKind::Bool => {
                let (label, hue) = if ed.bool_value {
                    ("true", palette.success)
                } else {
                    ("false", palette.danger)
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
                            .font_family(mono_family())
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
}

impl GlobalsView {
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
        let density = cx.density();
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
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .child("\u{00A0}"),
                );
            } else {
                code_line = code_line.child(
                    div()
                        .flex_none()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .whitespace_nowrap()
                        .child(highlighted_text(Language::Json, line.to_owned(), palette)),
                );
            }
            listing = listing.child(
                div()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .w(px(34.0))
                            .pr(scaled(px(12.0), density))
                            .flex()
                            .justify_end()
                            .font_family(mono_family())
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
            .py(spacing(Spacing::Sm, density))
            .px(scaled(px(12.0), density))
            .child(listing);
        let body = div()
            .id("globals-inspect-scroll")
            .w_full()
            .max_h(px(400.0))
            .overflow_y_scroll()
            .child(code);

        let hint = div()
            .font_family(body_family())
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
                    .gap(spacing(Spacing::Xs, density))
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

        let stats = self.render_stats(&palette, cx);
        let filter_left = self.render_filter_left(&palette, density, cx);
        let filter_right = self.render_filter_right(&palette, density, cx);
        let table = self.render_table(&palette, density, cx);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("globals_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("globals_breadcrumb_globals")),
            ],
            &palette,
        )
        .header_right(stats)
        .subheader_left(filter_left)
        .subheader_right(filter_right)
        .density(density)
        .body(table);

        let delete_overlay = self
            .pending_delete
            .get()
            .cloned()
            .map(|name| self.render_delete_confirm(name, &palette, cx));
        let editor_overlay = self.editor.clone();
        let inspect_overlay = self
            .inspecting
            .clone()
            .map(|g| self.render_inspect(&g, &palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(delete_overlay)
            .children(editor_overlay)
            .children(inspect_overlay)
            .children(self.render_row_context_menu(&palette, cx))
    }
}

fn section(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(
            div()
                .font_family(mono_family())
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

async fn export_globals_to_chosen_file(
    repo: Arc<dyn GlobalsRepo>,
) -> Result<Option<PathBuf>, DesktopError> {
    let entries = repo.export_all().await?;
    let envelope = GlobalsExport::new(entries);
    let json = serde_json::to_string_pretty(&envelope)?;
    let default_name = format!(
        "forge-globals-{}.json",
        time::OffsetDateTime::now_utc().unix_timestamp()
    );
    let filter = async_bridge::DialogFilter {
        name: "JSON".to_owned(),
        extensions: vec!["json"],
    };
    let Some(path) = async_bridge::save_file(Some(filter), Some(default_name)).await else {
        return Ok(None);
    };
    tokio::fs::write(&path, json).await?;
    Ok(Some(path))
}

fn global_from_entry(entry: &GlobalEntry) -> Global {
    Global {
        name: entry.name.clone().into(),
        value: entry.value.clone(),
        persisted: entry.persisted,
        reads: entry.reads,
        writes: entry.writes,
        modified_at: entry.last_modified,
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

fn json_placeholder(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Object => r#"{"key": "value"}"#,
        _ => "[1, 2, 3]",
    }
}

fn single_line_seed(value: &Variant) -> Option<String> {
    match value {
        Variant::Int(n) => Some(n.to_string()),
        Variant::Float(f) => Some(f.to_string()),
        Variant::String(s) => Some(s.clone()),
        Variant::Datetime(dt) => Some(forge_types::datetime_to_text(dt)),
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{EventSource, GlobalSet, ManualTestRun, TypedEvent};
    use forge_storage::globals::MockGlobalsRepo;
    use forge_types::ActionId;
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        StubEventLog, error_toasts, install_presentation, press_undo, pump, pump_until, runtime,
    };

    const SCORE: &str = "score";

    fn entry(name: &str) -> GlobalEntry {
        let now = OffsetDateTime::UNIX_EPOCH;
        GlobalEntry {
            name: name.to_owned(),
            value: Variant::Int(1),
            persisted: true,
            reads: 0,
            writes: 0,
            created_at: now,
            last_modified: now,
        }
    }

    struct Stored {
        rows: Arc<Mutex<Vec<GlobalEntry>>>,
        lists: Arc<AtomicUsize>,
    }

    fn stored_repo() -> (Stored, Arc<dyn GlobalsRepo>) {
        let stored = Stored {
            rows: Arc::new(Mutex::new(Vec::new())),
            lists: Arc::new(AtomicUsize::new(0)),
        };
        let (rows, lists) = (Arc::clone(&stored.rows), Arc::clone(&stored.lists));
        let mut repo = MockGlobalsRepo::new();
        repo.expect_list().returning(move || {
            lists.fetch_add(1, Ordering::SeqCst);
            Ok(rows.lock().unwrap().clone())
        });
        (stored, Arc::new(repo))
    }

    fn mount(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        repo: Arc<dyn GlobalsRepo>,
        bus: &Arc<EventBus>,
    ) -> (Entity<Globals>, Entity<GlobalsView>) {
        install_language(forge_storage::Language::En);
        install_presentation(cx);
        let globals = cx.new(|_| Globals::empty());
        let shared = globals.clone();
        let bus = Arc::clone(bus);
        let handle = rt.handle().clone();
        let view = cx.new(|cx| {
            let mut view = GlobalsView::new(shared, repo, handle, cx);
            view.follow_runtime_writes(bus, cx);
            view
        });
        pump_until(rt, || {
            cx.run_until_parked();
            view.read_with(cx, |view, _| !view.loading)
        });
        (globals, view)
    }

    fn publish(rt: &tokio::runtime::Runtime, bus: &EventBus, event: forge_events::Event) {
        rt.block_on(async { bus.publish(event) });
    }

    fn global_set() -> forge_events::Event {
        GlobalSet {
            key: SCORE.to_owned(),
            new_value: serde_json::json!(1),
            prev_value: serde_json::Value::Null,
            persisted: None,
            via: None,
        }
        .into_event(EventSource::Core, None)
    }

    fn shown(cx: &mut TestAppContext, globals: &Entity<Globals>, name: &str) -> bool {
        globals.read_with(cx, |globals, _| globals.contains(name))
    }

    #[gpui::test]
    fn a_runtime_global_write_refreshes_the_list_without_touching_the_screen(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (stored, repo) = stored_repo();
        let (globals, _view) = mount(cx, &rt, repo, &bus);

        stored.rows.lock().unwrap().push(entry(SCORE));
        publish(&rt, &bus, global_set());

        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });
    }

    #[gpui::test]
    fn an_event_about_anything_but_globals_does_not_reload_the_list(cx: &mut TestAppContext) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (stored, repo) = stored_repo();
        let (globals, _view) = mount(cx, &rt, repo, &bus);

        publish(
            &rt,
            &bus,
            ManualTestRun {
                action_id: ActionId::new(),
            }
            .into_event(EventSource::Core, None),
        );
        pump(&rt);
        cx.run_until_parked();
        stored.rows.lock().unwrap().push(entry(SCORE));
        publish(&rt, &bus, global_set());
        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });

        assert_eq!(stored.lists.load(Ordering::SeqCst), 2);
    }

    #[gpui::test]
    fn a_slow_reload_never_overwrites_a_newer_one_that_landed_first(cx: &mut TestAppContext) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (_stored, repo) = stored_repo();
        let (globals, view) = mount(cx, &rt, repo, &bus);
        let (older_tx, older_rx) = tokio::sync::oneshot::channel::<()>();
        let (newer_tx, newer_rx) = tokio::sync::oneshot::channel::<()>();
        let older_landed = Arc::new(AtomicUsize::new(0));
        let landed = Arc::clone(&older_landed);

        view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    older_rx.await.map_err(RepoFailure::operation)?;
                    landed.store(1, Ordering::SeqCst);
                    Ok(vec![entry("older")])
                },
                cx,
            );
            view.spawn_reload(
                async move {
                    newer_rx.await.map_err(RepoFailure::operation)?;
                    Ok(vec![entry("newer")])
                },
                cx,
            );
        });
        newer_tx.send(()).unwrap();
        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, "newer")
        });
        older_tx.send(()).unwrap();
        pump_until(&rt, || older_landed.load(Ordering::SeqCst) == 1);
        cx.run_until_parked();

        assert_eq!(
            (shown(cx, &globals, "newer"), shown(cx, &globals, "older")),
            (true, false)
        );
    }

    fn errors(cx: &mut TestAppContext) -> usize {
        cx.update(|cx| error_toasts(cx))
    }

    fn with_toasts(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
    }

    #[gpui::test]
    fn a_failed_write_toasts_even_with_a_newer_reload_in_flight(cx: &mut TestAppContext) {
        with_toasts(cx);
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (_stored, repo) = stored_repo();
        let (_globals, view) = mount(cx, &rt, repo, &bus);
        let (_held, reload_gate) = tokio::sync::oneshot::channel::<()>();

        view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    reload_gate.await.map_err(RepoFailure::operation)?;
                    Ok(Vec::new())
                },
                cx,
            );
            view.spawn_write(
                async { Err::<(), _>(RepoFailure::operation("disk gone")) },
                cx,
            );
        });

        pump_until(&rt, || {
            cx.run_until_parked();
            errors(cx) == 1
        });
    }

    #[gpui::test]
    fn a_successful_write_relists_and_shows_what_it_wrote(cx: &mut TestAppContext) {
        with_toasts(cx);
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (stored, repo) = stored_repo();
        let (globals, view) = mount(cx, &rt, repo, &bus);
        let rows = Arc::clone(&stored.rows);

        view.update(cx, |view, cx| {
            view.spawn_write(
                async move {
                    rows.lock().unwrap().push(entry(SCORE));
                    Ok(())
                },
                cx,
            );
        });

        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });
    }

    #[gpui::test]
    fn a_stale_reload_that_failed_is_dropped_without_a_toast(cx: &mut TestAppContext) {
        with_toasts(cx);
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (stored, repo) = stored_repo();
        let (globals, view) = mount(cx, &rt, repo, &bus);
        let (release_stale, stale_gate) = tokio::sync::oneshot::channel::<()>();
        let stale_landed = Arc::new(AtomicUsize::new(0));
        let landed = Arc::clone(&stale_landed);

        view.update(cx, |view, cx| {
            view.spawn_reload(
                async move {
                    stale_gate.await.map_err(RepoFailure::operation)?;
                    landed.store(1, Ordering::SeqCst);
                    Err(RepoFailure::operation("disk gone"))
                },
                cx,
            );
            view.reload(cx);
        });
        stored.rows.lock().unwrap().push(entry(SCORE));
        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });
        release_stale.send(()).unwrap();
        pump_until(&rt, || stale_landed.load(Ordering::SeqCst) == 1);
        cx.run_until_parked();

        assert_eq!(errors(cx), 0);
    }

    fn archiving_repo() -> (Arc<Mutex<Vec<GlobalEntry>>>, Arc<dyn GlobalsRepo>) {
        let rows = Arc::new(Mutex::new(vec![entry(SCORE)]));
        let shelf = Arc::new(Mutex::new(Vec::new()));
        let mut repo = MockGlobalsRepo::new();
        let listed = Arc::clone(&rows);
        repo.expect_list()
            .returning(move || Ok(listed.lock().unwrap().clone()));
        let (live, archived) = (Arc::clone(&rows), Arc::clone(&shelf));
        repo.expect_archive().returning(move |_| {
            archived.lock().unwrap().append(&mut live.lock().unwrap());
            Ok(true)
        });
        let live = Arc::clone(&rows);
        repo.expect_restore().returning(move |_| {
            live.lock().unwrap().append(&mut shelf.lock().unwrap());
            Ok(true)
        });
        (rows, Arc::new(repo))
    }

    #[gpui::test]
    fn undoing_a_delete_brings_the_global_back(cx: &mut TestAppContext) {
        with_toasts(cx);
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (_rows, repo) = archiving_repo();
        let (globals, view) = mount(cx, &rt, repo, &bus);
        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });

        view.update(cx, |view, cx| {
            view.request_delete(SCORE.into(), cx);
            view.confirm_delete(cx);
        });
        pump_until(&rt, || {
            cx.run_until_parked();
            !shown(cx, &globals, SCORE)
        });
        press_undo(cx);

        pump_until(&rt, || {
            cx.run_until_parked();
            shown(cx, &globals, SCORE)
        });
    }

    #[gpui::test]
    fn the_empty_caption_invites_a_first_variable_only_when_there_are_none_at_all(
        cx: &mut TestAppContext,
    ) {
        for (stored, filter, expected) in [
            (false, GlobalsFilter::All, "globals_empty_none"),
            (false, GlobalsFilter::Persisted, "globals_empty_caption"),
            (false, GlobalsFilter::Session, "globals_empty_caption"),
            (true, GlobalsFilter::All, "globals_empty_caption"),
        ] {
            let rt = runtime();
            let bus = EventBus::new(Arc::new(StubEventLog));
            let (rows, repo) = stored_repo();
            if stored {
                rows.rows.lock().unwrap().push(entry(SCORE));
            }
            let (_globals, view) = mount(cx, &rt, repo, &bus);

            view.update(cx, |view, cx| view.set_filter(filter, cx));

            assert_eq!(
                view.read_with(cx, |view, cx| view.empty_caption_key(cx)),
                expected,
                "stored={stored} filter={filter:?}"
            );
        }
    }
}
