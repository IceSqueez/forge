use crate::async_bridge;
use crate::presentation::ActivePresentation;
use crate::run_history_modal::RunHistoryModal;
use crate::screen::Screen;
use crate::sidebar::NavRequested;
use crate::toasts::PushToast;
use forge_components::{
    BreadcrumbCrumb, Confirm, Density, ForgePalette, GridPicker, Icon, InlineEdit, OverlayPosition,
    SearchState, TextArea, TextInput, ToastKind, fmt_number, fmt_relative_time, icon, overlay,
    page_frame, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::actions::{ActionDetail, ActionsService};
use forge_runtime::{EventBus, QueueSchedulerHandle};
use forge_storage::{
    ActionRepo, ActionTelemetry, GlobalsRepo, OverlayRepo, QueueRepo, ScriptRepo, SettingsRepo,
    SoundboardClipsRepo, TriggerInstanceRepo, reserved_keys,
};
use forge_tts_core::TtsRegistry;
use forge_types::{Action, ActionId, ExecutionOutcome, QueueId, SubActionStep, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, Pixels, Point,
    SharedString, Subscription, Window, div, prelude::*, px,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, RwLock};

mod analyzer;
mod branch;
mod editor;
mod list;
mod nav;
mod overlay_schema;
mod run_history;
mod sub_action_modal;
mod test_run;
mod test_trigger;

pub(crate) use editor::parse_variable_segments;

const LEFT_PANEL_W: Pixels = px(290.0);
const LEFT_PANEL_MIN: Pixels = px(220.0);
const LEFT_PANEL_MAX: Pixels = px(480.0);
const ROW_HEIGHT: Pixels = px(30.0);
const RIGHT_SLOT_W: Pixels = px(46.0);
const ROW_INDENT: Pixels = px(32.0);
const ROW_GUTTER: Pixels = px(14.0);
const STRIPE_W: Pixels = px(2.0);
const TREE_GUTTER: Pixels = px(14.0);
const TREE_GLYPH: Pixels = px(11.0);
const SEARCH_W: Pixels = px(240.0);
const HEADER_DIV_W: Pixels = px(0.5);
const HEADER_DIV_H: Pixels = px(16.0);
const GROUP_DOT: Pixels = px(8.0);
const EMPTY_GLYPH: Pixels = px(28.0);
const NAME_LIMIT: usize = 64;

const PILL_RADIUS: Pixels = px(8.0);
const PILL_DOT: Pixels = px(5.0);
const CARD_GLYPH: Pixels = px(13.0);
const TRIGGER_DOT: Pixels = px(7.0);
const STEP_HEALTH_TILE: Pixels = px(18.0);
const STEP_HEALTH_GLYPH: Pixels = px(12.0);
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
const HALF_BORDER: Pixels = px(0.5);
const STAT_GAP: Pixels = px(8.0);
const STAT_PAD_V: Pixels = px(10.0);
const STAT_VALUE_GAP: Pixels = px(3.0);
const PANE_PAD_V: Pixels = px(18.0);
const PANE_PAD_H: Pixels = px(22.0);
const STEP_GAP: Pixels = px(6.0);
const STEP_DISABLED_OPACITY: f32 = 0.55;
const STEP_CARD_PAD_V: Pixels = px(10.0);
const STEP_CARD_PAD_H: Pixels = px(12.0);
const HEADER_ACTION_H: Pixels = px(28.0);
const SUB_MODAL_MAX_H: Pixels = px(440.0);
const SUB_AREA_FIELD_H: Pixels = px(150.0);
const STEP_MODAL_W: Pixels = px(560.0);
const STEP_TILE: Pixels = px(32.0);
const STEP_TILE_GLYPH: Pixels = px(16.0);
const GRID_COL_GAP: Pixels = px(12.0);

const CHIP_RADIUS: Pixels = px(6.0);
const BRANCH_GLYPH: Pixels = px(11.0);
const CASE_MATCH_W: Pixels = px(160.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionCategory {
    Chat,
    Timers,
    Points,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionsFilter {
    All,
    Chat,
    Timers,
    Points,
}

struct ActionsTreeResizeDrag;

struct ActionSummary {
    id: ActionId,
    name: String,
    enabled: bool,
    sub_action_count: usize,
}

struct ActionGroup {
    name: SharedString,
    category: ActionCategory,
    collapsed: bool,
    actions: Vec<ActionSummary>,
}

struct Renaming {
    id: ActionId,
    editor: Entity<InlineEdit>,
    _sub: Subscription,
}

struct ActionForm {
    editing: Option<ActionId>,
    base: Option<Action>,
    name: Entity<TextInput>,
    group: Entity<TextInput>,
    description: Entity<TextArea>,
    queues: Vec<(QueueId, SharedString)>,
    selected_queue: usize,
    preselect_queue: Option<QueueId>,
    enabled: bool,
    concurrent: bool,
    bypass_pause: bool,
    random_pick: bool,
    _name_sub: Subscription,
}

struct HistoryModalHost {
    view: Entity<RunHistoryModal>,
    _sub: Subscription,
}

pub struct ScreenActionsView {
    action_repo: Arc<dyn ActionRepo>,
    queue_repo: Arc<dyn QueueRepo>,
    actions_service: Arc<ActionsService>,
    trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
    script_repo: Arc<dyn ScriptRepo>,
    soundboard_repo: Arc<dyn SoundboardClipsRepo>,
    globals_repo: Arc<dyn GlobalsRepo>,
    settings_repo: Arc<dyn SettingsRepo>,
    overlay_repo: Arc<dyn OverlayRepo>,
    overlay_schema: Arc<overlay_schema::OverlayContentSchema>,
    sub_action_favorites: HashSet<SharedString>,
    trigger_favorites: HashSet<SharedString>,
    tts_registry: Option<Arc<RwLock<TtsRegistry>>>,
    sub_action_registry: Arc<SubActionRegistry>,
    trigger_registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
    bus: Arc<EventBus>,
    scheduler: QueueSchedulerHandle,
    select_options: HashMap<String, Vec<(String, String)>>,
    concurrent_queue_ids: HashSet<String>,
    tree_width: Pixels,
    loading: bool,
    groups: Vec<ActionGroup>,
    filter: ActionsFilter,
    search: SearchState,
    selected: Option<ActionId>,
    hovered: Option<ActionId>,
    menu_open: Option<ActionId>,
    renaming: Option<Renaming>,
    action_modal: Option<ActionForm>,
    history_modal: Option<HistoryModalHost>,
    test_run: Option<Entity<test_run::TestRunModal>>,
    _test_run_sub: Option<Subscription>,
    header_menu_open: Option<Point<Pixels>>,
    pending_delete: Confirm<ActionId>,
    detail: Option<ActionDetail>,
    telemetry: Option<ActionTelemetry>,
    last_outcome: Option<ExecutionOutcome>,
    sub_form: Option<Entity<sub_action_modal::EditSubActionForm>>,
    _sub_form_sub: Option<Subscription>,
    step_menu_open: Option<usize>,
    menu_click_pos: Option<Point<Pixels>>,
    grid_picker: Option<GridPickerForm>,
    add_trigger: Option<AddTriggerStage>,
    nav_path: Vec<nav::NavFrame>,
    /// Keyed by `(step_index, case_index)` within the current chain.
    case_fields: BTreeMap<(usize, usize), CaseField>,
    step_health: Vec<analyzer::StepHealth>,
    _search_sub: Subscription,
}

impl ScreenActionsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        action_repo: Arc<dyn ActionRepo>,
        queue_repo: Arc<dyn QueueRepo>,
        actions_service: Arc<ActionsService>,
        trigger_instance_repo: Arc<dyn TriggerInstanceRepo>,
        script_repo: Arc<dyn ScriptRepo>,
        soundboard_repo: Arc<dyn SoundboardClipsRepo>,
        globals_repo: Arc<dyn GlobalsRepo>,
        settings_repo: Arc<dyn SettingsRepo>,
        overlay_repo: Arc<dyn OverlayRepo>,
        overlay_kinds: Arc<OverlayKindRegistry>,
        tts_registry: Option<Arc<RwLock<TtsRegistry>>>,
        sub_action_registry: Arc<SubActionRegistry>,
        trigger_registry: Arc<TriggerRegistry>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        scheduler: QueueSchedulerHandle,
        preselect: Option<ActionId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("actions_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);

        let view = Self {
            action_repo,
            queue_repo,
            actions_service,
            trigger_instance_repo,
            script_repo,
            soundboard_repo,
            globals_repo,
            settings_repo,
            overlay_repo,
            overlay_schema: Arc::new(overlay_schema::OverlayContentSchema::new(overlay_kinds)),
            sub_action_favorites: HashSet::new(),
            trigger_favorites: HashSet::new(),
            tts_registry,
            sub_action_registry,
            trigger_registry,
            rt_handle,
            bus,
            scheduler,
            select_options: HashMap::new(),
            concurrent_queue_ids: HashSet::new(),
            tree_width: LEFT_PANEL_W,
            loading: true,
            groups: Vec::new(),
            filter: ActionsFilter::All,
            search,
            selected: preselect,
            hovered: None,
            menu_open: None,
            renaming: None,
            action_modal: None,
            history_modal: None,
            test_run: None,
            _test_run_sub: None,
            header_menu_open: None,
            pending_delete: Confirm::default(),
            detail: None,
            telemetry: None,
            last_outcome: None,
            sub_form: None,
            _sub_form_sub: None,
            step_menu_open: None,
            menu_click_pos: None,
            grid_picker: None,
            add_trigger: None,
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            step_health: Vec::new(),
            _search_sub: search_sub,
        };
        view.reload(cx);
        view.load_favorites(cx);
        view
    }

    fn load_favorites(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.settings_repo);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                let subs = forge_storage::get_json_setting::<Vec<String>>(
                    repo.as_ref(),
                    reserved_keys::PICKER_FAVORITES_SUB_ACTIONS,
                )
                .await
                .unwrap_or_default();
                let trigs = forge_storage::get_json_setting::<Vec<String>>(
                    repo.as_ref(),
                    reserved_keys::PICKER_FAVORITES_TRIGGERS,
                )
                .await
                .unwrap_or_default();
                (subs, trigs)
            },
            |this, (subs, trigs): (Vec<String>, Vec<String>), _cx| {
                this.sub_action_favorites = crate::picker_favorites::to_set(subs);
                this.trigger_favorites = crate::picker_favorites::to_set(trigs);
            },
            cx,
        );
    }

    fn persist_favorites(
        &self,
        key: &'static str,
        favorites: HashSet<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let repo = Arc::clone(&self.settings_repo);
        let ids = crate::picker_favorites::to_ids(&favorites);
        async_bridge::run_async(
            &self.rt_handle,
            async move {
                forge_storage::set_json_setting(repo.as_ref(), key, &ids)
                    .await
                    .map_err(|e| e.to_string())
            },
            |this, result: Result<(), String>, cx| {
                if let Err(message) = result {
                    this.on_repo_error(&message, cx);
                }
            },
            cx,
        );
    }

    fn set_tree_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.tree_width != width {
            self.tree_width = width;
            cx.notify();
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move { repo.list().await.map_err(|e| e.to_string()) },
            cx,
        );
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    fn reload_entity(
        view: Entity<ScreenActionsView>,
        rt_handle: tokio::runtime::Handle,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        app: &mut App,
    ) {
        async_bridge::run_async_entity(
            &rt_handle,
            view,
            work,
            |this, result, cx| match result {
                Ok(actions) => this.apply_actions(actions, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            app,
        );
    }

    fn apply_actions(&mut self, actions: Vec<Action>, cx: &mut Context<Self>) {
        let collapsed: HashSet<SharedString> = self
            .groups
            .iter()
            .filter(|g| g.collapsed)
            .map(|g| g.name.clone())
            .collect();
        let mut groups = group_actions(actions);
        for group in &mut groups {
            if collapsed.contains(&group.name) {
                group.collapsed = true;
            }
        }
        self.groups = groups;

        if let Some(selected) = self.selected {
            if self.find(selected).is_some() {
                self.reload_detail(cx);
            } else {
                self.selected = None;
                self.detail = None;
                self.telemetry = None;
                self.last_outcome = None;
                self.nav_path.clear();
                self.case_fields.clear();
            }
        }
        self.loading = false;
        cx.notify();
    }

    fn on_repo_error(&mut self, message: &str, cx: &mut Context<Self>) {
        eprintln!("forge-desktop: actions operation failed: {message}");
        self.loading = false;
        cx.push_toast(
            ToastKind::Error,
            tr!("actions_toast_error", message = message),
        );
        cx.notify();
    }

    fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            self.load_detail_for(id, cx);
        }
    }

    pub(super) fn load_detail_for(&self, id: ActionId, cx: &mut Context<Self>) {
        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.load_detail(id).await.map_err(|e| e.to_string()) },
            move |this, result, cx| match result {
                Ok(detail) => this.apply_detail(id, detail, cx),
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );

        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.load_telemetry(id).await },
            move |this, result, cx| {
                if let Ok(telemetry) = result {
                    this.apply_telemetry(id, telemetry, cx);
                }
            },
            cx,
        );

        let service = Arc::clone(&self.actions_service);
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.recent_runs(id, 1).await },
            move |this, result, cx| {
                if let Ok(runs) = result {
                    let outcome = runs.into_iter().next().map(|ctx| ctx.outcome);
                    this.apply_last_outcome(id, outcome, cx);
                }
            },
            cx,
        );
    }

    fn apply_detail(&mut self, id: ActionId, detail: ActionDetail, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            return;
        }
        self.detail = Some(detail);
        self.sync_case_fields(cx);
        self.recompute_step_health();
        cx.notify();
    }

    fn recompute_step_health(&mut self) {
        self.step_health = match &self.detail {
            Some(detail) => analyzer::analyze(
                &detail.action,
                &detail.trigger_instances,
                &detail.last_step_outcomes,
                &self.sub_action_registry,
                &self.trigger_registry,
            ),
            None => Vec::new(),
        };
    }

    fn apply_telemetry(
        &mut self,
        id: ActionId,
        telemetry: ActionTelemetry,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        self.telemetry = Some(telemetry);
        cx.notify();
    }

    fn apply_last_outcome(
        &mut self,
        id: ActionId,
        outcome: Option<ExecutionOutcome>,
        cx: &mut Context<Self>,
    ) {
        if self.selected != Some(id) {
            return;
        }
        self.last_outcome = outcome;
        cx.notify();
    }

    pub(super) fn persist_action(&self, action: Action, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move {
                repo.save(&action).await.map_err(|e| e.to_string())?;
                repo.list().await.map_err(|e| e.to_string())
            },
            cx,
        );
    }

    pub(super) fn persist_chain_mutation(
        &mut self,
        mutate: impl FnOnce(&mut Vec<SubActionStep>),
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let mut action = detail.action.clone();
        let path = self.nav_path.clone();
        let mut chain = nav::resolve_chain(&action.sub_actions, &path);
        mutate(&mut chain);
        if !nav::set_chain(&mut action.sub_actions, &path, &chain) {
            return;
        }
        self.persist_action(action, cx);
    }
}

