use crate::async_bridge::{self, Loadable};
use crate::integration_switch::SwitchWatch;
use crate::navigation::NavRequested;
use crate::navigation::Screen;
use crate::picker_favorites::{SUB_ACTION_FAVORITES, StoredFavorites, TRIGGER_FAVORITES};
use crate::repo_failure::RepoFailure;
use crate::run_history_modal::RunHistoryModal;
use forge_components::PushToast;
use forge_components::Radius;
use forge_components::drive_overlay_focus;
use forge_components::radius;
use forge_components::{
    ActivePresentation, BreadcrumbCrumb, Density, ForgePalette, GridPicker, Icon, InlineEdit,
    KeyActivated, OverlayPosition, SearchState, TextArea, TextInput, ToastKind,
    fmt_display_relative, fmt_number, icon, overlay, page_frame, primary_button_with_icon, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerCategory, TriggerRegistry};
use forge_runtime::ActionDetail;
use forge_runtime::{EventBus, QueueSchedulerHandle};
use forge_storage::{ActionRepo, ActionTelemetry, SettingsRepo, StorageError, TriggerInstanceRepo};
use forge_types::{Action, ActionId, ExecutionOutcome, QueueId, TriggerInstanceId};
use gpui::FocusHandle;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, Pixels, Point,
    SharedString, Subscription, Window, div, prelude::*, px,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

mod action_modal;
mod analyzer;
pub(crate) mod config_problem_text;
mod editor;
mod files_root_note;
mod nav;
pub(crate) mod overlay_schema;
mod pack_export_host;
mod run_history;
mod shortcuts;
mod state;
mod step_validation;
mod sub_action_modal;
mod test_run;
mod test_trigger;
mod tree;

use action_modal::{ActionFormEvent, ActionFormModal};
pub(crate) use editor::parse_variable_segments;
use editor::{ActionEditor, ActionEditorEvent, ActionSettings, ActionWrite};
use pack_export_host::PackExportHost;
pub(crate) use state::ActionsDeps;
use state::{ScreenDeps, ScreenModals};
use tree::{ActionTree, ActionTreeDeps, ActionTreeEvent};

const EMPTY_GLYPH: Pixels = px(28.0);

const PILL_RADIUS: Pixels = px(8.0);
const PILL_DOT: Pixels = px(5.0);
const CARD_GLYPH: Pixels = px(13.0);
const TRIGGER_DOT: Pixels = px(7.0);
const STEP_HEALTH_TILE_ALPHA: f32 = 0.14;
const TRIGGER_GLYPH: Pixels = px(13.0);
const UNLINK_GLYPH: Pixels = px(13.0);
const STEP_CIRCLE: Pixels = px(22.0);
const STEP_CIRCLE_RADIUS: Pixels = px(11.0);
const STEP_COL_W: Pixels = px(24.0);
const STEP_CONNECTOR_W: Pixels = px(2.0);
const STEP_CONNECTOR_H: Pixels = px(14.0);
const STEP_BTN: Pixels = px(22.0);
const STEP_BTN_GLYPH: Pixels = px(12.0);
const STEP_BTN_RADIUS: Pixels = px(4.0);
const CARD_PAD_V: Pixels = px(9.0);
const CARD_PAD_H: Pixels = px(12.0);
const EMPTY_CARD_PAD_V: Pixels = px(18.0);
const EMPTY_CARD_PAD_H: Pixels = px(12.0);
const EMPTY_CARD_GLYPH: Pixels = px(16.0);
const STAT_GAP: Pixels = px(8.0);
const STAT_PAD_V: Pixels = px(10.0);
const STAT_VALUE_GAP: Pixels = px(3.0);
const PANE_PAD_V: Pixels = px(18.0);
const PANE_PAD_H: Pixels = px(22.0);
const STEP_GAP: Pixels = px(6.0);
const STEP_NOTICE_MT: Pixels = px(8.0);
const TRIGGER_DIMMED_OPACITY: f32 = 0.7;
const STEP_CARD_PAD_V: Pixels = px(10.0);
const STEP_CARD_PAD_H: Pixels = px(12.0);
const HEADER_ACTION_H: Pixels = px(28.0);
const SUB_MODAL_MAX_H: Pixels = px(440.0);
const SUB_AREA_FIELD_H: Pixels = px(150.0);
const STEP_MODAL_W: Pixels = px(560.0);
const STEP_TILE: Pixels = px(32.0);
const STEP_TILE_GLYPH: Pixels = px(16.0);
const GRID_COL_GAP: Pixels = px(12.0);

