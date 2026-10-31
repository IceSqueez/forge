mod editor;
mod filter_bar;
mod inspect;
mod table;

use crate::async_bridge::{self, KeyedSnapshotWriters, Loadable, WriteOutcome};
use crate::error::DesktopError;
use crate::globals::{Global, Globals, GlobalsFilter};
use crate::navigable_list::{ListCursor, ListStep, NavigableList};
use crate::repo_failure::RepoFailure;
use editor::{EditorMode, GlobalDraft, GlobalEditor, GlobalEditorEvent};
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Confirm, FONT_XS, InlineEdit, InlineEditEvent, InputEvent,
    PushToast, SearchState, TextInput, ToastKind, drive_overlay_focus, focus_inside_modal,
    inline_edit, page_frame, tr,
};
use forge_runtime::EventBus;
use forge_storage::{GlobalEntry, GlobalTransit, GlobalsExport, GlobalsRepo};
use forge_types::VariantKind;
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, ScrollStrategy, SharedString, Subscription, Task,
    UniformListScrollHandle, Window, div, px,
};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

const ROW_DOT: gpui::Pixels = px(6.0);
const VALUE_ICON: gpui::Pixels = px(11.0);
const ACTION_HOVER_ALPHA: f32 = 0.14;

struct RenameState {
    original: SharedString,
    editor: Entity<InlineEdit>,
    _sub: Subscription,
}

pub(crate) struct GlobalsView {
    globals: Entity<Globals>,
    backend: Arc<dyn GlobalsRepo>,
    rt_handle: tokio::runtime::Handle,
    list_load: Loadable<()>,
    filter: GlobalsFilter,
    search: SearchState,
    visible: Vec<Global>,
    editor: Option<Entity<GlobalEditor>>,
    _editor_sub: Option<Subscription>,
    pending_delete: Confirm<SharedString>,
    inspecting: Option<inspect::InspectedGlobal>,
    renaming: Option<RenameState>,
    row_menu: Option<RowMenu>,
    menu_focus: FocusHandle,
    menu_focus_restore: Option<FocusHandle>,
    table_keys: ListCursor<SharedString>,
    table_scroll: UniformListScrollHandle,
    _globals_obs: Subscription,
    _search_sub: Subscription,
    reload_generation: async_bridge::Generation,
    persist_writers: KeyedSnapshotWriters<String>,
    persist_pending: HashMap<String, bool>,
    _runtime_writes: Option<Task<()>>,
}

struct RowMenu {
    name: SharedString,
    position: gpui::Point<gpui::Pixels>,
}

const GLOBAL_EVENT_PREFIX: &str = "global.";

impl GlobalsView {
    pub(crate) fn new(
        globals: Entity<Globals>,
        backend: Arc<dyn GlobalsRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("globals_search_placeholder"));

