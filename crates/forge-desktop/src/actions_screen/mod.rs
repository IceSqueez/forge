//! Actions screen root: the `ScreenActionsView` view-entity, its cached roster
//! model, the shared render tokens, and the `Render` dispatcher. The list pane,
//! editor pane, branch drill-in and nested-chain navigation live in the sibling
//! submodules.

use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    ForgePalette, GridPicker, Icon, OverlayPosition, TextArea, TextInput, ToastKind, icon, overlay,
    search_input,
};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::EventBus;
use forge_runtime::actions::{ActionDetail, ActionsService};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Action, ActionId, QueueId, SubActionStep, TriggerInstanceId};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, Pixels, SharedString, Subscription,
    Window, div, prelude::*, px,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

mod branch;
mod editor;
mod list;
mod nav;
mod test_trigger;

/// Left tree panel width. The parity source pins it at a fixed 290px, off the
/// `Spacing` scale, so it is carried as a literal.
const LEFT_PANEL_W: Pixels = px(290.0);
/// Action-row height. Pinned so the selection stripe, the row wash and the
/// overflow-menu trigger share one clean 30px bar regardless of per-widget height.
const ROW_HEIGHT: Pixels = px(30.0);
/// Right-slot width the sub-action count and the `⋮` menu trigger share, so
/// swapping one for the other on hover never shifts the right edge (wide enough for
/// a two-digit "NN sub" count). Fixed 46px in the source.
const RIGHT_SLOT_W: Pixels = px(46.0);
/// Left indent to the state icon / name column inside a tree row (fixed 32px).
const ROW_INDENT: Pixels = px(32.0);
/// Right gutter sitting outside the 46px right slot, matching the group header's
/// 14px count gutter so both right edges align.
const ROW_GUTTER: Pixels = px(14.0);
/// Selection stripe width down a row's leading edge (fixed 2px).
const STRIPE_W: Pixels = px(2.0);
/// Group-header and tree-row leading padding gutter (fixed 14px).
const TREE_GUTTER: Pixels = px(14.0);
/// State-icon / chevron glyph size in the tree (fixed 11px, off the `FONT_*` scale).
const TREE_GLYPH: Pixels = px(11.0);
/// Search field width in the page header (fixed 180px in the source).
const SEARCH_W: Pixels = px(180.0);
/// Header divider bar between the filter chips and the search field.
const HEADER_DIV_W: Pixels = px(0.5);
const HEADER_DIV_H: Pixels = px(16.0);
/// Leading brand dot inside the group field frame (fixed 8px disc).
const GROUP_DOT: Pixels = px(8.0);
/// The empty-right-pane placeholder glyph.
const EMPTY_GLYPH: Pixels = px(28.0);
/// Name character cap surfaced by the modal's `N/64` counter.
const NAME_LIMIT: usize = 64;

/// Enabled/disabled pill corner (fixed 8px, off the `Radius` scale) and its
/// leading status-dot diameter (fixed 5px).
const PILL_RADIUS: Pixels = px(8.0);
const PILL_DOT: Pixels = px(5.0);
/// Leading glyph size on a step card (fixed 13px, off the `FONT_*` scale).
const CARD_GLYPH: Pixels = px(13.0);
/// Leading status-dot diameter (7px) and kind-glyph size (13px) on a trigger card.
const TRIGGER_DOT: Pixels = px(7.0);
const TRIGGER_GLYPH: Pixels = px(13.0);
/// Trailing unlink-`X` glyph size on a linked trigger card (fixed 13px, matching the
/// parity source's row-icon affordance).
const UNLINK_GLYPH: Pixels = px(13.0);
/// Numbered step-circle side and its fully-rounded corner (fixed 22px / 11px).
const STEP_CIRCLE: Pixels = px(22.0);
const STEP_CIRCLE_RADIUS: Pixels = px(11.0);
/// Width of the step column carrying the circle + connector (fixed 24px).
const STEP_COL_W: Pixels = px(24.0);
/// Connector rule width / height between consecutive step circles (fixed 2px / 14px).
const STEP_CONNECTOR_W: Pixels = px(2.0);
const STEP_CONNECTOR_H: Pixels = px(14.0);
/// Square side / glyph size / corner of a step-control icon button (22px / 12px / 4px).
const STEP_BTN: Pixels = px(22.0);
const STEP_BTN_GLYPH: Pixels = px(12.0);
const STEP_BTN_RADIUS: Pixels = px(4.0);
/// Card inner padding (fixed 9px vertical / 12px horizontal in the source).
const CARD_PAD_V: Pixels = px(9.0);
const CARD_PAD_H: Pixels = px(12.0);
/// Empty-placeholder card padding (fixed 18px vertical / 12px horizontal) + glyph 16px,
/// framed with a 0.5px hairline.
const EMPTY_CARD_PAD_V: Pixels = px(18.0);
const EMPTY_CARD_PAD_H: Pixels = px(12.0);
const EMPTY_CARD_GLYPH: Pixels = px(16.0);
const HALF_BORDER: Pixels = px(0.5);
/// Detail pane outer padding (fixed 18px vertical / 22px horizontal).
const PANE_PAD_V: Pixels = px(18.0);
const PANE_PAD_H: Pixels = px(22.0);
/// Bottom gap under a non-final step block (fixed 6px).
const STEP_GAP: Pixels = px(6.0);
/// Right side-sheet width for the edit-sub-action panel (fixed 480px seed in the source).
const SUB_SHEET_W: Pixels = px(480.0);

