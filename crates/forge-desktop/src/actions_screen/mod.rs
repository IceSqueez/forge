use crate::async_bridge::{self, Loadable};
use crate::config_form::CollectionChoices;
use crate::integration_switch::SwitchWatch;
use crate::navigation::NavRequested;
use crate::navigation::Screen;
use crate::picker_favorites::{SUB_ACTION_FAVORITES, StoredFavorites, TRIGGER_FAVORITES};
use crate::repo_failure::RepoFailure;
use crate::run_history_modal::RunHistoryModal;
use forge_components::ActivePresentation;
use forge_components::PushToast;
use forge_components::Radius;
use forge_components::drive_overlay_focus;
use forge_components::radius;
use forge_components::{
    BreadcrumbCrumb, Density, ForgePalette, GridPicker, Icon, InlineEdit, KeyActivated,
    OverlayPosition, SearchState, TextArea, TextInput, ToastKind, fmt_display_relative, fmt_number,
    icon, overlay, page_frame, tr,
};
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerCategory, TriggerRegistry};
use forge_runtime::ActionDetail;
use forge_runtime::translate_legacy_time_steps;
use forge_runtime::{EventBus, QueueSchedulerHandle};
use forge_storage::{ActionRepo, ActionTelemetry, SettingsRepo, StorageError, TriggerInstanceRepo};
use forge_types::{Action, ActionId, ExecutionOutcome, QueueId, SubActionStep, TriggerInstanceId};
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
mod action_writes;
mod analyzer;
mod branch;
mod collection_choices;
pub(crate) mod config_problem_text;
mod delete_confirm_count;
mod editor;
mod files_root_note;
mod integration_gating;
mod list;
mod nav;
pub(crate) mod overlay_schema;
mod run_history;
mod shortcuts;
mod state;
mod step_validation;
mod sub_action_modal;
mod test_run;
mod test_trigger;

pub(crate) use editor::parse_variable_segments;
pub(crate) use state::ActionsDeps;
use state::{ActionEditorState, ActionMenus, ActionModals, ActionTreeState, StepChoices};

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
const HEADER_DIV_H: Pixels = px(16.0);
const GROUP_DOT: Pixels = px(8.0);
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

const TRIGGER_FILTERS: [(ActionsFilter, TriggerCategory); 3] = [
    (ActionsFilter::Chat, TriggerCategory::Chat),
    (ActionsFilter::Timers, TriggerCategory::Timer),
    (ActionsFilter::Points, TriggerCategory::ChannelPoints),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
    _subs: [Subscription; 2],
}

pub(crate) struct ScreenActionsView {
    deps: ActionsDeps,
    overlay_schema: Arc<overlay_schema::OverlayContentSchema>,
    sub_action_favorites: Entity<StoredFavorites>,
    trigger_favorites: Entity<StoredFavorites>,
    scheduled_runs: Option<Arc<dyn forge_storage::ScheduledRunRepo>>,
    integrations: Option<SwitchWatch>,
    tree: ActionTreeState,
    menus: ActionMenus,
    choices: StepChoices,
    editor: ActionEditorState,
    modals: ActionModals,
}

fn next_list_read(list_reads: &AtomicU64) -> u64 {
    list_reads.fetch_add(1, Ordering::Relaxed) + 1
}

impl ScreenActionsView {
    pub(crate) fn new(
        deps: ActionsDeps,
        settings_repo: Arc<dyn SettingsRepo>,
        overlay_kinds: Arc<OverlayKindRegistry>,
        preselect: Option<ActionId>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search = SearchState::new(cx, palette, tr!("actions_search_placeholder"));
        let search_sub = cx.subscribe(search.field(), Self::on_search_event);
        let sub_action_favorites = StoredFavorites::shared(
            SUB_ACTION_FAVORITES,
            Arc::clone(&settings_repo),
            &deps.rt_handle,
            cx,
        );
        let trigger_favorites = StoredFavorites::shared(
            TRIGGER_FAVORITES,
            Arc::clone(&settings_repo),
            &deps.rt_handle,
            cx,
        );

        let view = Self {
            deps,
            overlay_schema: Arc::new(overlay_schema::OverlayContentSchema::new(overlay_kinds)),
            sub_action_favorites,
            trigger_favorites,
            scheduled_runs: None,
            integrations: None,
            tree: ActionTreeState::new(search, search_sub, preselect, cx),
            menus: ActionMenus::new(cx),
            choices: StepChoices::new(),
            editor: ActionEditorState::new(cx),
            modals: ActionModals::default(),
        };
        view.reload(cx);
        view.fetch_select_options(cx);
        view
    }