        let persist_writers = KeyedSnapshotWriters::new(&rt_handle);
        let globals_obs = cx.observe(&globals, Self::on_globals_changed);
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);

        let view = Self {
            globals,
            backend,
            rt_handle,
            list_load: Loadable::Loading,
            filter: GlobalsFilter::default(),
            search,
            visible: Vec::new(),
            editor: None,
            _editor_sub: None,
            pending_delete: Confirm::default(),
            inspecting: None,
            renaming: None,
            row_menu: None,
            menu_focus: cx.focus_handle(),
            menu_focus_restore: None,
            table_keys: ListCursor::new(cx),
            table_scroll: UniformListScrollHandle::new(),
            _globals_obs: globals_obs,
            _search_sub: search_sub,
            reload_generation: async_bridge::Generation::default(),
            persist_writers,
            persist_pending: HashMap::new(),
            _runtime_writes: None,
        };
        view.reload(cx);
        view
    }

    pub(crate) fn follow_runtime_writes(&mut self, bus: Arc<EventBus>, cx: &mut Context<Self>) {
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
                Err(failure) => this.on_list_load_failed(failure, cx),
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
        self.list_load = Loadable::Loaded(());
        cx.notify();
    }

    fn on_list_load_failed(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        if self.globals.read(cx).total() > 0 {
            self.on_repo_error(&failure, cx);
        }
        self.list_load = Loadable::Failed(failure);
        cx.notify();
    }

    fn retry_list_load(&mut self, cx: &mut Context<Self>) {
        self.list_load = Loadable::Loading;
        self.reload(cx);
        cx.notify();
    }

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::error!(error = %failure, "globals operation failed");
        let message = failure.text();
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
        let key = name.to_string();
        let current = self.persist_pending.get(&key).copied().unwrap_or_else(|| {
            self.globals
                .read(cx)
                .entries()
                .iter()
                .any(|g| g.name == name && g.persisted)
        });
        let next = !current;
        self.persist_pending.insert(key.clone(), next);
        let backend = Arc::clone(&self.backend);
        let settled_key = key.clone();
        self.persist_writers.writer(key.clone()).write(
            async move {
                backend
                    .set_persisted(&key, next)
                    .await
                    .map(drop)
                    .map_err(RepoFailure::from)
            },
            move |this: &mut Self, outcome, cx| {
                this.on_persist_write_settled(&settled_key, outcome, cx)
            },
            cx,
        );
    }

    fn on_persist_write_settled(
        &mut self,
        key: &str,
        outcome: WriteOutcome,
        cx: &mut Context<Self>,
    ) {
        let written = match outcome {
            WriteOutcome::Superseded => return,
            WriteOutcome::Written => Ok(()),
            WriteOutcome::Failed(failure) => Err(failure),
        };
        self.persist_pending.remove(key);
        self.on_write_settled(written, cx);
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
        cx.push_undo_toast(message, move |_window, app: &mut App| {
            let backend = Arc::clone(&backend);
            let key = name.to_string();
            view.update(app, |this, cx| {
                this.spawn_write(
                    async move { backend.restore(&key).await.map_err(RepoFailure::from) },
                    cx,
                );
            });
        });
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
        focus_inside_modal(window, cx, |window, cx| {
            editor.update(cx, |e, cx| e.focus(window, cx))
        });
        self._editor_sub = Some(cx.subscribe(&editor, Self::on_editor_event));
        self.editor = Some(editor);
        cx.notify();
    }

    fn open_edit(&mut self, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.table_keys.move_to(name.clone());
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
        focus_inside_modal(window, cx, |window, cx| {
            editor.update(cx, |e, cx| e.focus(window, cx))
        });
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
}

impl NavigableList for GlobalsView {
    fn step_list(&mut self, step: ListStep, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        let names = self.visible_names();
        if let Some(next) = self.table_keys.step(&names, None, step) {
            self.table_scroll
                .scroll_to_item(next, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn activate_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return;
        }
        if let Some(name) = self.table_keys.activation_target(&self.visible_names()) {
            self.open_edit(name, window, cx);
        }
    }
}

impl Render for GlobalsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.row_menu.is_some(),
            &self.menu_focus,
            &mut self.menu_focus_restore,
            window,
            cx,
        );
        self.table_keys.claim_focus_once(window, cx);
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
            .as_ref()
            .map(|inspected| self.render_inspect(inspected, &palette, cx));

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

async fn export_globals_to_chosen_file(
    repo: Arc<dyn GlobalsRepo>,
) -> Result<Option<PathBuf>, DesktopError> {
    let json = globals_export_json(repo.export_all().await?)?;
    let default_name = default_export_name(time::OffsetDateTime::now_utc());
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

fn globals_export_json(entries: Vec<GlobalTransit>) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&GlobalsExport::new(entries))
}