/// Drill-in chip corner (fixed 6px, off the `Radius` scale) and its leading-edge
/// hairline width (0.5px, shared with [`HALF_BORDER`]).
const CHIP_RADIUS: Pixels = px(6.0);
/// Branch-affordance glyph size — the drill-chip chevron and the add-case plus
/// (fixed 11px, off the `FONT_*` scale).
const BRANCH_GLYPH: Pixels = px(11.0);
/// Single-value switch-case match input width (fixed 160px in the source).
const CASE_MATCH_W: Pixels = px(160.0);

/// The category a group filters under. `Other` shows only under the `All` filter —
/// matching the design's `groupTypeMap` (chat / timers / points, everything else
/// unmapped).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionCategory {
    Chat,
    Timers,
    Points,
    Other,
}

/// The page's top filter tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionsFilter {
    All,
    Chat,
    Timers,
    Points,
}

/// A cached action summary — the tree row's payload, folded from a persisted
/// [`Action`] on each `list` pull. The storage provider is the source of truth; the
/// roster reconciles by a full re-pull after every write, never a local patch.
struct ActionSummary {
    id: ActionId,
    name: String,
    enabled: bool,
    sub_action_count: usize,
}

/// A named, collapsible group of actions.
struct ActionGroup {
    name: SharedString,
    category: ActionCategory,
    collapsed: bool,
    actions: Vec<ActionSummary>,
}

