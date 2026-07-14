//! Actions screen root: the `ScreenActionsView` view-entity, its seeded action
//! and sub-action model, the shared render tokens, and the `Render` dispatcher.
//! The list pane, editor pane and branch drill-in live in the sibling submodules.

use crate::presentation::ActivePresentation;
use crate::toasts::PushToast;
use forge_components::{
    ForgePalette, GridPicker, Icon, OverlayPosition, TextArea, TextInput, ToastKind, icon, overlay,
    search_input,
};
use forge_storage::{ActionRepo, QueueRepo};
use forge_types::{Action, ActionId, QueueId};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

mod branch;
mod editor;
mod list;

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
/// Leading status-dot diameter on a trigger card (fixed 7px).
const TRIGGER_DOT: Pixels = px(7.0);
/// Leading glyph size on trigger / step cards (fixed 13px, off the `FONT_*` scale).
const CARD_GLYPH: Pixels = px(13.0);
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
/// Small glyph size on a section hint / info line (fixed 11px).
const HINT_GLYPH: Pixels = px(11.0);
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

/// Authoring depth ceiling for nested sub-chains. Drilling past this into an
/// *empty* branch is disabled (no new depth is created), while an already-deeper
/// chain stays fully editable at its existing depth. Deliberately small so the
/// breadcrumb stays legible.
const UI_MAX_NESTING_DEPTH: usize = 8;
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
/// folded from [`ActionRepo::list`]: every CRUD op (create / rename / duplicate /
/// enable / delete) writes through the repo then reconciles by a full re-pull, so the
/// tree always holds real persisted [`ActionId`]s, never a view-minted placeholder.
/// The right editor pane is still the seeded prototype — the real detail (real
/// sub-action chain + linked trigger instances) lands in a follow-up slice; `select`
/// keeps building that seeded `ActionDetail` from the real summary so the screen stays
/// runnable.
pub struct ScreenActionsView {
    action_repo: Arc<dyn ActionRepo>,
    queue_repo: Arc<dyn QueueRepo>,
    rt_handle: tokio::runtime::Handle,
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
    /// The unified centred "Add" grid picker, driving both sub-action and trigger adds.
    grid_picker: Option<GridPickerForm>,
    pending_trigger_unlink: Option<usize>,
    /// Drill-in path into the selected action's nested sub-chains. Empty = the
    /// step list renders the action's top-level chain; each frame descends one
    /// composite branch or switch case.
    nav_path: Vec<NavFrame>,
    /// Live match inputs for the switch cases in the *current* chain, keyed by
    /// `(step_index, case_index)`. Rebuilt whenever the current chain changes so a
    /// case's single-value match owns its own edit state (mirrors every other
    /// inline field in the screen). Multi-value imported matches carry no input.
    case_fields: BTreeMap<(usize, usize), CaseField>,
    _search_sub: Subscription,
}

impl ScreenActionsView {
    pub fn new(
        action_repo: Arc<dyn ActionRepo>,
        queue_repo: Arc<dyn QueueRepo>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search actions...", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let view = Self {
            action_repo,
            queue_repo,
            rt_handle,
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
            pending_trigger_unlink: None,
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
    /// selection in sync (refreshing the seeded detail's name/enabled, or clearing it
    /// when the selected action no longer exists).
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
            match self.find(selected).map(|s| (s.name.clone(), s.enabled)) {
                Some((name, enabled)) => {
                    if let Some(detail) = self.detail.as_mut() {
                        detail.name = name;
                        detail.enabled = enabled;
                    }
                }
                None => {
                    self.selected = None;
                    self.detail = None;
                    self.nav_path.clear();
                    self.case_fields.clear();
                }
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
        let unlink_modal = self
            .pending_trigger_unlink
            .map(|index| self.render_trigger_unlink_confirm(index, &palette, cx));

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
            .children(unlink_modal)
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

// ── editor stub state ──────────────────────────────────────────────────────

/// A representative sub-action kind. `forge-desktop` wires no sub-action registry
/// yet, so the editor seeds a fixed set — each carries its own summary shape, config
/// fields and seed values, mirroring what a runner descriptor would expose.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SubKind {
    SendChat,
    Speak,
    PlaySound,
    SetGlobal,
    RandomInt,
    Delay,
    Log,
    ReadFile,
    SubAction,
    IfThenElse,
    Loop,
    Switch,
}

const SUB_KINDS: [SubKind; 12] = [
    SubKind::SendChat,
    SubKind::Speak,
    SubKind::PlaySound,
    SubKind::SetGlobal,
    SubKind::RandomInt,
    SubKind::Delay,
    SubKind::Log,
    SubKind::ReadFile,
    SubKind::SubAction,
    SubKind::IfThenElse,
    SubKind::Loop,
    SubKind::Switch,
];

/// One editable config entry a sub-action kind exposes in the add-sub-action form.
struct SubField {
    key: &'static str,
    label: &'static str,
    placeholder: &'static str,
}

/// The category a [`SubKind`] groups under in the kind picker. The full runtime
/// registry carries more categories; the seeded subset touches only these.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SubCategory {
    Chat,
    Tts,
    Audio,
    Globals,
    Logic,
    Delay,
    Files,
    Util,
}

impl SubCategory {
    fn label(self) -> &'static str {
        match self {
            SubCategory::Chat => "Chat",
            SubCategory::Tts => "Text-to-speech",
            SubCategory::Audio => "Audio",
            SubCategory::Globals => "Globals",
            SubCategory::Logic => "Logic",
            SubCategory::Delay => "Delay",
            SubCategory::Files => "Files",
            SubCategory::Util => "Utilities",
        }
    }

    fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            SubCategory::Chat => palette.brand,
            SubCategory::Tts | SubCategory::Audio => palette.success,
            SubCategory::Globals => palette.warning,
            SubCategory::Files => palette.random,
            SubCategory::Logic | SubCategory::Delay | SubCategory::Util => palette.text_muted,
        }
    }

    /// Stable scope slug used for the grid's scope chips and element ids.
    fn slug(self) -> &'static str {
        match self {
            SubCategory::Chat => "chat",
            SubCategory::Tts => "tts",
            SubCategory::Audio => "audio",
            SubCategory::Globals => "globals",
            SubCategory::Logic => "logic",
            SubCategory::Delay => "delay",
            SubCategory::Files => "files",
            SubCategory::Util => "util",
        }
    }
}

impl SubKind {
    fn label(self) -> &'static str {
        match self {
            SubKind::SendChat => "Send chat message",
            SubKind::Speak => "Speak (TTS)",
            SubKind::PlaySound => "Play sound",
            SubKind::SetGlobal => "Set global",
            SubKind::RandomInt => "Random number",
            SubKind::Delay => "Delay",
            SubKind::Log => "Write log",
            SubKind::ReadFile => "Read file",
            SubKind::SubAction => "Run sub-action",
            SubKind::IfThenElse => "If / Then / Else",
            SubKind::Loop => "Loop",
            SubKind::Switch => "Switch / Case",
        }
    }

    /// Stable slug used to mint the grid card's element id.
    fn slug(self) -> &'static str {
        match self {
            SubKind::SendChat => "send-chat",
            SubKind::Speak => "speak",
            SubKind::PlaySound => "play-sound",
            SubKind::SetGlobal => "set-global",
            SubKind::RandomInt => "random-int",
            SubKind::Delay => "delay",
            SubKind::Log => "log",
            SubKind::ReadFile => "read-file",
            SubKind::SubAction => "sub-action",
            SubKind::IfThenElse => "if-then-else",
            SubKind::Loop => "loop",
            SubKind::Switch => "switch",
        }
    }

    fn summary_hint(self) -> &'static str {
        match self {
            SubKind::SendChat => "Post a message to chat",
            SubKind::Speak => "Read text aloud through the TTS queue",
            SubKind::PlaySound => "Play a soundboard clip",
            SubKind::SetGlobal => "Assign a value to a global variable",
            SubKind::RandomInt => "Roll a random integer into a variable",
            SubKind::Delay => "Pause the chain for a fixed time",
            SubKind::Log => "Write a line to the log",
            SubKind::ReadFile => "Read a file into a variable",
            SubKind::SubAction => "Run another action inline",
            SubKind::IfThenElse => "Run one of two sub-chains depending on a condition",
            SubKind::Loop => {
                "Repeat a sub-chain a fixed count, over an array, or while a condition holds"
            }
            SubKind::Switch => "Run the sub-chain whose case matches an expression",
        }
    }

    /// Leading step-card glyph. `Read file` degrades to a document glyph and `Random
    /// number` to a diamond — the kit ships no file/dice tabler icon.
    fn glyph(self) -> Icon {
        match self {
            SubKind::SendChat => Icon::Send,
            SubKind::Speak => Icon::Volume,
            SubKind::PlaySound => Icon::Music,
            SubKind::SetGlobal => Icon::Variable,
            SubKind::RandomInt => Icon::Diamond,
            SubKind::Delay => Icon::Clock,
            SubKind::Log => Icon::InfoCircle,
            SubKind::ReadFile => Icon::FileCode,
            SubKind::SubAction => Icon::Bolt,
            // The kit ships no git-branch / list-checks glyph, so the composite kinds
            // degrade to the nearest tabler icon (Loop maps exactly to Repeat).
            SubKind::IfThenElse => Icon::TargetArrow,
            SubKind::Loop => Icon::Repeat,
            SubKind::Switch => Icon::Notebook,
        }
    }

    fn category(self) -> SubCategory {
        match self {
            SubKind::SendChat => SubCategory::Chat,
            SubKind::Speak => SubCategory::Tts,
            SubKind::PlaySound => SubCategory::Audio,
            SubKind::SetGlobal => SubCategory::Globals,
            SubKind::RandomInt
            | SubKind::SubAction
            | SubKind::IfThenElse
            | SubKind::Loop
            | SubKind::Switch => SubCategory::Logic,
            SubKind::Delay => SubCategory::Delay,
            SubKind::Log => SubCategory::Util,
            SubKind::ReadFile => SubCategory::Files,
        }
    }

    /// Config keys holding a single nested sub-chain, in the order they seed a fresh
    /// step's `branches`.
    fn default_branch_keys(self) -> &'static [&'static str] {
        match self {
            SubKind::IfThenElse => &["then_chain", "else_chain"],
            SubKind::Loop => &["body"],
            SubKind::Switch => &["default_chain"],
            _ => &[],
        }
    }

    fn has_cases(self) -> bool {
        matches!(self, SubKind::Switch)
    }

    /// The branch affordances a composite kind renders under its step card, ordered to
    /// match the runtime editor (a switch shows its case list before the default chip).
    fn branch_specs(self) -> &'static [BranchSpec] {
        match self {
            SubKind::IfThenElse => &[
                BranchSpec::Chain {
                    key: "then_chain",
                    label: "Then",
                },
                BranchSpec::Chain {
                    key: "else_chain",
                    label: "Else",
                },
            ],
            SubKind::Loop => &[BranchSpec::Chain {
                key: "body",
                label: "Body",
            }],
            SubKind::Switch => &[
                BranchSpec::Cases {
                    key: "cases",
                    label: "Cases",
                },
                BranchSpec::Chain {
                    key: "default_chain",
                    label: "Default",
                },
            ],
            _ => &[],
        }
    }

    fn fields(self) -> &'static [SubField] {
        match self {
            SubKind::SendChat => &[
                SubField {
                    key: "target",
                    label: "TARGET",
                    placeholder: "twitch",
                },
                SubField {
                    key: "message",
                    label: "MESSAGE",
                    placeholder: "Message to send",
                },
            ],
            SubKind::Speak => &[SubField {
                key: "text",
                label: "TEXT",
                placeholder: "Text to speak",
            }],
            SubKind::PlaySound => &[SubField {
                key: "clip_id",
                label: "CLIP",
                placeholder: "Clip name",
            }],
            SubKind::SetGlobal => &[
                SubField {
                    key: "name",
                    label: "NAME",
                    placeholder: "Variable name",
                },
                SubField {
                    key: "value",
                    label: "VALUE",
                    placeholder: "Value",
                },
            ],
            SubKind::RandomInt => &[
                SubField {
                    key: "min",
                    label: "MIN",
                    placeholder: "0",
                },
                SubField {
                    key: "max",
                    label: "MAX",
                    placeholder: "100",
                },
                SubField {
                    key: "target_var",
                    label: "TARGET VARIABLE",
                    placeholder: "roll",
                },
            ],
            SubKind::Delay => &[SubField {
                key: "ms",
                label: "MILLISECONDS",
                placeholder: "1000",
            }],
            SubKind::Log => &[
                SubField {
                    key: "level",
                    label: "LEVEL",
                    placeholder: "info",
                },
                SubField {
                    key: "message",
                    label: "MESSAGE",
                    placeholder: "Log line",
                },
            ],
            SubKind::ReadFile => &[
                SubField {
                    key: "path",
                    label: "PATH",
                    placeholder: "~/file.txt",
                },
                SubField {
                    key: "target_var",
                    label: "TARGET VARIABLE",
                    placeholder: "lines",
                },
            ],
            SubKind::SubAction => &[SubField {
                key: "action_id",
                label: "ACTION",
                placeholder: "!other",
            }],
            SubKind::IfThenElse => &[SubField {
                key: "condition",
                label: "CONDITION",
                placeholder: "%value% > 0",
            }],
            SubKind::Loop => &[SubField {
                key: "count",
                label: "COUNT",
                placeholder: "3",
            }],
            SubKind::Switch => &[SubField {
                key: "expression",
                label: "EXPRESSION",
                placeholder: "%value%",
            }],
        }
    }

    fn seed_config(self) -> BTreeMap<String, String> {
        let pairs: &[(&str, &str)] = match self {
            SubKind::SendChat => &[("target", "twitch"), ("message", "Go follow %user%!")],
            SubKind::Speak => &[("text", "Shoutout to %user%")],
            SubKind::PlaySound => &[("clip_id", "airhorn")],
            SubKind::SetGlobal => &[("name", "so_count"), ("value", "%so_count%")],
            SubKind::RandomInt => &[("min", "1"), ("max", "100"), ("target_var", "roll")],
            SubKind::Delay => &[("ms", "1500")],
            SubKind::Log => &[("level", "info"), ("message", "shoutout done for %user%")],
            SubKind::ReadFile => &[("path", "~/quotes.txt"), ("target_var", "lines")],
            SubKind::SubAction => &[("action_id", "!quote")],
            SubKind::IfThenElse => &[("condition", "%followage% > 30")],
            SubKind::Loop => &[("count", "3")],
            SubKind::Switch => &[("expression", "%tier%")],
        };
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
}

