mod commands;
mod prompts;
mod rename;
mod render;

pub(super) use self::rename::PendingRename;
use self::rename::RenameState;
pub(super) use self::render::RowMenu;
use super::ScriptDeps;
use super::console::{LogTag, ScriptConsole, push_line};
use crate::app::async_bridge::{self, Loadable};
use crate::app::navigable_list::ListCursor;
use crate::shared::repo_failure::RepoFailure;
use forge_components::{
    ActivePresentation, Confirm, FONT_XS, Icon, Radius, SearchState, TextInput, tr,
};
use forge_script::validate_syntax;
use forge_storage::{
    SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND, ScriptRecord, ScriptReferences, ScriptRepo,
};
use forge_types::{Action, ActionId, ScriptId};
use gpui::prelude::*;
use gpui::{Context, Entity, EventEmitter, Pixels, ScrollHandle, Subscription, Window, px};
use std::sync::Arc;

const LEFT_PANE_W: Pixels = px(200.0);

#[derive(Clone)]
pub(super) struct LinkedAction {
    pub(super) id: ActionId,
    pub(super) name: String,
}

pub(super) struct ScriptEntry {
    id: ScriptId,
    pub(super) name: String,
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

pub(super) enum ScriptListEvent {
    RowChosen(ScriptId),
    NewRequested,
    Loaded { first: Option<ScriptId> },
    Created(ScriptRecord),
    SelectionLost,
    Renamed { id: ScriptId, name: String },
    RowMenuOpened,
}

pub(super) struct ScriptList {
    deps: ScriptDeps,
    console: Entity<ScriptConsole>,

    pub(super) scripts: Vec<ScriptEntry>,
    pub(super) selected: Option<ScriptId>,
    pub(super) list_load: Loadable<()>,

    list_width: Pixels,
    pub(super) list_keys: ListCursor<ScriptId>,
    pub(super) list_scroll: ScrollHandle,

    pub(super) search: SearchState,
    _search_sub: Subscription,

    rename: Option<RenameState>,
    pub(super) row_menu: Option<RowMenu>,
    pub(super) pending_delete: Confirm<ScriptId>,
    pub(super) delete_referrers: Option<ScriptReferences>,
    pub(super) pending_rename: Option<PendingRename>,
}

impl EventEmitter<ScriptListEvent> for ScriptList {}

impl ScriptList {
    pub(super) fn new(
        deps: ScriptDeps,
        console: Entity<ScriptConsole>,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = SearchState::from_field(cx.new(|cx| {
            TextInput::new(tr!("script_editor_search_placeholder"), cx)
                .with_font_size(FONT_XS)
                .leading_icon(Icon::Search, |palette| palette.text_faint)
                .on_surface()
                .static_chrome(|palette| palette.surface_overlay, Radius::Sm)
        }));
        let search_sub = cx.subscribe(search.field(), |this: &mut Self, _f, event, cx| {
            if this.search.on_changed(event) {
                cx.notify();
            }
        });

        let mut list = Self {
            deps,
            console,
            scripts: Vec::new(),
            selected: None,
            list_load: Loadable::Loading,
            list_width: LEFT_PANE_W,
            list_keys: ListCursor::new(cx),
            list_scroll: ScrollHandle::new(),
            search,
            _search_sub: search_sub,
            rename: None,
            row_menu: None,
            pending_delete: Confirm::default(),
            delete_referrers: None,
            pending_rename: None,
        };
        list.load_scripts(cx);
        list
    }

    pub(super) fn prompt_open(&self) -> bool {
        self.pending_delete.get().is_some() || self.pending_rename.is_some()
    }

    pub(super) fn row_menu_open(&self) -> bool {
        self.row_menu.is_some()
    }

    pub(super) fn health(&self) -> (usize, usize) {
        let ok = self.scripts.iter().filter(|e| e.status_ok).count();
        (ok, self.scripts.len())
    }

    pub(super) fn linked_action(&self, id: ScriptId) -> Option<LinkedAction> {
        self.find_entry(id).and_then(|e| e.linked.clone())
    }

    fn find_entry(&self, id: ScriptId) -> Option<&ScriptEntry> {
        self.scripts.iter().find(|e| e.id == id)
    }

    pub(super) fn mark_selected(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.list_keys.move_to(id);
        cx.notify();
    }

    pub(super) fn mark_valid(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        if let Some(entry) = self.scripts.iter_mut().find(|e| e.id == id) {
            entry.status_ok = true;
            cx.notify();
        }
    }

    pub(super) fn load_scripts(&mut self, cx: &mut Context<Self>) {
        self.list_load = Loadable::Loading;
        let repo = Arc::clone(&self.deps.backend) as Arc<dyn ScriptRepo>;
        let action_repo = self.deps.backend.action_repo();
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                let records = repo.list().await.map_err(RepoFailure::from)?;
                let actions = action_repo.list().await.map_err(RepoFailure::from)?;
                Ok(records
                    .into_iter()
                    .map(|r| ScriptEntry {
                        status_ok: validate_syntax(&r.body).is_ok(),
                        enabled: r.enabled,
                        linked: find_linked_action(&actions, &r.name),
                        id: r.id,
                        name: r.name,
                    })
                    .collect::<Vec<_>>())
            },
            |this, result, cx| this.apply_scripts_loaded(result, cx),
            cx,
        );
        cx.notify();
    }

    fn apply_scripts_loaded(
        &mut self,
        result: Result<Vec<ScriptEntry>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(entries) => {
                self.list_load = Loadable::Loaded(());
                let first = entries.first().map(|e| e.id);
                self.scripts = entries;
                cx.emit(ScriptListEvent::Loaded { first });
            }
            Err(e) => {
                tracing::warn!(error = %e, "script list load failed");
                push_line(
                    &self.console,
                    LogTag::Err,
                    tr!("script_console_load_failed", error = e.text()),
                    cx,
                );
                self.list_load = Loadable::Failed(e);
            }
        }
        cx.notify();
    }

    fn choose_row(&mut self, id: ScriptId, cx: &mut Context<Self>) {
        if self.selected == Some(id) {
            self.list_keys.move_to(id);
            cx.notify();
            return;
        }
        cx.emit(ScriptListEvent::RowChosen(id));
    }
}

impl Render for ScriptList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.list_keys.claim_focus_once(window, cx);
        let palette = cx.palette();
        let density = cx.density();
        self.left_pane(&palette, density, cx)
    }
}