impl EventEmitter<NavRequested> for ScreenActionsView {}

impl Render for ScreenActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let stats = self.render_stats(&palette);
        let filter_left = self.render_filter_left(&palette, cx);
        let filter_right = self.render_filter_right(&palette, cx);
        let tree = self.render_tree(&palette, cx);
        let editor = self.render_editor_pane(&palette, cx);

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(tree)
            .child(editor);

        let action_modal = self
            .action_modal
            .as_ref()
            .map(|form| self.render_action_modal(form, &palette, cx));
        let history_modal = self.history_modal.as_ref().map(|host| host.view.clone());
        let test_run_modal = self.test_run.clone();
        let delete_modal = self
            .pending_delete
            .get()
            .copied()
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let sub_modal = self.sub_form.clone();
        let grid_picker = self.grid_picker.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_grid_picker(cx));
                })
                .into_any_element()
        });
        let trigger_grid = self
            .add_trigger
            .as_ref()
            .map(|stage| self.render_add_trigger(stage, &palette, cx));
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
        .density(Density::Cozy)
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
            .children(sub_modal)
            .children(grid_picker)
            .children(trigger_grid)
    }
}

fn group_actions(actions: Vec<Action>) -> Vec<ActionGroup> {
    let mut by_name: BTreeMap<String, Vec<ActionSummary>> = BTreeMap::new();
    for action in actions {
        let group_name = action
            .group
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_uppercase)
            .unwrap_or_else(|| "UNGROUPED".to_owned());
        let summary = ActionSummary {
            id: action.id,
            name: action.name,
            enabled: action.enabled,
            sub_action_count: action.sub_actions.len(),
        };
        by_name.entry(group_name).or_default().push(summary);
    }
    by_name
        .into_iter()
        .map(|(name, mut actions)| {
            actions.sort_by_key(|a| a.name.to_lowercase());
            let category = category_from_group_name(&name);
            ActionGroup {
                name: name.into(),
                category,
                collapsed: false,
                actions,
            }
        })
        .collect()
}