/// A single sub-action step in the editor chain: its kind plus a string-keyed config
/// bag the summary line and the edit form read. Composite kinds also carry their
/// nested `branches` (single sub-chains keyed then / else / body / default) and, for a
/// switch, an ordered `cases` list — modelled directly on the step rather than as
/// encoded config blobs, since `forge-desktop` seeds the chain in memory.
#[derive(Clone)]
struct EditorStep {
    kind: SubKind,
    config: BTreeMap<String, String>,
    branches: Vec<SubChain>,
    cases: Option<Vec<SwitchCase>>,
}

/// One named nested sub-chain a composite step holds (e.g. `then_chain` → "Then").
#[derive(Clone)]
struct SubChain {
    key: &'static str,
    steps: Vec<EditorStep>,
}

/// One switch case: a match value paired with its own nested chain.
#[derive(Clone)]
struct SwitchCase {
    match_value: CaseMatch,
    chain: Vec<EditorStep>,
}

/// A switch case's match. A single authored value is editable; an imported
/// multi-value match stays read-only per the single-value authoring contract.
#[derive(Clone)]
enum CaseMatch {
    Single(String),
    Multi,
}

/// Names one branch affordance a composite kind exposes, in render order — mirroring
/// the runtime editor's field ordering (a switch shows its case list before its
/// `default_chain` chip).
enum BranchSpec {
    Chain {
        key: &'static str,
        label: &'static str,
    },
    Cases {
        key: &'static str,
        label: &'static str,
    },
}