const CHIP_RADIUS: Pixels = radius(Radius::Sm);
const BRANCH_GLYPH: Pixels = px(11.0);
const CASE_MATCH_W: Pixels = px(160.0);

fn on_repo_error<V: 'static>(failure: &RepoFailure, cx: &mut Context<V>) {
    tracing::error!(error = %failure, "actions operation failed");
    let message = failure.text();
    cx.push_toast(
        ToastKind::Error,
        tr!("actions_toast_error", message = message.as_str()),
    );
    cx.notify();
}

struct ActionFormHost {
    view: Entity<ActionFormModal>,
    _sub: Subscription,
}

struct HistoryModalHost {
    view: Entity<RunHistoryModal>,
    _subs: [Subscription; 2],
}

pub(crate) struct ScreenActionsView {
    deps: ScreenDeps,
    tree: Entity<ActionTree>,
    editor: Entity<ActionEditor>,
    modals: ScreenModals,
    _children: [Subscription; 2],
}

impl ScreenActionsView {
    pub(crate) fn new(
        deps: ActionsDeps,
        settings_repo: Arc<dyn SettingsRepo>,
        overlay_kinds: Arc<OverlayKindRegistry>,
        preselect: Option<ActionId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let screen_deps = ScreenDeps {
            queue_repo: Arc::clone(&deps.queue_repo),
            actions_service: Arc::clone(&deps.actions_service),
            trigger_registry: Arc::clone(&deps.trigger_registry),
            rt_handle: deps.rt_handle.clone(),
        };
        let tree_deps = ActionTreeDeps {
            action_repo: Arc::clone(&deps.action_repo),
            trigger_instance_repo: Arc::clone(&deps.trigger_instance_repo),
            trigger_registry: Arc::clone(&deps.trigger_registry),
            rt_handle: deps.rt_handle.clone(),
        };
        let tree = cx.new(|cx| ActionTree::new(tree_deps, preselect, cx));
        let list_reads = tree.read(cx).list_reads();
        let editor = cx.new(|cx| {
            ActionEditor::new(
                deps,
                settings_repo,
                overlay_kinds,
                list_reads,
                preselect,
                cx,
            )
        });
        let children = [
            cx.subscribe(&tree, Self::on_tree_event),
            cx.subscribe(&editor, Self::on_editor_event),
        ];
        Self {
            deps: screen_deps,
            tree,
            editor,
            modals: ScreenModals::default(),
            _children: children,
        }
    }

    #[must_use]
    pub(crate) fn with_builtins(
        self,
        builtins: forge_host::integrations::BuiltinRegistry,
        cx: &mut Context<Self>,
    ) -> Self {
        self.editor
            .update(cx, |editor, _| editor.set_builtins(builtins));
        self
    }

    #[must_use]
    pub(crate) fn with_scheduled_runs(
        self,
        repo: Arc<dyn forge_storage::ScheduledRunRepo>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.tree
            .update(cx, |tree, _| tree.set_scheduled_runs(repo));
        self
    }

    #[must_use]
    pub(crate) fn with_integration_switch(
        self,
        switch: crate::integration_switch::IntegrationSwitch,
        cx: &mut Context<Self>,
    ) -> Self {
        self.editor.update(cx, |editor, cx| {
            editor.attach_integration_switch(switch, cx)
        });
        self
    }