fn category_from_group_name(name: &str) -> ActionCategory {
    match name {
        "CHAT COMMANDS" => ActionCategory::Chat,
        "TIMERS" => ActionCategory::Timers,
        "CHANNEL POINTS" => ActionCategory::Points,
        _ => ActionCategory::Other,
    }
}

struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    action_id: ActionId,
    _sub: Subscription,
}

enum AddTriggerStage {
    Pick(AddTriggerPicker),
    Fill(AddTriggerFill),
}

struct AddTriggerPicker {
    picker: Entity<GridPicker>,
    picks_kind: HashMap<SharedString, String>,
    picks_instance: HashMap<SharedString, TriggerInstanceId>,
    action_id: ActionId,
    _sub: Subscription,
}

struct AddTriggerFill {
    action_id: ActionId,
    kind_id: String,
    kind_label: String,
    name_field: Entity<TextInput>,
    fields: Vec<crate::config_form::ConfigField>,
    saving: bool,
    _name_sub: Subscription,
}

struct CaseField {
    field: Entity<TextInput>,
    _sub: Subscription,
}

fn step_icon_btn(
    id: impl Into<ElementId>,
    glyph: Icon,
    disabled: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let color = if disabled {
        palette.disabled
    } else {
        palette.text_faint
    };
    let base = div()
        .flex()
        .items_center()
        .justify_center()
        .size(STEP_BTN)
        .rounded(STEP_BTN_RADIUS)
        .child(icon(glyph, STEP_BTN_GLYPH, color));
    if disabled {
        return base.into_any_element();
    }
    let hover = palette.surface_overlay;
    base.id(id.into())
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .into_any_element()
}