/// A live match input for one switch case, plus the subscription routing its submit
/// back into the model.
struct CaseField {
    field: Entity<TextInput>,
    _sub: Subscription,
}

/// One drill-in frame: the parent step's index in the chain it was reached from, the
/// branch key it descends (unused for cases, which resolve through `cases`), and —
/// for a switch — which case row's chain was entered.
#[derive(Clone, Copy)]
struct NavFrame {
    step_index: usize,
    chain_key: &'static str,
    case_index: Option<usize>,
}

/// Resolves a nav path to the chain it points at, starting from an action's top-level
/// steps. An unresolvable frame yields `None` (never a panic).
fn resolve_chain<'a>(root: &'a [EditorStep], path: &[NavFrame]) -> Option<&'a [EditorStep]> {
    let mut current = root;
    for frame in path {
        let step = current.get(frame.step_index)?;
        current = match frame.case_index {
            None => step
                .branches
                .iter()
                .find(|b| b.key == frame.chain_key)?
                .steps
                .as_slice(),
            Some(ci) => step.cases.as_ref()?.get(ci)?.chain.as_slice(),
        };
    }
    Some(current)
}

/// Mutable twin of [`resolve_chain`].
fn resolve_chain_mut<'a>(
    root: &'a mut Vec<EditorStep>,
    path: &[NavFrame],
) -> Option<&'a mut Vec<EditorStep>> {
    let mut current = root;
    for frame in path {
        let step = current.get_mut(frame.step_index)?;
        current = match frame.case_index {
            None => {
                &mut step
                    .branches
                    .iter_mut()
                    .find(|b| b.key == frame.chain_key)?
                    .steps
            }
            Some(ci) => &mut step.cases.as_mut()?.get_mut(ci)?.chain,
        };
    }
    Some(current)
}