fn default_export_name(now: time::OffsetDateTime) -> String {
    format!("forge-globals-{}.json", now.unix_timestamp())
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

#[cfg(test)]
mod tests {
    use super::editor::{json_seed, parse_json_variant, single_line_seed};
    use super::inspect::inspect_count;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{EventSource, GlobalSet, ManualTestRun, TypedEvent};
    use forge_storage::MockGlobalsRepo;
    use forge_types::{ActionId, Variant};
    use gpui::TestAppContext;
    use time::OffsetDateTime;

    use super::*;
    use crate::i18n::install_language;
    use crate::test_support::{
        StubEventLog, error_toasts, install_presentation, press_undo, pump, pump_until, runtime,
    };

    #[test]
    fn the_export_envelope_carries_its_format_version_and_every_global() {
        let json = globals_export_json(vec![GlobalTransit {
            name: "score".to_owned(),
            value: Variant::Int(3),
            persisted: true,
            last_modified: OffsetDateTime::UNIX_EPOCH,
            reads: 1,
            writes: 2,
        }])
        .unwrap();

        let parsed: GlobalsExport = serde_json::from_str(&json).unwrap();
        assert_eq!(
            (
                parsed.format_version,
                parsed
                    .globals
                    .iter()
                    .map(|global| (global.name.as_str(), &global.value))
                    .collect::<Vec<_>>()
            ),
            (
                GlobalsExport::new(Vec::new()).format_version,
                vec![("score", &Variant::Int(3))]
            )
        );
    }

    #[test]
    fn the_default_export_name_is_stamped_with_the_unix_second() {
        assert_eq!(
            default_export_name(OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()),
            "forge-globals-1700000000.json"
        );
    }

    const SCORE: &str = "score";
    const LISTED: [&str; 3] = ["alpha", "beta", "gamma"];

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

    fn persist_recording_repo(
        rows: Vec<GlobalEntry>,
    ) -> (Arc<Mutex<Vec<bool>>>, Arc<dyn GlobalsRepo>) {
        let rows = Arc::new(Mutex::new(rows));
        let writes = Arc::new(Mutex::new(Vec::new()));
        let listed = Arc::clone(&rows);
        let mut repo = MockGlobalsRepo::new();
        repo.expect_list()
            .returning(move || Ok(listed.lock().unwrap().clone()));
        let recorded = Arc::clone(&writes);
        repo.expect_set_persisted()
            .returning(move |name, persisted| {
                recorded.lock().unwrap().push(persisted);
                for row in rows
                    .lock()
                    .unwrap()
                    .iter_mut()
                    .filter(|row| row.name == name)
                {
                    row.persisted = persisted;
                }
                Ok(true)
            });
        (writes, Arc::new(repo))
    }

    fn settle_persist(cx: &mut TestAppContext, rt: &tokio::runtime::Runtime) {
        for _ in 0..3 {
            cx.run_until_parked();
            pump(rt);
        }
        cx.run_until_parked();
    }

    fn toggle(view: &Entity<GlobalsView>, cx: &mut TestAppContext) {
        view.update(cx, |view, cx| view.toggle_persist(SCORE.into(), cx));
    }

    #[gpui::test]
    fn toggling_persistence_with_the_store_answering_between_writes_in_click_order(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (writes, repo) = persist_recording_repo(vec![entry(SCORE)]);
        let (_globals, view) = mount(cx, &rt, repo, &bus);

        for _ in 0..2 {
            toggle(&view, cx);
            settle_persist(cx, &rt);
        }

        assert_eq!(writes.lock().unwrap().clone(), vec![false, true]);
    }

    #[gpui::test]
    fn toggling_persistence_twice_before_the_store_answers_returns_to_the_first_state(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let bus = EventBus::new(Arc::new(StubEventLog));
        let (writes, repo) = persist_recording_repo(vec![entry(SCORE)]);
        let (_globals, view) = mount(cx, &rt, repo, &bus);

        toggle(&view, cx);
        toggle(&view, cx);
        settle_persist(cx, &rt);

        assert_eq!(writes.lock().unwrap().last().copied(), Some(true));
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
            view.read_with(cx, |view, _| !matches!(view.list_load, Loadable::Loading))
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

    fn refused() -> SharedString {
        "refused".into()
    }

    #[test]
    fn json_text_of_the_wrong_shape_or_syntax_is_refused_with_the_given_reason() {
        for (text, want_array) in [
            ("{\"a\": 1}", true),
            ("[1]", false),
            ("3", true),
            ("null", false),
            ("not json", true),
            ("", false),
        ] {
            assert_eq!(
                parse_json_variant(text, want_array, refused()),
                Err(refused()),
                "{text:?}"
            );
        }
    }

    #[test]
    fn json_text_of_the_right_shape_becomes_the_matching_variant() {
        assert_eq!(
            parse_json_variant("[1, \"two\"]", true, refused()),
            Ok(Variant::Array(vec![
                Variant::Int(1),
                Variant::String("two".to_owned())
            ]))
        );
        assert_eq!(
            parse_json_variant("{\"k\": true}", false, refused()),
            Ok(Variant::Object(std::collections::BTreeMap::from([(
                "k".to_owned(),
                Variant::Bool(true)
            )])))
        );
    }

    #[test]
    fn a_stored_array_or_object_reloads_from_its_json_seed_unchanged() {
        let nested = Variant::Object(std::collections::BTreeMap::from([(
            "items".to_owned(),
            Variant::Array(vec![Variant::Int(1), Variant::Float(2.5)]),
        )]));
        let list = Variant::Array(vec![nested.clone()]);
        for (value, want_array) in [(list, true), (nested, false)] {
            let seed = json_seed(&value).expect("a json seed");
            assert_eq!(parse_json_variant(&seed, want_array, refused()), Ok(value));
        }
    }

    #[test]
    fn a_float_seed_parses_back_to_the_exact_stored_value() {
        for stored in [1e300, -0.1, 1.0, f64::MIN_POSITIVE] {
            let seed = single_line_seed(&Variant::Float(stored)).expect("a seed");
            assert_eq!(seed.parse::<f64>(), Ok(stored), "{seed}");
        }
    }

    #[test]
    fn a_datetime_seed_parses_back_to_the_exact_stored_instant() {
        let stored = time::macros::datetime!(2026-05-18 14:23:00.25 UTC);
        let seed = single_line_seed(&Variant::Datetime(stored)).expect("a seed");

        assert_eq!(forge_types::datetime_from_text(&seed), Some(stored));
    }

    #[test]
    fn only_container_variants_seed_the_json_editor_and_only_scalars_the_line_editor() {
        for scalar in [Variant::Int(1), Variant::String("s".to_owned())] {
            assert_eq!(json_seed(&scalar), None);
        }
        assert_eq!(single_line_seed(&Variant::Bool(true)), None);
        assert_eq!(single_line_seed(&Variant::Array(vec![])), None);
    }

    #[test]
    fn the_inspector_counts_items_of_a_list_and_keys_of_an_object() {
        let object = Variant::Object(std::collections::BTreeMap::from([(
            "k".to_owned(),
            Variant::Int(1),
        )]));

        assert_eq!(
            inspect_count(&Variant::Array(vec![Variant::Int(1), Variant::Int(2)])),
            ("globals_inspect_subtitle_items", 2)
        );
        assert_eq!(inspect_count(&object), ("globals_inspect_subtitle_keys", 1));
    }

    struct Hosting(Entity<GlobalsView>);

    impl Render for Hosting {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    struct ListRig {
        view: Entity<GlobalsView>,
        vcx: gpui::VisualTestContext,
        _rt: tokio::runtime::Runtime,
    }

    impl ListRig {
        fn mount(cx: &mut TestAppContext) -> Self {
            let rt = runtime();
            let bus = EventBus::new(Arc::new(StubEventLog));
            let (stored, repo) = stored_repo();
            stored.rows.lock().unwrap().extend(LISTED.map(entry));
            let (_globals, view) = mount(cx, &rt, repo, &bus);
            cx.update(crate::actions::register_key_bindings);
            let hosted = view.clone();
            let window = cx.add_window(|_, _| Hosting(hosted));
            let vcx = gpui::VisualTestContext::from_window(window.into(), cx);
            vcx.run_until_parked();
            Self { view, vcx, _rt: rt }
        }

        fn press(&mut self, keys: &str) {
            self.vcx.simulate_keystrokes(keys);
            self.vcx.run_until_parked();
        }

        fn search(&mut self, query: &str) {
            let changed = InputEvent::Changed(query.to_owned().into());
            self.view.update(&mut self.vcx, |view, cx| {
                let field = view.search.field().clone();
                view.on_search_event(field, &changed, cx);
            });
            self.vcx.run_until_parked();
        }

        fn open_and_close(&mut self, opening: Opening, name: &str) {
            self.view
                .update_in(&mut self.vcx, |view, window, cx| match opening {
                    Opening::Edit => view.open_edit(name.to_owned().into(), window, cx),
                    Opening::Inspect => {
                        let at = view.visible.iter().find(|g| g.name == name);
                        view.open_inspect(at.unwrap().clone(), cx);
                    }
                });
            self.vcx.run_until_parked();
            self.view.update(&mut self.vcx, |view, cx| match opening {
                Opening::Edit => view.cancel_editor(cx),
                Opening::Inspect => view.close_inspect(cx),
            });
            self.vcx.run_until_parked();
            self.vcx.update(|_, _| {});
            self.vcx.run_until_parked();
        }

        fn cursor(&self) -> Option<&'static str> {
            self.view.read_with(&self.vcx, |view, _| {
                LISTED
                    .into_iter()
                    .find(|name| view.table_keys.is_at(&SharedString::from(*name)))
            })
        }

        fn edited(&self) -> Option<String> {
            self.view.read_with(&self.vcx, |view, cx| {
                let editor = view.editor.as_ref()?.read(cx);
                match &editor.mode {
                    EditorMode::Edit(name) => Some(name.to_string()),
                    EditorMode::Create => None,
                }
            })
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Opening {
        Edit,
        Inspect,
    }

    #[gpui::test]
    fn arrows_continue_from_the_row_its_own_button_opened(cx: &mut TestAppContext) {
        for opening in [Opening::Edit, Opening::Inspect] {
            let mut rig = ListRig::mount(cx);
            rig.press("down");

            rig.open_and_close(opening, LISTED[1]);
            rig.press("down");

            assert_eq!(rig.cursor(), Some(LISTED[2]), "{opening:?}");
        }
    }

    #[gpui::test]
    fn enter_after_a_row_button_reopens_that_row(cx: &mut TestAppContext) {
        for opening in [Opening::Edit, Opening::Inspect] {
            let mut rig = ListRig::mount(cx);
            rig.press("down");

            rig.open_and_close(opening, LISTED[2]);
            rig.press("enter");

            assert_eq!(rig.edited().as_deref(), Some(LISTED[2]), "{opening:?}");
        }
    }

    #[gpui::test]
    fn an_open_row_menu_keeps_arrows_and_enter_from_the_list(cx: &mut TestAppContext) {
        let mut rig = ListRig::mount(cx);
        rig.press("down");
        rig.view.update(&mut rig.vcx, |view, cx| {
            view.open_row_menu(LISTED[1].into(), gpui::Point::default(), cx)
        });
        rig.vcx.run_until_parked();

        rig.press("down enter");

        assert_eq!(rig.cursor(), Some(LISTED[0]));
        assert_eq!(rig.edited(), None);
    }

    #[gpui::test]
    fn enter_opens_the_editor_on_the_row_the_arrows_reached(cx: &mut TestAppContext) {
        let mut rig = ListRig::mount(cx);

        rig.press("down down down up enter");

        assert_eq!(rig.edited().as_deref(), Some(LISTED[1]));
    }

    #[gpui::test]
    fn a_search_that_hides_the_cursor_row_takes_the_cursor_with_it(cx: &mut TestAppContext) {
        let mut rig = ListRig::mount(cx);
        rig.press("down");

        rig.search(LISTED[2]);
        rig.press("enter");
        assert_eq!(rig.edited(), None);

        rig.press("down");
        assert_eq!(rig.cursor(), Some(LISTED[2]));
    }

    #[gpui::test]
    fn an_inline_rename_silences_the_list_keys(cx: &mut TestAppContext) {
        let mut rig = ListRig::mount(cx);
        rig.press("down");
        rig.view.update_in(&mut rig.vcx, |view, window, cx| {
            view.start_rename(LISTED[2].into(), window, cx);
            view.step_list(ListStep::Next, cx);
            view.activate_list(window, cx);
        });
        rig.vcx.run_until_parked();

        assert_eq!(rig.cursor(), Some(LISTED[0]));
        assert_eq!(rig.edited(), None);
    }
}