    fn on_tree_event(
        &mut self,
        _tree: Entity<ActionTree>,
        event: &ActionTreeEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActionTreeEvent::Selected(id) => {
                let id = *id;
                self.editor
                    .update(cx, |editor, cx| editor.open_action(id, cx));
            }
            ActionTreeEvent::SelectedListed => {
                self.editor
                    .update(cx, |editor, cx| editor.reload_detail(cx));
            }
            ActionTreeEvent::SelectionLost => {
                self.editor
                    .update(cx, |editor, cx| editor.clear_selection(cx));
            }
            ActionTreeEvent::Write(id, write) => {
                let (id, write) = (*id, write.clone());
                self.editor
                    .update(cx, |editor, cx| editor.write_action(id, write, cx));
            }
            ActionTreeEvent::DeletePromptChanged => self.sync_step_drag_lock(cx),
        }
    }

    fn on_editor_event(
        &mut self,
        _editor: Entity<ActionEditor>,
        event: &ActionEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ActionEditorEvent::SelectAction(id) => {
                let id = *id;
                self.tree.update(cx, |tree, cx| tree.select(id, cx));
            }
            ActionEditorEvent::ListRead { list_read, listed } => {
                let (list_read, listed) = (*list_read, listed.clone());
                self.tree
                    .update(cx, |tree, cx| tree.land_list(list_read, listed, cx));
            }
            ActionEditorEvent::TriggerKindsRead { id, kinds } => {
                self.tree
                    .update(cx, |tree, _| tree.sync_filter_membership(*id, kinds));
                cx.notify();
            }
            ActionEditorEvent::RequestDelete(id) => {
                let id = *id;
                self.tree.update(cx, |tree, cx| tree.request_delete(id, cx));
            }
            ActionEditorEvent::EditAction(action) => {
                self.open_action_form(Some(action.clone()), cx);
            }
            ActionEditorEvent::OpenHistory => self.open_history_modal(cx),
            ActionEditorEvent::ExportPack(id) => self.export_pack(*id, cx),
            ActionEditorEvent::Navigate(screen) => cx.emit(NavRequested(screen.clone())),
        }
    }

    fn open_action_form(&mut self, base: Option<Action>, cx: &mut Context<Self>) {
        let queue_repo = Arc::clone(&self.deps.queue_repo);
        let rt_handle = self.deps.rt_handle.clone();
        let view = cx.new(|cx| ActionFormModal::new(base, queue_repo, &rt_handle, cx));
        let sub = cx.subscribe(&view, Self::on_action_form_event);
        self.modals.action_form = Some(ActionFormHost { view, _sub: sub });
        self.sync_step_drag_lock(cx);
        cx.notify();
    }

    fn on_action_form_event(
        &mut self,
        _form: Entity<ActionFormModal>,
        event: &ActionFormEvent,
        cx: &mut Context<Self>,
    ) {
        self.modals.action_form = None;
        self.sync_step_drag_lock(cx);
        if let ActionFormEvent::Saved(id, write) = event {
            let (id, write) = (*id, ActionWrite::clone(write));
            self.editor
                .update(cx, |editor, cx| editor.write_action(id, write, cx));
        }
        cx.notify();
    }

    fn render_new_action_button(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        primary_button_with_icon(Icon::Plus, tr!("actions_modal_new_action_title"), palette)
            .on_click(
                "actions-new",
                cx.listener(|this, _: &ClickEvent, _, cx| this.open_action_form(None, cx)),
            )
            .into_any_element()
    }

    fn sync_step_drag_lock(&self, cx: &mut Context<Self>) {
        let locked = self.screen_modal_open(cx);
        self.editor
            .update(cx, |editor, _| editor.set_step_drag_locked(locked));
    }

    fn screen_modal_open(&self, cx: &App) -> bool {
        self.modals.action_form.is_some()
            || self.modals.history.is_some()
            || self.tree.read(cx).delete_pending()
            || self
                .modals
                .pack_export
                .as_ref()
                .is_some_and(|host| host.flow.read(cx).is_busy())
    }
}

impl EventEmitter<NavRequested> for ScreenActionsView {}

impl Render for ScreenActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let palette = cx.palette();

        let stats = self.tree.read(cx).render_stats(&palette, density, cx);
        let filter_left = self
            .tree
            .update(cx, |tree, cx| tree.render_filter_left(&palette, cx));
        let filter_right = self.render_new_action_button(&palette, cx);

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(self.tree.clone())
            .child(self.editor.clone());

        let action_modal = self
            .modals
            .action_form
            .as_ref()
            .map(|host| host.view.clone());
        let history_modal = self.modals.history.as_ref().map(|host| host.view.clone());
        let test_run_modal = self.editor.read(cx).test_run_modal();
        let delete_modal = self
            .tree
            .update(cx, |tree, cx| tree.render_delete_layer(&palette, cx));
        let editor_layers = self
            .editor
            .update(cx, |editor, cx| editor.render_modal_layers(&palette, cx));
        let row_menu = self
            .tree
            .update(cx, |tree, cx| tree.render_row_context_menu(&palette, cx));
        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("actions_breadcrumb_automation")),
                BreadcrumbCrumb::leaf(tr!("actions_breadcrumb_actions")),
            ],
            &palette,
        )
        .header_right(stats)
        .subheader_left(filter_left)
        .subheader_right(filter_right)
        .density(density)
        .body(body);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(frame)
            .children(action_modal)
            .children(history_modal)
            .children(test_run_modal)
            .children(delete_modal)
            .children(editor_layers)
            .children(
                self.modals
                    .pack_export
                    .as_ref()
                    .map(|host| host.flow.clone()),
            )
            .children(row_menu)
    }
}