impl EditorStep {
    /// Builds a step, seeding its nested branch structure from the kind so a composite
    /// added via the picker starts with empty (drillable) branches and a switch with an
    /// empty case list.
    fn new(kind: SubKind, config: BTreeMap<String, String>) -> Self {
        let branches = kind
            .default_branch_keys()
            .iter()
            .map(|key| SubChain {
                key,
                steps: Vec::new(),
            })
            .collect();
        let cases = kind.has_cases().then(Vec::new);
        Self {
            kind,
            config,
            branches,
            cases,
        }
    }

    fn get(&self, key: &str) -> &str {
        self.config.get(key).map(String::as_str).unwrap_or("")
    }

    /// The summary line under a step's title. Interpolation markers stay as literal
    /// `%name%` so [`variable_text`] can two-tone them.
    fn detail(&self) -> String {
        match self.kind {
            SubKind::SendChat => {
                let target = if self.get("target").is_empty() {
                    "twitch"
                } else {
                    self.get("target")
                };
                format!("\u{2192} {target}: \"{}\"", self.get("message"))
            }
            SubKind::Speak => self.get("text").to_owned(),
            SubKind::PlaySound => self.get("clip_id").to_owned(),
            SubKind::SetGlobal => format!("{} = \"{}\"", self.get("name"), self.get("value")),
            SubKind::RandomInt => {
                let min = if self.get("min").is_empty() {
                    "0"
                } else {
                    self.get("min")
                };
                let max = if self.get("max").is_empty() {
                    "0"
                } else {
                    self.get("max")
                };
                format!("[{min}..{max}] \u{2192} %{}%", self.get("target_var"))
            }
            SubKind::Delay => {
                let ms = if self.get("ms").is_empty() {
                    "0"
                } else {
                    self.get("ms")
                };
                format!("{ms} ms")
            }
            SubKind::Log => {
                let level = if self.get("level").is_empty() {
                    "info"
                } else {
                    self.get("level")
                };
                format!("[{level}] \"{}\"", self.get("message"))
            }
            SubKind::ReadFile => {
                format!("{} \u{2192} %{}%", self.get("path"), self.get("target_var"))
            }
            SubKind::SubAction => self.get("action_id").to_owned(),
            SubKind::IfThenElse => format!("if {}", self.get("condition")),
            SubKind::Loop => {
                let count = if self.get("count").is_empty() {
                    "0"
                } else {
                    self.get("count")
                };
                format!("repeat {count}\u{00d7}")
            }
            SubKind::Switch => format!("switch {}", self.get("expression")),
        }
    }
}

/// A trigger link shown under the editor's TRIGGERS section.
struct SeededTrigger {
    name: String,
    kind_label: String,
    condition: String,
    glyph: Icon,
    enabled: bool,
}

// ── trigger picker stub state ──────────────────────────────────────────────

/// A top-level platform grouping in the two-level trigger picker. The real screen
/// derives this from a trigger's `kind_id` prefix over the registry; here the seeded
/// entries carry `kind_id`s that map through [`platform_group_for`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlatformGroup {
    Twitch,
    YouTube,
    Kick,
    Obs,
    VTube,
    Midi,
    Hotkey,
    Discord,
    Script,
    Core,
}