/// An in-progress inline rename: the target action plus the field entity holding the
/// draft name and the subscription routing its submit/cancel back to the view.
struct Renaming {
    id: ActionId,
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// The open New-Action modal. The text fields are child [`TextInput`] / [`TextArea`]
/// entities that own their own edit state; the toggles and the selected queue are
/// plain fields. Submit is gated on a non-empty name.
struct AddActionForm {
    name: Entity<TextInput>,
    group: Entity<TextInput>,
    description: Entity<TextArea>,
    /// The real queues an action can be filed under, `(id, name)` pairs pulled off the
    /// queue repo when the modal opens. Empty until that pull lands (the QUEUE section
    /// shows a loading caption and Create stays disabled meanwhile) — a new action must
    /// carry a real [`QueueId`], never a fabricated one.
    queues: Vec<(QueueId, SharedString)>,
    selected_queue: usize,
    enabled: bool,
    concurrent: bool,
    bypass_pause: bool,
    random_pick: bool,
    /// Repaints the modal (its `N/64` counter and the Create-button gate) as the name
    /// field changes.
    _name_sub: Subscription,
}

/// The Actions screen view-entity: a page header (breadcrumb, filter chips, search
/// and a New-action button), a fixed-width collapsible action tree on the left, and
/// the action editor pane on the right.
///
/// Owns its tree, selection and interaction state. The roster is a cached read
/// folded from [`ActionRepo::list`] and the editor detail a cached read from
/// [`ActionsService::load_detail`]: every CRUD op (create / rename / duplicate /
/// enable / delete) and every chain edit writes through the repo then reconciles by
/// a full re-pull, so the tree and editor always hold real persisted state, never a
/// view-minted placeholder.
pub struct ScreenActionsView {
    action_repo: Arc<dyn ActionRepo>,
    queue_repo: Arc<dyn QueueRepo>,
    actions_service: Arc<ActionsService>,
    sub_action_registry: Arc<SubActionRegistry>,
    trigger_registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
    /// The runtime event bus, used to inject a synthesized event through the
    /// re-entrant store-then-replay path when the editor's "Test run" fires.
    bus: Arc<EventBus>,
    /// True until the first `list` pull lands, so the tree shows a loading caption
    /// rather than the empty-roster caption before any row arrives.
    loading: bool,
    groups: Vec<ActionGroup>,
    filter: ActionsFilter,
    search: String,
    search_field: Entity<TextInput>,
    selected: Option<ActionId>,
    hovered: Option<ActionId>,
    menu_open: Option<ActionId>,
    renaming: Option<Renaming>,
    add_modal: Option<AddActionForm>,
    pending_delete: Option<ActionId>,
    detail: Option<ActionDetail>,
    sub_form: Option<EditSubActionForm>,
    step_menu_open: Option<usize>,
    grid_picker: Option<GridPickerForm>,
    add_trigger: Option<AddTriggerForm>,
    /// Drill-in path into the selected action's nested sub-chains. Empty = the
    /// step list renders the action's top-level chain; each frame descends one
    /// composite branch or switch case.
    nav_path: Vec<nav::NavFrame>,
    /// Live match inputs for the switch cases in the *current* chain, keyed by
    /// `(step_index, case_index)`. Rebuilt whenever the current chain changes so a
    /// case's single-value match owns its own edit state. Multi-value imported
    /// matches carry no input.
    case_fields: BTreeMap<(usize, usize), CaseField>,
    _search_sub: Subscription,
}

impl ScreenActionsView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        action_repo: Arc<dyn ActionRepo>,
        queue_repo: Arc<dyn QueueRepo>,
        actions_service: Arc<ActionsService>,
        sub_action_registry: Arc<SubActionRegistry>,
        trigger_registry: Arc<TriggerRegistry>,
        rt_handle: tokio::runtime::Handle,
        bus: Arc<EventBus>,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search actions...", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let view = Self {
            action_repo,
            queue_repo,
            actions_service,
            sub_action_registry,
            trigger_registry,
            rt_handle,
            bus,
            loading: true,
            groups: Vec::new(),
            filter: ActionsFilter::All,
            search: String::new(),
            search_field,
            selected: None,
            hovered: None,
            menu_open: None,
            renaming: None,
            add_modal: None,
            pending_delete: None,
            detail: None,
            sub_form: None,
            step_menu_open: None,
            grid_picker: None,
            add_trigger: None,
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            _search_sub: search_sub,
        };
        view.reload(cx);
        view
    }

    // --- async pull + reconcile -------------------------------------------

    /// Pulls the full roster off the storage provider and reconciles the cached tree
    /// with it. Every create/rename/enable/duplicate/archive routes back here for a
    /// full re-pull rather than patching a row locally, so the tree always mirrors the
    /// persisted actions.
    fn reload(&self, cx: &mut Context<Self>) {
        let repo = Arc::clone(&self.action_repo);
        self.spawn_reload(
            async move { repo.list().await.map_err(|e| e.to_string()) },
            cx,
        );
    }

    /// Spawns `work` (a repo verb that ends by returning the fresh `list`) on the tokio
    /// runtime, then folds the result back on the foreground executor: the new roster
    /// on success, a PII-safe error toast on failure. A released view makes the apply a
    /// no-op.
    fn spawn_reload(
        &self,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        Self::reload_entity(cx.entity(), self.rt_handle.clone(), work, cx);
    }

    /// The context-free reload path: usable both from a screen handler and from a toast
    /// action closure (which only has an [`App`] and the view handle). Hops the tokio
    /// runtime for `work`, then applies the outcome to `view`.
    fn reload_entity(
        view: Entity<ScreenActionsView>,
        rt_handle: tokio::runtime::Handle,
        work: impl Future<Output = Result<Vec<Action>, String>> + Send + 'static,
        app: &mut App,
    ) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let _ = tx.send(work.await);
        });
        app.spawn(async move |cx| match rx.await {
            Ok(Ok(actions)) => {
                let _ = view.update(cx, |this, cx| this.apply_actions(actions, cx));
            }
            Ok(Err(message)) => {
                let _ = view.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    /// Reconciles the cached tree with a freshly pulled roster: rebuilds the groups,
    /// carries over each group's collapse state by name, and keeps the current
    /// selection in sync — re-pulling the open editor's detail when the selected
    /// action survives, or clearing the selection when it no longer exists.
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
        cx.push_toast(ToastKind::Error, format!("Actions: {message}"));
        cx.notify();
    }

    // --- editor detail: async pull + chain-edit persist -------------------

    fn reload_detail(&self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected {
            self.load_detail_for(id, cx);
        }
    }

    /// Pulls the selected action's full editor detail (action + linked trigger
    /// instances + per-step averages) off the runtime service and applies it,
    /// guarding on the selection not having moved on while the pull was in flight.
    pub(super) fn load_detail_for(&self, id: ActionId, cx: &mut Context<Self>) {
        let service = Arc::clone(&self.actions_service);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rt_handle.spawn(async move {
            let _ = tx.send(service.load_detail(id).await.map_err(|e| e.to_string()));
        });
        cx.spawn(async move |this, cx| match rx.await {
            Ok(Ok(detail)) => {
                let _ = this.update(cx, |this, cx| this.apply_detail(id, detail, cx));
            }
            Ok(Err(message)) => {
                let _ = this.update(cx, |this, cx| this.on_repo_error(&message, cx));
            }
            Err(_) => {}
        })
        .detach();
    }

    fn apply_detail(&mut self, id: ActionId, detail: ActionDetail, cx: &mut Context<Self>) {
        if self.selected != Some(id) {
            return;
        }
        self.detail = Some(detail);
        self.sync_case_fields(cx);
        cx.notify();
    }

    /// Saves `action`, then reconciles both the roster and (via `apply_actions`)
    /// the open editor detail by a full re-pull. Chain edits route here so the
    /// editor never renders a locally-patched chain.
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

    /// Applies `mutate` to the sub-chain the nav path currently points at inside a
    /// clone of the loaded action, re-serializes up through every parent step, and
    /// persists the whole action. A path that no longer resolves is a no-op.
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

impl Render for ScreenActionsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let header = self.render_header(&palette, cx);
        let tree = self.render_tree(&palette, cx);
        let editor = self.render_editor_pane(&palette, cx);

        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(tree)
            .child(editor);

        let add_modal = self
            .add_modal
            .as_ref()
            .map(|form| self.render_add_modal(form, &palette, cx));
        let delete_modal = self
            .pending_delete
            .map(|id| self.render_delete_confirm(id, &palette, cx));
        let sub_modal = self
            .sub_form
            .as_ref()
            .map(|form| self.render_sub_action_modal(form, &palette, cx));
        let grid_picker = self.grid_picker.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_grid_picker(cx));
                })
                .into_any_element()
        });
        let trigger_grid = self.add_trigger.as_ref().map(|form| {
            let view = cx.entity();
            overlay(form.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("actions-trigger-grid-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.cancel_trigger_picker(cx));
                })
                .into_any_element()
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(body)
            .children(add_modal)
            .children(delete_modal)
            .children(sub_modal)
            .children(grid_picker)
            .children(trigger_grid)
    }
}