    fn set_tree_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.tree.width != width {
            self.tree.width = width;
            cx.notify();
        }
    }

    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.deps.action_repo);
        self.spawn_reload(
            async move { repo.list().await.map_err(RepoFailure::from) },
            cx,
        );
    }

    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<Action>, RepoFailure>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let list_read = next_list_read(&self.tree.list_reads);
        async_bridge::run_async(
            &self.deps.rt_handle,
            work,
            move |this, result, cx| this.land_list(list_read, result, cx),
            cx,
        );
    }

    fn land_list(
        &mut self,
        list_read: u64,
        result: Result<Vec<Action>, RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        if list_read <= self.tree.landed_list_read {
            return;
        }
        self.tree.landed_list_read = list_read;
        match result {
            Ok(actions) => self.apply_actions(actions, cx),
            Err(failure) => {
                self.on_list_load_failed(failure, cx);
                self.reload_detail(cx);
            }
        }
    }

    fn on_list_load_failed(&mut self, failure: RepoFailure, cx: &mut Context<Self>) {
        if self.total_actions() > 0 {
            self.on_repo_error(&failure, cx);
        }
        self.tree.list_load = Loadable::Failed(failure);
        cx.notify();
    }

    fn retry_list_load(&mut self, cx: &mut Context<Self>) {
        self.tree.list_load = Loadable::Loading;
        self.reload(cx);
        cx.notify();
    }

    fn apply_actions(&mut self, actions: Vec<Action>, cx: &mut Context<Self>) {
        let collapsed: HashSet<SharedString> = self
            .tree
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
        self.tree.groups = groups;
        self.reload_filter_actions(cx);

        if let Some(selected) = self.tree.selected {
            if self.find(selected).is_some() {
                self.reload_detail(cx);
            } else {
                self.tree.selected = None;
                self.editor.detail = None;
                self.editor.telemetry = None;
                self.editor.last_outcome = None;
                self.editor.nav_path.clear();
                self.editor.case_fields.clear();
            }
        }
        self.tree.list_load = Loadable::Loaded(());
        cx.notify();
    }

    fn filter_by_kind_id(&self) -> HashMap<String, ActionsFilter> {
        self.deps
            .trigger_registry
            .all()
            .filter_map(|d| {
                let filter = TRIGGER_FILTERS
                    .iter()
                    .find(|(_, category)| *category == d.category())
                    .map(|(filter, _)| *filter)?;
                Some((d.id().to_owned(), filter))
            })
            .collect()
    }

    fn reload_filter_actions(&self, cx: &mut Context<Self>) {
        let filter_by_kind = self.filter_by_kind_id();
        let repo = Arc::clone(&self.deps.trigger_instance_repo);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move {
                actions_by_trigger_filter(&*repo, &filter_by_kind)
                    .await
                    .map_err(RepoFailure::from)
            },
            |this, result, cx| match result {
                Ok(ids) => this.apply_filter_actions(ids, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_filter_actions(
        &mut self,
        ids: HashMap<ActionsFilter, HashSet<ActionId>>,
        cx: &mut Context<Self>,
    ) {
        self.tree.filter_action_ids = ids;
        cx.notify();
    }

    fn sync_filter_membership(&mut self, detail: &ActionDetail) {
        let filter_by_kind = self.filter_by_kind_id();
        for (filter, _) in TRIGGER_FILTERS {
            let has_trigger = detail
                .trigger_instances
                .iter()
                .any(|instance| filter_by_kind.get(&instance.kind_id) == Some(&filter));
            let members = self.tree.filter_action_ids.entry(filter).or_default();
            if has_trigger {
                members.insert(detail.action.id);
            } else {
                members.remove(&detail.action.id);
            }
        }
    }

    fn on_repo_error(&mut self, failure: &RepoFailure, cx: &mut Context<Self>) {
        tracing::error!(error = %failure, "actions operation failed");
        let message = failure.text();
        cx.push_toast(
            ToastKind::Error,
            tr!("actions_toast_error", message = message.as_str()),
        );
        cx.notify();
    }

    fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.tree.selected {
            self.load_detail_for(id, cx);
        }
    }

    pub(super) fn load_detail_for(&self, id: ActionId, cx: &mut Context<Self>) {
        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.load_detail(id).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(detail) => this.apply_detail(id, detail, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );

        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.load_telemetry(id).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(telemetry) => this.apply_telemetry(id, telemetry, cx),
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );

        let service = Arc::clone(&self.deps.actions_service);
        async_bridge::run_async(
            &self.deps.rt_handle,
            async move { service.recent_runs(id, 1).await.map_err(RepoFailure::from) },
            move |this, result, cx| match result {
                Ok(runs) => {
                    let outcome = runs.into_iter().next().map(|ctx| ctx.outcome);
                    this.apply_last_outcome(id, outcome, cx);
                }
                Err(failure) => this.on_repo_error(&failure, cx),
            },
            cx,
        );
    }

    fn apply_detail(&mut self, id: ActionId, mut detail: ActionDetail, cx: &mut Context<Self>) {
        if self.tree.selected != Some(id) {
            return;
        }
        translate_legacy_time_steps(&mut detail.action.sub_actions);
        if let Some(latest) = self.latest_written_action(id) {
            detail.action = latest.clone();
        } else if self.has_pending_action_writes(id)
            && let Some(current) = self.editor.detail.as_ref()
            && current.action.id == id
        {
            detail.action = current.action.clone();
        }
        self.sync_filter_membership(&detail);
        self.editor.detail = Some(detail);
        self.sync_case_fields(cx);
        self.recompute_step_health();
        cx.notify();
    }

    fn recompute_step_health(&mut self) {
        self.editor.step_health = match &self.editor.detail {
            Some(detail) => {
                let mut health = forge_registry::analyze(
                    &detail.action,
                    &detail.trigger_instances,
                    &detail.last_step_outcomes,
                    &self.deps.sub_action_registry,
                    &self.deps.trigger_registry,
                );
                if let Some(integrations) = &self.integrations {
                    forge_registry::flag_switched_off(
                        &detail.action.sub_actions,
                        &self.deps.sub_action_registry,
                        integrations.off(),
                        &mut health,
                    );
                    forge_registry::flag_failed(
                        &detail.action.sub_actions,
                        &self.deps.sub_action_registry,
                        integrations.failed(),
                        &mut health,
                    );
                }
                health
            }
            None => Vec::new(),
        };
    }

    fn apply_telemetry(
        &mut self,
        id: ActionId,
        telemetry: ActionTelemetry,
        cx: &mut Context<Self>,
    ) {
        if self.tree.selected != Some(id) {
            return;
        }
        self.editor.telemetry = Some(telemetry);
        cx.notify();
    }

    fn apply_last_outcome(
        &mut self,
        id: ActionId,
        outcome: Option<ExecutionOutcome>,
        cx: &mut Context<Self>,
    ) {
        if self.tree.selected != Some(id) {
            return;
        }
        self.editor.last_outcome = outcome;
        cx.notify();
    }

    pub(super) fn persist_chain_mutation(
        &mut self,
        mutate: impl FnOnce(&mut Vec<nav::ChainItem>),
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.editor.detail.as_ref() else {
            return;
        };
        let id = detail.action.id;
        let path = self.editor.nav_path.clone();
        let mut chain = nav::resolve_chain(&detail.action.sub_actions, &path);
        mutate(&mut chain);
        self.write_action(id, action_writes::ActionWrite::SetChain { path, chain }, cx);
    }
}