impl PlatformGroup {
    fn label(self) -> &'static str {
        match self {
            PlatformGroup::Twitch => "Twitch",
            PlatformGroup::YouTube => "YouTube",
            PlatformGroup::Kick => "Kick",
            PlatformGroup::Obs => "OBS",
            PlatformGroup::VTube => "VTube Studio",
            PlatformGroup::Midi => "MIDI",
            PlatformGroup::Hotkey => "Hotkey",
            PlatformGroup::Discord => "Discord",
            PlatformGroup::Script => "Script",
            PlatformGroup::Core => "Core",
        }
    }

    /// Stable slug used to mint gpui element ids for the platform rows.
    fn key(self) -> &'static str {
        match self {
            PlatformGroup::Twitch => "twitch",
            PlatformGroup::YouTube => "youtube",
            PlatformGroup::Kick => "kick",
            PlatformGroup::Obs => "obs",
            PlatformGroup::VTube => "vtube",
            PlatformGroup::Midi => "midi",
            PlatformGroup::Hotkey => "hotkey",
            PlatformGroup::Discord => "discord",
            PlatformGroup::Script => "script",
            PlatformGroup::Core => "core",
        }
    }

    fn color(self, palette: &ForgePalette) -> Rgba {
        match self {
            PlatformGroup::Twitch => palette.brand,
            PlatformGroup::YouTube => palette.platform_youtube,
            PlatformGroup::Kick => palette.platform_kick,
            PlatformGroup::Obs => palette.text_secondary,
            PlatformGroup::VTube => palette.accent_teal,
            PlatformGroup::Midi => palette.random,
            PlatformGroup::Hotkey => palette.warning,
            PlatformGroup::Discord => palette.info,
            PlatformGroup::Script => palette.warning,
            PlatformGroup::Core => palette.info,
        }
    }

    /// Leading glyph on a linked trigger card, standing in for the descriptor icon the
    /// real registry supplies.
    fn glyph(self) -> Icon {
        match self {
            PlatformGroup::Twitch
            | PlatformGroup::YouTube
            | PlatformGroup::Kick
            | PlatformGroup::Discord => Icon::MessageCircle,
            PlatformGroup::Obs | PlatformGroup::VTube => Icon::Bolt,
            PlatformGroup::Midi | PlatformGroup::Hotkey => Icon::Bolt,
            PlatformGroup::Script => Icon::Variable,
            PlatformGroup::Core => Icon::Clock,
        }
    }
}

/// One selectable trigger in the grid: a default instance plus any user-defined
/// custom instances.
struct PickerEntry {
    kind_id: &'static str,
    label: &'static str,
    desc: &'static str,
    sub_group: String,
    default_id: u64,
    customs: Vec<PickerCustom>,
}

/// A user-defined custom instance card nested under its [`PickerEntry`].
struct PickerCustom {
    id: u64,
    name: &'static str,
    override_summary: &'static str,
    enabled: bool,
}

// ── unified grid picker data ────────────────────────────────────────────────

/// Which add flow the unified grid picker drives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Step,
    Trigger,
}

/// The open unified "Add" grid picker: the shared [`GridPicker`] entity, a lookup from
/// each card id to what picking it applies, the action it targets (guarding a trigger
/// link against a stale selection), and the subscription draining its events.
struct GridPickerForm {
    picker: Entity<GridPicker>,
    picks: HashMap<SharedString, GridPick>,
    action_id: ActionId,
    _sub: Subscription,
}

/// What picking a grid card applies to the open action.
#[derive(Clone)]
enum GridPick {
    Step(SubKind),
    Trigger(TriggerSeed),
}

/// The values needed to mint a [`SeededTrigger`] when a trigger card is picked.
#[derive(Clone)]
struct TriggerSeed {
    name: String,
    kind_label: String,
    condition: String,
    glyph: Icon,
    enabled: bool,
}

/// The loaded editor payload for the selected action — seeded locally until the
/// actions repo is wired over the runtime→UI bridge.
struct ActionDetail {
    action_id: ActionId,
    name: String,
    enabled: bool,
    description: Option<String>,
    steps: Vec<EditorStep>,
    triggers: Vec<SeededTrigger>,
}