// ── roster grouping ────────────────────────────────────────────────────────

/// Folds a persisted roster into the sidebar tree: buckets each action by its
/// (uppercased) group name — an unnamed action falls under `UNGROUPED` — sorts the
/// groups by name and each group's rows by name, and classifies each group into a
/// filter category by name. Group ordering is the `BTreeMap` name order, so the tree
/// is stable across re-pulls.
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

/// Classifies an (uppercased) group name into a filter category, mirroring the
/// design's fixed `groupTypeMap` — every other name (including `UNGROUPED`) is `Other`
/// and so shows only under the `All` filter.
fn category_from_group_name(name: &str) -> ActionCategory {
    match name {
        "CHAT COMMANDS" => ActionCategory::Chat,
        "TIMERS" => ActionCategory::Timers,
        "CHANNEL POINTS" => ActionCategory::Points,
        _ => ActionCategory::Other,
    }
}

// ── editor detail state ─────────────────────────────────────────────────────

/// The open unified "Add sub-action" grid picker: the shared [`GridPicker`]
/// entity, a lookup from each card id to the `kind_id` picking it appends, the
/// action it targets (guarding against a stale selection), and the subscription
/// draining its events.
struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, String>,
    action_id: ActionId,
    _sub: Subscription,
}

/// The open "Add trigger" grid picker: the shared [`GridPicker`] entity, a lookup
/// from each card id to the [`TriggerInstanceId`] it links, the action it targets
/// (guarding against a stale selection), and the subscription draining its events.
struct AddTriggerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, TriggerInstanceId>,
    action_id: ActionId,
    _sub: Subscription,
}

/// The open edit-sub-action side sheet: the edited step's `kind_id` and its index
/// in the current chain, plus the per-field editing surface folded from the
/// runner's `config_fields`.
struct EditSubActionForm {
    kind_id: String,
    index: usize,
    fields: Vec<SubFormField>,
}

/// One row in the edit-sub-action form. `SubChain` / `CaseList` keys render as an
/// inert hint — those sub-chains are authored via drill-in, not this form.
enum SubFormField {
    Input {
        key: String,
        label: String,
        /// Saved as `Variant::Int` (lenient parse — a non-numeric value keeps the
        /// step's prior value) rather than `Variant::String`.
        integer: bool,
        /// Set on the inner member of an `Optional` group; renders and saves only
        /// while the gate toggle (a sibling `Bool` on this key) is on.
        gate: Option<String>,
        input: Entity<TextInput>,
    },
    Bool {
        key: String,
        label: String,
        gate: Option<String>,
        value: bool,
    },
    Hint {
        label: String,
    },
}

/// A live match input for one switch case, plus the subscription routing its
/// submit back into the model.
struct CaseField {
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// A 22px square step-reorder icon button. A disabled button inks `disabled`, drops
/// the hover wash and takes no click.
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