impl EventEmitter<NavRequested> for ScreenActionsView {}

impl Render for ScreenActionsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drive_overlay_focus(
            self.menus.row_open.is_some(),
            &self.menus.focus,
            &mut self.menus.focus_restore,
            window,
            cx,
        );
        self.tree.keys.claim_focus_once(window, cx);
        let density = cx.density();
        self.release_step_drag(window, cx);
        let palette = cx.palette();

        let stats = self.render_stats(&palette, density);
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
            .modals
            .action_form
            .as_ref()
            .map(|form| self.render_action_modal(form, &palette, cx));
        let history_modal = self.modals.history.as_ref().map(|host| host.view.clone());
        let test_run_modal = self.modals.test_run.clone();
        let delete_modal = self
            .tree
            .pending_delete
            .get()
            .copied()
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let sub_modal = self.modals.sub_form.clone();
        let grid_picker = self.modals.grid_picker.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_grid_picker(cx));
                })
                .into_any_element()
        });
        let trigger_grid = self
            .modals
            .add_trigger
            .as_ref()
            .map(|stage| self.render_add_trigger(stage, &palette, cx));
        let row_menu = self.render_row_context_menu(&palette, cx);
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
            .children(sub_modal)
            .children(grid_picker)
            .children(trigger_grid)
            .children(
                self.modals
                    .pack_export
                    .as_ref()
                    .map(|host| host.flow.clone()),
            )
            .children(row_menu)
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
            .unwrap_or_else(|| tr!("actions_group_ungrouped").to_uppercase());
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
            ActionGroup {
                name: name.into(),
                collapsed: false,
                actions,
            }
        })
        .collect()
}