/// The open edit-sub-action side sheet. Its config inputs are child [`TextInput`]
/// entities owning their own edit state; the kind and the edited step index are plain
/// fields. Adding a step no longer routes through here — the unified grid picker
/// appends with smart defaults, and this sheet only re-authors an existing step.
struct EditSubActionForm {
    kind: SubKind,
    fields: Vec<(&'static SubField, Entity<TextInput>)>,
    index: usize,
}

/// A leaf (non-composite) step seeded from its kind's default config.
fn leaf_step(kind: SubKind) -> EditorStep {
    EditorStep::new(kind, kind.seed_config())
}

/// A populated if/then-else: the then-chain nests a loop (so drilling reaches three
/// breadcrumb segments), the else-chain holds one leaf.
fn seed_if_then_else_step() -> EditorStep {
    let mut step = EditorStep::new(SubKind::IfThenElse, SubKind::IfThenElse.seed_config());
    for branch in &mut step.branches {
        match branch.key {
            "then_chain" => {
                let mut loop_step = EditorStep::new(SubKind::Loop, SubKind::Loop.seed_config());
                for body in &mut loop_step.branches {
                    if body.key == "body" {
                        body.steps = vec![leaf_step(SubKind::Speak)];
                    }
                }
                branch.steps = vec![leaf_step(SubKind::SendChat), loop_step];
            }
            "else_chain" => branch.steps = vec![leaf_step(SubKind::Log)],
            _ => {}
        }
    }
    step
}

/// A populated switch: three cases (one an imported read-only multi-value match) plus
/// a default chain.
fn seed_switch_step() -> EditorStep {
    let mut step = EditorStep::new(SubKind::Switch, SubKind::Switch.seed_config());
    step.cases = Some(vec![
        SwitchCase {
            match_value: CaseMatch::Single("1000".to_owned()),
            chain: vec![leaf_step(SubKind::SendChat)],
        },
        SwitchCase {
            match_value: CaseMatch::Single("3000".to_owned()),
            chain: vec![leaf_step(SubKind::Speak)],
        },
        SwitchCase {
            match_value: CaseMatch::Multi,
            chain: vec![leaf_step(SubKind::PlaySound)],
        },
    ]);
    for branch in &mut step.branches {
        if branch.key == "default_chain" {
            branch.steps = vec![leaf_step(SubKind::Log)];
        }
    }
    step
}

/// Seeds an [`ActionDetail`] from a cached tree summary: a `sub_action_count`-long
/// chain cycling representative kinds, one or two trigger links, and a description on
/// the busier actions.
fn build_detail(summary: &ActionSummary) -> ActionDetail {
    const ORDER: [SubKind; 9] = [
        SubKind::SendChat,
        SubKind::SetGlobal,
        SubKind::Delay,
        SubKind::PlaySound,
        SubKind::Speak,
        SubKind::RandomInt,
        SubKind::Log,
        SubKind::ReadFile,
        SubKind::SubAction,
    ];
    let mut steps: Vec<EditorStep> = (0..summary.sub_action_count)
        .map(|i| {
            let kind = ORDER[i % ORDER.len()];
            EditorStep::new(kind, kind.seed_config())
        })
        .collect();

    // The busiest seeded action grows two composite steps so branch drill-in (down to
    // a nested loop body — three breadcrumb segments), switch-case editing, and the
    // read-only multi-value match are all exercisable without a runtime registry.
    if summary.sub_action_count >= 7 {
        steps.push(seed_if_then_else_step());
        steps.push(seed_switch_step());
    }

    let mut triggers = vec![SeededTrigger {
        name: format!("Command {}", summary.name),
        kind_label: "Chat command".to_owned(),
        condition: summary.name.clone(),
        glyph: Icon::MessageCircle,
        enabled: summary.enabled,
    }];
    if summary.sub_action_count >= 5 {
        triggers.push(SeededTrigger {
            name: "Channel reward".to_owned(),
            kind_label: "Channel points".to_owned(),
            condition: "cost: 500".to_owned(),
            glyph: Icon::Star,
            enabled: true,
        });
    }

    let description = (summary.sub_action_count >= 7)
        .then(|| "Shout out a chatter or raider and celebrate with a sound.".to_owned());

    ActionDetail {
        action_id: summary.id,
        name: summary.name.clone(),
        enabled: summary.enabled,
        description,
        steps,
        triggers,
    }
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