async fn actions_by_trigger_filter(
    repo: &dyn TriggerInstanceRepo,
    filter_by_kind: &HashMap<String, ActionsFilter>,
) -> Result<HashMap<ActionsFilter, HashSet<ActionId>>, StorageError> {
    let mut links = repo.actions_by_instance().await?;
    let mut members: HashMap<ActionsFilter, HashSet<ActionId>> = HashMap::new();
    for instance in repo.list_all().await? {
        if let (Some(filter), Some(actions)) = (
            filter_by_kind.get(&instance.kind_id),
            links.remove(&instance.id),
        ) {
            members.entry(*filter).or_default().extend(actions);
        }
    }
    Ok(members)
}

struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    action_id: ActionId,
    _sub: Subscription,
    _favorites: Subscription,
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
    _favorites: Subscription,
}

struct AddTriggerFill {
    action_id: ActionId,
    kind_id: String,
    kind_label: String,
    name_field: Entity<TextInput>,
    fields: Vec<crate::config_form::ConfigField>,
    labels: Vec<SharedString>,
    choices: CollectionChoices,
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
        .key_activated(palette.text_primary)
        .on_click(handler)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use forge_types::{Action, ActionId, ExecutionMode, QueueId};

    use super::{group_actions, tr};

    fn action(name: &str, group: Option<&str>) -> Action {
        Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: group.map(str::to_owned),
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::default(),
            description: None,
            sub_actions: Vec::new(),
        }
    }

    fn shape(actions: Vec<Action>) -> Vec<(String, Vec<String>)> {
        group_actions(actions)
            .into_iter()
            .map(|group| {
                (
                    group.name.to_string(),
                    group.actions.into_iter().map(|a| a.name).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn groups_merge_regardless_of_case_or_surrounding_space_and_sort_their_members() {
        let groups = shape(vec![
            action("zeta", Some(" chat ")),
            action("Alpha", Some("CHAT")),
            action("beta", Some("Chat")),
            action("solo", Some("alerts")),
        ]);

        assert_eq!(
            groups,
            vec![
                ("ALERTS".to_owned(), vec!["solo".to_owned()]),
                (
                    "CHAT".to_owned(),
                    vec!["Alpha".to_owned(), "beta".to_owned(), "zeta".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn a_missing_or_blank_group_lands_in_the_ungrouped_bucket() {
        let groups = shape(vec![
            action("none", None),
            action("empty", Some("")),
            action("space", Some("   ")),
        ]);

        assert_eq!(
            groups,
            vec![(
                tr!("actions_group_ungrouped").to_uppercase(),
                vec!["empty".to_owned(), "none".to_owned(), "space".to_owned()]
            )]
        );
    }
}
