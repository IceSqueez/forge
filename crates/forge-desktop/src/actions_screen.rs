use std::collections::BTreeMap;

use forge_components::{
    BORDER_ACCENT, BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY,
    DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    InputEvent, MenuPlacement, ModalSize, OverlayPosition, Radius, SheetPosition, Spacing,
    TextArea, TextInput, badge, breadcrumb, chip, confirm_modal, ghost_button_with_icon, icon,
    menu_button, menu_divider, menu_item, modal, overlay, primary_button, primary_button_with_icon,
    radius, row_card, search_input, secondary_button, side_sheet, spacing, status_dot, toggle,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, div, prelude::*, px,
};

use crate::presentation::ActivePresentation;

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

// Unified "Add" grid picker (centred category-grid modal). All literals below are
// pinned to the design's fixed px scale, off the `Spacing` / `Radius` / `FONT_*`
// tokens where the design diverges from them.
/// Card envelope (design 660×600).
const GRID_W: Pixels = px(660.0);
const GRID_H: Pixels = px(600.0);
/// Shared horizontal band inset — every band gutters to 16px in the design.
const GRID_BAND_PAD_H: Pixels = px(16.0);
/// Header icon tile: 30px side, 7px corner, 15px glyph.
const GRID_TILE: Pixels = px(30.0);
const GRID_TILE_RADIUS: Pixels = px(7.0);
const GRID_TILE_ICON: Pixels = px(15.0);
/// Header row gap and vertical pad, plus the close glyph size.
const GRID_HEADER_GAP: Pixels = px(11.0);
const GRID_HEADER_PAD_V: Pixels = px(13.0);
const GRID_CLOSE_ICON: Pixels = px(15.0);
/// Search band top/bottom pad, the box corner, the leading glyph and the input font.
const GRID_SEARCH_PAD_T: Pixels = px(11.0);
const GRID_SEARCH_PAD_B: Pixels = px(9.0);
const GRID_SEARCH_ICON: Pixels = px(14.0);
const GRID_SEARCH_FS: Pixels = px(13.0);
/// Scope-chip band vertical pad, chip pad and its leading category dot.
const GRID_CHIPS_PAD_V: Pixels = px(9.0);
const GRID_CHIP_PAD_V: Pixels = px(4.0);
const GRID_CHIP_PAD_H: Pixels = px(10.0);
const GRID_CHIP_DOT: Pixels = px(5.0);
/// Grid body vertical pad, inter-group gap, and the group-header label / dot.
const GRID_BODY_PAD_V: Pixels = px(13.0);
const GRID_GROUP_GAP: Pixels = px(14.0);
const GRID_GROUP_HEADER_MB: Pixels = px(8.0);
const GRID_GROUP_FS: Pixels = px(9.5);
const GRID_GROUP_DOT: Pixels = px(5.0);
/// Card row / pair gap.
const GRID_CARD_GAP: Pixels = px(8.0);
/// Card padding, its leading tile (26px / 7px corner / 13px glyph) and name font.
const GRID_CARD_PAD_V: Pixels = px(11.0);
const GRID_CARD_PAD_H: Pixels = px(12.0);
const GRID_CARD_TILE: Pixels = px(26.0);
const GRID_CARD_TILE_RADIUS: Pixels = px(7.0);
const GRID_CARD_ICON: Pixels = px(13.0);
const GRID_CARD_NAME_FS: Pixels = px(12.5);
const GRID_CARD_ROW_MB: Pixels = px(6.0);
/// Meta font shared by subtitle / chips / match-count / card desc (design 11px).
const GRID_META_FS: Pixels = px(11.0);
/// Footer vertical pad and the `Esc` kbd chip (pad 1/5, 3px corner).
const GRID_FOOTER_PAD_V: Pixels = px(8.0);
const GRID_KBD_PAD_V: Pixels = px(1.0);
const GRID_KBD_PAD_H: Pixels = px(5.0);
const GRID_KBD_RADIUS: Pixels = px(3.0);
/// Empty-state vertical pad and its glyph.
const GRID_EMPTY_PAD_V: Pixels = px(50.0);
const GRID_EMPTY_GLYPH: Pixels = px(22.0);
/// Badge font on a card's added / off state pill.
const GRID_BADGE_FS: Pixels = px(9.0);

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

/// Local id for a seeded action. `forge-desktop` wires no actions repo yet, so the
/// tree is seeded in-memory and ids are minted from a per-view counter rather than
/// the runtime's persistent `ActionId`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ActionId(u64);

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

/// A cached action summary — the tree row's payload. The real screen reads these
/// from the actions repo over the runtime→UI bridge; here they are seeded.
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
    queue_options: Vec<SharedString>,
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
/// Owns its tree, selection and interaction state as seeded stub state — the real
/// screen loads the tree from the actions repo over the runtime→UI bridge and drives
/// every mutation through the runtime handle. Here the CRUD (rename / duplicate /
/// enable / delete / add) mutates this cached state locally. The right pane renders
/// the empty "no action selected" placeholder for every selection state; the real
/// editor (telemetry, trigger links, sub-action chain) lands in a follow-up slice.
pub struct ScreenActionsView {
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
    /// The unified centred "Add" grid picker, driving both sub-action and trigger
    /// adds off its `kind`.
    grid_picker: Option<GridPickerForm>,
    /// Element id of the grid card currently under the pointer, so its frame and
    /// trailing glyph recolour on hover (the design's per-card hover feedback).
    grid_hover: Option<SharedString>,
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
    next_id: u64,
    _search_sub: Subscription,
}

impl ScreenActionsView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search actions...", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_search_event);

        let mut next_id = 0u64;
        let mut mint = || {
            let id = ActionId(next_id);
            next_id += 1;
            id
        };
        let groups = seed_groups(&mut mint);

        Self {
            groups,
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
            grid_hover: None,
            pending_trigger_unlink: None,
            nav_path: Vec::new(),
            case_fields: BTreeMap::new(),
            next_id,
            _search_sub: search_sub,
        }
    }

    fn mint_id(&mut self) -> ActionId {
        let id = ActionId(self.next_id);
        self.next_id += 1;
        id
    }

    // --- pure lookup helpers ----------------------------------------------

    fn find(&self, id: ActionId) -> Option<&ActionSummary> {
        self.groups
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == id)
    }

    fn total_actions(&self) -> usize {
        self.groups.iter().map(|g| g.actions.len()).sum()
    }

    /// Whether a filter tab admits a group's category. `Other` shows only under
    /// `All`.
    fn category_visible(filter: ActionsFilter, category: ActionCategory) -> bool {
        match filter {
            ActionsFilter::All => true,
            ActionsFilter::Chat => category == ActionCategory::Chat,
            ActionsFilter::Timers => category == ActionCategory::Timers,
            ActionsFilter::Points => category == ActionCategory::Points,
        }
    }

    /// A row survives the current filter + search. Combining both here reproduces the
    /// source's per-action gate: a group in a hidden category yields no surviving rows
    /// and is skipped whole.
    fn action_passes(&self, group: &ActionGroup, action: &ActionSummary) -> bool {
        if !Self::category_visible(self.filter, group.category) {
            return false;
        }
        if self.search.is_empty() {
            return true;
        }
        action
            .name
            .to_lowercase()
            .contains(&self.search.to_lowercase())
    }

    // --- interaction handlers ---------------------------------------------

    fn on_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event {
            self.search = text.to_string();
            cx.notify();
        }
    }

    fn set_filter(&mut self, filter: ActionsFilter, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    fn toggle_group(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(group) = self.groups.get_mut(index) {
            group.collapsed = !group.collapsed;
            cx.notify();
        }
    }

    fn select(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.detail = self.find(id).map(build_detail);
        self.nav_path.clear();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn set_hover(&mut self, id: ActionId, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.hovered != Some(id) {
                self.hovered = Some(id);
                cx.notify();
            }
        } else if self.hovered == Some(id) {
            self.hovered = None;
            cx.notify();
        }
    }

    fn toggle_menu(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            Some(id)
        };
        cx.notify();
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.menu_open = None;
        cx.notify();
    }

    fn set_enabled(&mut self, id: ActionId, enabled: bool, cx: &mut Context<Self>) {
        for group in &mut self.groups {
            if let Some(action) = group.actions.iter_mut().find(|a| a.id == id) {
                action.enabled = enabled;
            }
        }
        self.menu_open = None;
        cx.notify();
    }

    /// Clones an action into its own group, right after the original, with a fresh id.
    fn duplicate(&mut self, id: ActionId, cx: &mut Context<Self>) {
        let new_id = self.mint_id();
        for group in &mut self.groups {
            if let Some(pos) = group.actions.iter().position(|a| a.id == id) {
                let src = &group.actions[pos];
                let clone = ActionSummary {
                    id: new_id,
                    name: format!("{} copy", src.name),
                    enabled: src.enabled,
                    sub_action_count: src.sub_action_count,
                };
                group.actions.insert(pos + 1, clone);
                break;
            }
        }
        self.menu_open = None;
        cx.notify();
    }

    fn start_rename(&mut self, id: ActionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.find(id) else {
            return;
        };
        let palette = cx.palette();
        let seed = action.name.clone();
        let field = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx)
                .with_palette(palette)
                .static_chrome(palette.brand, Radius::Sm);
            input.set_content(seed, cx);
            input
        });
        field.read(cx).focus(window);
        let sub = cx.subscribe(&field, Self::on_rename_event);
        self.menu_open = None;
        self.renaming = Some(Renaming {
            id,
            field,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_rename_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Submitted(text) => self.commit_rename(text.to_string(), cx),
            InputEvent::Cancelled => {
                self.renaming = None;
                cx.notify();
            }
            InputEvent::Changed(_) => {}
        }
    }

    fn commit_rename(&mut self, name: String, cx: &mut Context<Self>) {
        let trimmed = name.trim();
        if let Some(renaming) = self.renaming.take()
            && !trimmed.is_empty()
        {
            for group in &mut self.groups {
                if let Some(action) = group.actions.iter_mut().find(|a| a.id == renaming.id) {
                    action.name = trimmed.to_owned();
                }
            }
        }
        cx.notify();
    }

    // --- delete (two-phase confirm) ---------------------------------------

    fn request_delete(&mut self, id: ActionId, cx: &mut Context<Self>) {
        self.pending_delete = Some(id);
        self.menu_open = None;
        cx.notify();
    }

    fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        self.pending_delete = None;
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.pending_delete.take() {
            for group in &mut self.groups {
                group.actions.retain(|a| a.id != id);
            }
            if self.selected == Some(id) {
                self.selected = None;
                self.detail = None;
                self.nav_path.clear();
                self.case_fields.clear();
            }
        }
        cx.notify();
    }

    // --- add-action modal -------------------------------------------------

    fn open_add_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let name = cx.new(|cx| TextInput::new("My automation", cx).with_palette(palette));
        let group = cx.new(|cx| TextInput::new("Examples", cx).with_palette(palette));
        let description = cx.new(|cx| {
            TextArea::new("Plays a sound, shows overlay alert…", cx).with_palette(palette)
        });
        name.read(cx).focus(window);
        let name_sub = cx.subscribe(&name, |_this, _f, _e: &InputEvent, cx| cx.notify());
        self.add_modal = Some(AddActionForm {
            name,
            group,
            description,
            queue_options: vec!["Default".into(), "TTS".into(), "Alerts".into()],
            selected_queue: 0,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            random_pick: false,
            _name_sub: name_sub,
        });
        cx.notify();
    }

    fn cancel_add_modal(&mut self, cx: &mut Context<Self>) {
        self.add_modal = None;
        cx.notify();
    }

    fn select_queue(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.selected_queue = index;
            cx.notify();
        }
    }

    fn toggle_modal_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.enabled = !form.enabled;
            cx.notify();
        }
    }

    fn toggle_modal_concurrent(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.concurrent = !form.concurrent;
            cx.notify();
        }
    }

    fn toggle_modal_bypass(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.bypass_pause = !form.bypass_pause;
            cx.notify();
        }
    }

    fn toggle_modal_random(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.add_modal.as_mut() {
            form.random_pick = !form.random_pick;
            cx.notify();
        }
    }

    /// Commits the modal into the cached tree: the new action lands in the group whose
    /// name the form carries (a new `Other` group is minted if none matches), then the
    /// modal closes. No-op while the name is blank.
    fn submit_add_modal(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.add_modal.as_ref() else {
            return;
        };
        let name = form.name.read(cx).content().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let group_name = form.group.read(cx).content().trim().to_owned();
        let enabled = form.enabled;
        let id = self.mint_id();
        let summary = ActionSummary {
            id,
            name,
            enabled,
            sub_action_count: 0,
        };

        let target = if group_name.is_empty() {
            self.groups.first_mut()
        } else {
            self.groups
                .iter_mut()
                .find(|g| g.name.eq_ignore_ascii_case(&group_name))
        };
        match target {
            Some(group) => group.actions.push(summary),
            None => self.groups.push(ActionGroup {
                name: group_name.into(),
                category: ActionCategory::Other,
                collapsed: false,
                actions: vec![summary],
            }),
        }
        self.selected = Some(id);
        self.detail = self.find(id).map(build_detail);
        self.nav_path.clear();
        self.sync_case_fields(cx);
        self.add_modal = None;
        cx.notify();
    }

    // --- render: page header ----------------------------------------------

    fn render_header(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let chips = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                chip(
                    "All",
                    ChipGlyph::Dot(palette.brand),
                    self.filter == ActionsFilter::All,
                    palette,
                )
                .on_click(
                    "actions-chip-all",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::All, cx)),
                ),
            )
            .child(
                chip(
                    "Chat",
                    ChipGlyph::DotIcon(palette.info, Icon::MessageCircle),
                    self.filter == ActionsFilter::Chat,
                    palette,
                )
                .on_click(
                    "actions-chip-chat",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Chat, cx)),
                ),
            )
            .child(
                chip(
                    "Timers",
                    ChipGlyph::DotIcon(palette.warning, Icon::Clock),
                    self.filter == ActionsFilter::Timers,
                    palette,
                )
                .on_click(
                    "actions-chip-timers",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Timers, cx)),
                ),
            )
            .child(
                chip(
                    "Points",
                    ChipGlyph::DotIcon(palette.accent_pink_light, Icon::Star),
                    self.filter == ActionsFilter::Points,
                    palette,
                )
                .on_click(
                    "actions-chip-points",
                    cx.listener(|this, _, _, cx| this.set_filter(ActionsFilter::Points, cx)),
                ),
            );

        let divider = div()
            .w(HEADER_DIV_W)
            .h(HEADER_DIV_H)
            .bg(palette.border_regular);

        let search = div().w(SEARCH_W).child(self.search_field.clone());

        let new_btn = primary_button_with_icon(Icon::Plus, "New action", palette).on_click(
            "actions-new",
            cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_modal(window, cx)),
        );

        let cluster = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(chips)
            .child(divider)
            .child(search)
            .child(new_btn);

        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Automation"),
                BreadcrumbCrumb::leaf("Actions"),
            ],
            palette,
        )
        .right(cluster)
        .into_any_element()
    }

    // --- render: left tree ------------------------------------------------

    fn render_tree(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex().flex_col();

        if self.total_actions() == 0 {
            col = col.child(tree_notice("No actions yet", palette.text_faint, palette));
        } else {
            for (index, group) in self.groups.iter().enumerate() {
                let surviving: Vec<&ActionSummary> = group
                    .actions
                    .iter()
                    .filter(|a| self.action_passes(group, a))
                    .collect();
                if surviving.is_empty() {
                    continue;
                }
                col = col.child(self.render_group_header(index, group, palette, cx));
                if !group.collapsed {
                    for action in surviving {
                        col = col.child(self.render_row(action, palette, cx));
                    }
                }
            }
        }

        div()
            .id("actions-tree")
            .w(LEFT_PANEL_W)
            .flex_none()
            .h_full()
            .py(spacing(Spacing::Xs, Density::Cozy))
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .overflow_y_scroll()
            .child(col)
            .into_any_element()
    }

    fn render_group_header(
        &self,
        index: usize,
        group: &ActionGroup,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chevron = if group.collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let hover_bg = palette.elevated;
        div()
            .id(SharedString::from(format!("actions-group-{index}")))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .px(TREE_GUTTER)
            .py(px(6.0))
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(index, cx)))
            .child(icon(chevron, TREE_GLYPH, palette.text_muted))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(group.name.clone()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(group.actions.len().to_string()),
            )
            .into_any_element()
    }

    fn render_row(
        &self,
        action: &ActionSummary,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = action.id;
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);
        let menu_open = self.menu_open == Some(id);
        let renaming = self.renaming.as_ref().filter(|r| r.id == id);

        let (state_icon, state_color) = if action.enabled {
            (Icon::CircleCheckFilled, palette.success)
        } else {
            (Icon::Circle, palette.text_faint)
        };
        let name_color = if !action.enabled {
            palette.text_faint
        } else if selected {
            palette.text_primary
        } else {
            palette.text_secondary
        };
        let stripe_color = if selected {
            palette.brand
        } else {
            gpui::transparent_black().into()
        };
        let row_bg: Rgba = if selected || hovered {
            palette.elevated
        } else {
            gpui::transparent_black().into()
        };

        // The name column: an inline rename field while this row is being renamed,
        // otherwise the (clipped, non-wrapping) name label.
        let name_el: AnyElement = match renaming {
            Some(renaming) => div()
                .flex_1()
                .min_w(px(0.0))
                .child(renaming.field.clone())
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(name_color)
                .child(action.name.clone())
                .into_any_element(),
        };

        // The select area: state icon + name, indented, filling the row's free width.
        // A rename field swallows its own clicks, so selecting is disabled mid-rename.
        let mut select_area = div()
            .id(SharedString::from(format!("actions-row-select-{}", id.0)))
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .pl(ROW_INDENT)
            .pr(px(8.0))
            .child(icon(state_icon, TREE_GLYPH, state_color))
            .child(name_el);
        if renaming.is_none() {
            select_area = select_area
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(id, cx)));
        }

        // The right slot: the "N sub" count, swapped for the `⋮` overflow menu while
        // the row is hovered or its menu is open, inside a fixed 46px slot so the edge
        // never shifts.
        let show_menu = hovered || menu_open;
        let slot_inner: AnyElement = if show_menu {
            self.render_row_menu(action, menu_open, palette, cx)
        } else {
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(format!("{} sub", action.sub_action_count))
                .into_any_element()
        };
        let right_slot = div().pr(ROW_GUTTER).child(
            div()
                .w(RIGHT_SLOT_W)
                .flex()
                .items_center()
                .justify_end()
                .child(slot_inner),
        );

        div()
            .id(SharedString::from(format!("actions-row-{}", id.0)))
            .w_full()
            .h(ROW_HEIGHT)
            .flex()
            .items_center()
            .bg(row_bg)
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.set_hover(id, *hovered, cx)),
            )
            .child(div().w(STRIPE_W).h_full().bg(stripe_color))
            .child(select_area)
            .child(right_slot)
            .into_any_element()
    }

    fn render_row_menu(
        &self,
        action: &ActionSummary,
        menu_open: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = action.id;
        let toggle_label = if action.enabled { "Disable" } else { "Enable" };
        let next_enabled = !action.enabled;
        let view = cx.entity();

        menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("actions-menu-rename-{}", id.0)),
                    "Rename…",
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.start_rename(id, window, cx)
                    }),
                )
                .icon(Icon::Pencil)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-menu-dup-{}", id.0)),
                    "Duplicate",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate(id, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-menu-toggle-{}", id.0)),
                    toggle_label,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_enabled(id, next_enabled, cx)
                    }),
                )
                .icon(Icon::Bolt)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-menu-del-{}", id.0)),
                    "Delete…",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.request_delete(id, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("actions-menu-trigger-{}", id.0)),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_menu(id, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    // --- editor: step interaction handlers --------------------------------

    /// Copies `id`/`n` out of the loaded detail before touching the tree so the
    /// summary borrow ends before the mutable group iteration begins. The tree badge
    /// tracks the action's *top-level* chain length, so a nested edit leaves it
    /// unchanged.
    fn sync_selected_count(&mut self) {
        let Some((id, n)) = self.detail.as_ref().map(|d| (d.action_id, d.steps.len())) else {
            return;
        };
        for group in &mut self.groups {
            if let Some(action) = group.actions.iter_mut().find(|a| a.id == id) {
                action.sub_action_count = n;
            }
        }
    }

    /// The chain the step list currently renders — the action's top-level steps at
    /// root, or the nested sub-chain [`Self::nav_path`] descends into. Falls back to
    /// an empty slice when the path no longer resolves (never panics).
    fn current_chain(&self) -> &[EditorStep] {
        match &self.detail {
            Some(detail) => resolve_chain(&detail.steps, &self.nav_path).unwrap_or(&[]),
            None => &[],
        }
    }

    /// Mutable handle to the current chain. Clones the (small, `Copy`-framed) nav path
    /// so the detail can be borrowed mutably alongside it.
    fn current_chain_mut(&mut self) -> Option<&mut Vec<EditorStep>> {
        let path = self.nav_path.clone();
        resolve_chain_mut(&mut self.detail.as_mut()?.steps, &path)
    }

    fn move_step(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && from < chain.len()
            && to < chain.len()
            && from != to
        {
            let step = chain.remove(from);
            chain.insert(to, step);
        }
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn move_step_up(&mut self, i: usize, cx: &mut Context<Self>) {
        if i > 0 {
            self.move_step(i, i - 1, cx);
        }
    }

    fn move_step_down(&mut self, i: usize, cx: &mut Context<Self>) {
        self.move_step(i, i + 1, cx);
    }

    fn move_step_top(&mut self, i: usize, cx: &mut Context<Self>) {
        self.move_step(i, 0, cx);
    }

    fn move_step_bottom(&mut self, i: usize, cx: &mut Context<Self>) {
        let last = self.current_chain().len();
        if last > 0 {
            self.move_step(i, last - 1, cx);
        }
    }

    fn duplicate_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(src) = chain.get(i)
        {
            let clone = src.clone();
            chain.insert(i + 1, clone);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn remove_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && i < chain.len()
        {
            chain.remove(i);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    // --- editor: branch drill-in + switch cases ---------------------------

    /// Descends into a composite step's nested sub-chain or a switch case, pushing a
    /// nav frame. Refuses to create new depth past the authoring cap on an empty
    /// branch — mirrors the disabled drill-in chip so a stale click is inert.
    fn enter_branch(
        &mut self,
        step_index: usize,
        chain_key: &'static str,
        case_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let count = self
            .current_chain()
            .get(step_index)
            .map(|s| branch_count(s, chain_key, case_index))
            .unwrap_or(0);
        if self.nav_path.len() >= UI_MAX_NESTING_DEPTH && count == 0 {
            return;
        }
        self.nav_path.push(NavFrame {
            step_index,
            chain_key,
            case_index,
        });
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    /// Pops the nav path back to `depth` (a breadcrumb ancestor segment).
    fn breadcrumb_pop(&mut self, depth: usize, cx: &mut Context<Self>) {
        self.nav_path.truncate(depth);
        self.step_menu_open = None;
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn add_switch_case(&mut self, step_index: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
        {
            cases.push(SwitchCase {
                match_value: CaseMatch::Single(String::new()),
                chain: Vec::new(),
            });
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn remove_switch_case(&mut self, step_index: usize, case_index: usize, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
            && case_index < cases.len()
        {
            cases.remove(case_index);
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    fn move_switch_case(
        &mut self,
        step_index: usize,
        case_index: usize,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(cases) = chain.get_mut(step_index).and_then(|s| s.cases.as_mut())
        {
            let target = if up {
                case_index.checked_sub(1)
            } else {
                case_index.checked_add(1).filter(|&t| t < cases.len())
            };
            if let Some(t) = target
                && case_index < cases.len()
            {
                cases.swap(case_index, t);
            }
        }
        self.sync_case_fields(cx);
        cx.notify();
    }

    /// Writes a switch case's single-value match back into the model. Multi-value
    /// imported matches carry no input, so they are never reached here.
    fn commit_case_match(
        &mut self,
        step_index: usize,
        case_index: usize,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(chain) = self.current_chain_mut()
            && let Some(case) = chain
                .get_mut(step_index)
                .and_then(|s| s.cases.as_mut())
                .and_then(|cases| cases.get_mut(case_index))
            && let CaseMatch::Single(m) = &mut case.match_value
        {
            *m = value.trim().to_owned();
        }
        cx.notify();
    }

    /// Rebuilds the per-case match inputs for every switch step in the current chain.
    /// Called at each edge that reshapes the current chain (nav change, step reorder,
    /// case add/remove/move) so the `(step_index, case_index)` keys stay accurate.
    fn sync_case_fields(&mut self, cx: &mut Context<Self>) {
        let specs: Vec<(usize, usize, String)> = {
            let chain = self.current_chain();
            let mut specs = Vec::new();
            for (si, step) in chain.iter().enumerate() {
                if let Some(cases) = &step.cases {
                    for (ci, case) in cases.iter().enumerate() {
                        if let CaseMatch::Single(m) = &case.match_value {
                            specs.push((si, ci, m.clone()));
                        }
                    }
                }
            }
            specs
        };

        let palette = cx.palette();
        let mut fields = BTreeMap::new();
        for (si, ci, seed) in specs {
            let field = cx.new(|cx| {
                let mut input = TextInput::new("match value", cx).with_palette(palette);
                if !seed.is_empty() {
                    input.set_content(seed, cx);
                }
                input
            });
            let sub = cx.subscribe(&field, move |this, _f, event: &InputEvent, cx| {
                if let InputEvent::Submitted(text) = event {
                    this.commit_case_match(si, ci, text.to_string(), cx);
                }
            });
            fields.insert((si, ci), CaseField { field, _sub: sub });
        }
        self.case_fields = fields;
    }

    fn toggle_step_menu(&mut self, i: usize, cx: &mut Context<Self>) {
        self.step_menu_open = if self.step_menu_open == Some(i) {
            None
        } else {
            Some(i)
        };
        cx.notify();
    }

    fn close_step_menu(&mut self, cx: &mut Context<Self>) {
        self.step_menu_open = None;
        cx.notify();
    }

    /// Local, persistence-free re-run affordance: the runtime engine is not yet wired
    /// into `forge-desktop`, so Test-run only repaints.
    fn test_run(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    // --- editor: edit-sub-action side sheet -------------------------------

    fn open_edit_sub_action(&mut self, i: usize, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let Some((kind, seed)) = self
            .current_chain()
            .get(i)
            .map(|step| (step.kind, step.config.clone()))
        else {
            return;
        };
        let fields = build_sub_fields(kind, &seed, palette, cx);
        self.step_menu_open = None;
        self.sub_form = Some(EditSubActionForm {
            kind,
            fields,
            index: i,
        });
        cx.notify();
    }

    fn cancel_sub_action(&mut self, cx: &mut Context<Self>) {
        self.sub_form = None;
        cx.notify();
    }

    fn submit_sub_action(&mut self, cx: &mut Context<Self>) {
        let (kind, index, fields) = {
            let Some(form) = self.sub_form.as_ref() else {
                return;
            };
            (form.kind, form.index, form.fields.clone())
        };

        let mut config = BTreeMap::new();
        for (spec, input) in &fields {
            config.insert(spec.key.to_owned(), input.read(cx).content().to_owned());
        }

        // Editing keeps the step's nested branches / cases intact — only its kind +
        // scalar config are re-authored from the form.
        if let Some(chain) = self.current_chain_mut()
            && let Some(step) = chain.get_mut(index)
        {
            step.kind = kind;
            step.config = config;
        }
        self.sync_selected_count();
        self.sync_case_fields(cx);
        self.sub_form = None;
        cx.notify();
    }

    // --- editor: unified "Add" grid picker --------------------------------

    fn open_grid_picker(&mut self, kind: PickerKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action_id) = self.selected else {
            return;
        };
        if self.detail.is_none() {
            return;
        }
        let palette = cx.palette();
        let placeholder: SharedString = match kind {
            PickerKind::Step => format!("Search {} sub-actions\u{2026}", SUB_KINDS.len()).into(),
            PickerKind::Trigger => "Search triggers\u{2026}".into(),
        };
        let search_field = cx.new(|cx| {
            TextInput::new(placeholder, cx)
                .with_palette(palette)
                .leading_icon(Icon::Search, palette.text_muted)
                .with_font_size(GRID_SEARCH_FS)
                .static_chrome(palette.border_regular, Radius::Sm)
        });
        let search_sub = cx.subscribe(&search_field, Self::on_grid_search_event);
        search_field.read(cx).focus(window);
        let trigger_entries = match kind {
            PickerKind::Trigger => seed_picker_entries(),
            PickerKind::Step => Vec::new(),
        };
        self.step_menu_open = None;
        self.grid_hover = None;
        self.grid_picker = Some(GridPickerForm {
            kind,
            action_id,
            search_field,
            search: String::new(),
            scope: None,
            trigger_entries,
            _search_sub: search_sub,
        });
        cx.notify();
    }

    fn on_grid_search_event(
        &mut self,
        field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Changed(text) => {
                let palette = cx.palette();
                let Some(form) = self.grid_picker.as_mut() else {
                    return;
                };
                form.search = text.to_string();
                let border = if form.search.trim().is_empty() {
                    palette.border_regular
                } else {
                    form.kind.accent(&palette)
                };
                field.update(cx, |input, cx| {
                    input.set_static_chrome(Some((border, Radius::Sm)));
                    cx.notify();
                });
                cx.notify();
            }
            InputEvent::Cancelled => self.cancel_grid_picker(cx),
            InputEvent::Submitted(_) => {}
        }
    }

    fn clear_grid_search(&mut self, cx: &mut Context<Self>) {
        let palette = cx.palette();
        if let Some(form) = self.grid_picker.as_mut() {
            form.search.clear();
            let field = form.search_field.clone();
            field.update(cx, |input, cx| {
                input.set_content("", cx);
                input.set_static_chrome(Some((palette.border_regular, Radius::Sm)));
            });
        }
        cx.notify();
    }

    fn set_grid_scope(&mut self, scope: Option<SharedString>, cx: &mut Context<Self>) {
        if let Some(form) = self.grid_picker.as_mut() {
            form.scope = scope;
        }
        cx.notify();
    }

    fn set_grid_hover(&mut self, id: SharedString, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            if self.grid_hover.as_ref() != Some(&id) {
                self.grid_hover = Some(id);
                cx.notify();
            }
        } else if self.grid_hover.as_ref() == Some(&id) {
            self.grid_hover = None;
            cx.notify();
        }
    }

    fn cancel_grid_picker(&mut self, cx: &mut Context<Self>) {
        self.grid_picker = None;
        self.grid_hover = None;
        cx.notify();
    }

    fn grid_pick_step(&mut self, kind: SubKind, cx: &mut Context<Self>) {
        if let Some(chain) = self.current_chain_mut() {
            chain.push(EditorStep::new(kind, kind.seed_config()));
        }
        self.sync_selected_count();
        self.sync_case_fields(cx);
        self.grid_picker = None;
        self.grid_hover = None;
        cx.notify();
    }

    /// Links a picked trigger to the open action, guarding on the picker still
    /// targeting the selected action, then closes the picker.
    fn grid_pick_trigger(&mut self, trigger: SeededTrigger, cx: &mut Context<Self>) {
        let same = self
            .grid_picker
            .as_ref()
            .zip(self.detail.as_ref())
            .is_some_and(|(f, d)| f.action_id == d.action_id);
        if same && let Some(detail) = self.detail.as_mut() {
            detail.triggers.push(trigger);
        }
        self.grid_picker = None;
        self.grid_hover = None;
        cx.notify();
    }

    // --- trigger links: unlink --------------------------------------------

    /// Navigate-to-registry intent for a trigger card. The triggers registry screen is
    /// not yet built in `forge-desktop`, so the click is inert.
    fn trigger_card_clicked(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn request_trigger_unlink(&mut self, index: usize, cx: &mut Context<Self>) {
        self.pending_trigger_unlink = Some(index);
        cx.notify();
    }

    fn cancel_trigger_unlink(&mut self, cx: &mut Context<Self>) {
        self.pending_trigger_unlink = None;
        cx.notify();
    }

    fn confirm_trigger_unlink(&mut self, cx: &mut Context<Self>) {
        if let Some(i) = self.pending_trigger_unlink.take()
            && let Some(detail) = self.detail.as_mut()
            && i < detail.triggers.len()
        {
            detail.triggers.remove(i);
        }
        cx.notify();
    }

    // --- render: right editor pane ----------------------------------------

    fn render_editor_pane(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        match (self.selected, self.detail.as_ref()) {
            (Some(sel), Some(detail)) if detail.action_id == sel => {
                self.render_editor(detail, palette, cx)
            }
            (Some(_), _) => self.render_loading(palette),
            (None, _) => self.render_empty(palette),
        }
    }

    fn render_empty(&self, palette: &ForgePalette) -> AnyElement {
        let placeholder = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(icon(Icon::Bolt, EMPTY_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child("No action selected"),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child("Select an action from the list to view its details."),
            );

        div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(placeholder)
            .into_any_element()
    }

    fn render_loading(&self, palette: &ForgePalette) -> AnyElement {
        div()
            .flex_1()
            .h_full()
            .py(spacing(Spacing::Md, Density::Cozy))
            .px(spacing(Spacing::Lg, Density::Cozy))
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child("Loading action…")
            .into_any_element()
    }

    fn render_editor(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, Density::Cozy))
            .child(self.render_editor_header(detail, palette, cx))
            .child(self.render_triggers_section(detail, palette, cx))
            .child(self.render_sub_actions_section(detail, palette, cx));

        div()
            .id("actions-editor-scroll")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .py(PANE_PAD_V)
            .px(PANE_PAD_H)
            .child(body)
            .into_any_element()
    }

    fn render_editor_header(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (pill_color, pill_label) = if detail.enabled {
            (palette.success, "Enabled")
        } else {
            (palette.text_faint, "Disabled")
        };
        let pill = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .py(px(1.0))
            .px(px(6.0))
            .rounded(PILL_RADIUS)
            .bg(palette.surface_overlay)
            .child(status_dot(pill_color, PILL_DOT))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(pill_color)
                    .child(pill_label),
            );

        let title_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_LG)
                    .text_color(palette.text_primary)
                    .child(detail.name.clone()),
            )
            .child(pill);

        let desc = detail
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_owned());
        let desc_line = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(desc);

        let header_left = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(title_row)
            .child(desc_line);

        let btn_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, "Test run", palette).on_click(
                    "actions-editor-test",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.test_run(cx)),
                ),
            )
            .child(
                ghost_button_with_icon(Icon::Copy, "Duplicate", palette).on_click(
                    "actions-editor-dup",
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        if let Some(id) = this.selected {
                            this.duplicate(id, cx);
                        }
                    }),
                ),
            );

        div()
            .flex()
            .items_start()
            .justify_between()
            .child(header_left)
            .child(btn_row)
            .into_any_element()
    }

    fn render_triggers_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_muted)
            .child(format!("TRIGGERS · {}", detail.triggers.len()));

        let hint = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(icon(Icon::InfoCircle, HINT_GLYPH, palette.text_faint))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("Click a trigger to edit it in the registry"),
            );

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(label)
            .child(hint);

        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        if detail.triggers.is_empty() {
            col = col.child(empty_placeholder_card(
                Icon::Bolt,
                palette.warning,
                "No triggers — this action will never fire on its own",
                palette,
            ));
        } else {
            for (index, trigger) in detail.triggers.iter().enumerate() {
                col = col.child(self.render_trigger_card(index, trigger, palette, cx));
            }
        }
        col = col.child(add_row_button(
            "actions-add-trigger",
            Icon::Plus,
            "Add trigger",
            palette.warning,
            palette,
            cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_grid_picker(PickerKind::Trigger, window, cx)
            }),
        ));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(header)
            .child(col)
            .into_any_element()
    }

    fn render_sub_actions_section(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = resolve_chain(&detail.steps, &self.nav_path).unwrap_or(&[]);
        let total = current.len();
        let at_root = self.nav_path.is_empty();
        let depth = self.nav_path.len();

        // At root: the mono sub-action count. Drilled in: a breadcrumb of the nav
        // path with the current chain's length pinned to the right edge.
        let header = if at_root {
            div().flex().items_center().child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(format!("{total} sub-actions")),
            )
        } else {
            div()
                .flex()
                .items_center()
                .child(self.render_breadcrumb(detail, palette, cx))
                .child(div().flex_1())
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child(total.to_string()),
                )
        };

        let mut steps_col = div().flex().flex_col();
        if current.is_empty() {
            let empty_label = if at_root {
                "This action has no steps yet"
            } else {
                "No steps yet · click Add step to start"
            };
            steps_col = steps_col.child(empty_placeholder_card(
                Icon::Plus,
                palette.brand,
                empty_label,
                palette,
            ));
        }
        for (i, step) in current.iter().enumerate() {
            steps_col = steps_col.child(self.render_step_block(step, i, total, depth, palette, cx));
        }
        steps_col = steps_col.child(
            div()
                .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                .pt(spacing(Spacing::Xs, Density::Cozy))
                .child(add_row_button(
                    "actions-add-step",
                    Icon::Plus,
                    "Add step",
                    palette.brand,
                    palette,
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_grid_picker(PickerKind::Step, window, cx)
                    }),
                )),
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(header)
            .child(steps_col)
            .into_any_element()
    }

    fn render_step_block(
        &self,
        step: &EditorStep,
        i: usize,
        total: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last = i + 1 == total;

        let circle = div()
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_CIRCLE)
            .rounded(STEP_CIRCLE_RADIUS)
            .bg(palette.brand)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.shell)
                    .child((i + 1).to_string()),
            );
        let connector = div()
            .w(STEP_CONNECTOR_W)
            .h(if is_last { px(0.0) } else { STEP_CONNECTOR_H })
            .bg(palette.border_regular);
        let left_col = div()
            .flex()
            .flex_col()
            .items_center()
            .w(STEP_COL_W)
            .child(circle)
            .child(connector);

        let title_el = div()
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(step.kind.label());

        let card = row_card(title_el, palette)
            .leading(icon(step.kind.glyph(), CARD_GLYPH, palette.text_secondary))
            .meta(variable_text(&step.detail(), palette))
            .trailing(self.render_step_controls(i, total, palette, cx))
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md));

        let step_row = div()
            .flex()
            .items_start()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(left_col)
            .child(div().flex_1().min_w(px(0.0)).child(card));

        // Composite / switch steps carry their branch drill-ins indented under the
        // card body, aligned past the step-circle column.
        let block: AnyElement = match self.render_branch_affordances(step, i, depth, palette, cx) {
            Some(branches) => {
                let indented = div()
                    .pl(STEP_COL_W + spacing(Spacing::Xs, Density::Cozy))
                    .pt(spacing(Spacing::Xxs, Density::Cozy))
                    .child(branches);
                div()
                    .flex()
                    .flex_col()
                    .child(step_row)
                    .child(indented)
                    .into_any_element()
            }
            None => step_row.into_any_element(),
        };

        div()
            .w_full()
            .pb(if is_last { px(0.0) } else { STEP_GAP })
            .child(block)
            .into_any_element()
    }

    /// The breadcrumb that replaces the step-count header while drilled in. Every
    /// ancestor segment pops the nav path to its depth; the current (final) segment
    /// is inert.
    fn render_breadcrumb(
        &self,
        detail: &ActionDetail,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // `(label, pop_target)` — a `Some(depth)` target makes the segment a
        // pop-to-that-depth button; `None` is the inert current segment.
        let mut segments: Vec<(String, Option<usize>)> = vec![("Steps".to_owned(), Some(0))];
        for (depth, frame) in self.nav_path.iter().enumerate() {
            let prefix = resolve_chain(&detail.steps, &self.nav_path[..depth]).unwrap_or(&[]);
            let step_label = prefix
                .get(frame.step_index)
                .map(|s| s.kind.label().to_owned())
                .unwrap_or_else(|| "Sub-action".to_owned());
            let branch_label = match frame.case_index {
                Some(ci) => format!("Case {}", ci + 1),
                None => branch_field_label(frame.chain_key).to_owned(),
            };
            let pop_target = if depth + 1 == self.nav_path.len() {
                None
            } else {
                Some(depth + 1)
            };
            segments.push((format!("{step_label} \u{2023} {branch_label}"), pop_target));
        }

        let last = segments.len().saturating_sub(1);
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (idx, (label, target)) in segments.into_iter().enumerate() {
            if idx > 0 {
                row = row.child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(palette.text_faint)
                        .child("\u{25B8}"),
                );
            }
            match target {
                Some(depth) => {
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("actions-breadcrumb-{depth}")))
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.breadcrumb_pop(depth, cx)
                            }))
                            .child(label),
                    );
                }
                None => {
                    let color = if idx == last {
                        palette.text_secondary
                    } else {
                        palette.text_muted
                    };
                    row = row.child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(color)
                            .child(label),
                    );
                }
            }
        }
        row.into_any_element()
    }

    /// The drill-in affordances under a composite / switch step: one chip per single
    /// sub-chain (then / else / body / default) and, for a switch, a full per-case row
    /// editor. `None` when the step declares no nested chains.
    fn render_branch_affordances(
        &self,
        step: &EditorStep,
        step_index: usize,
        depth: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let specs = step.kind.branch_specs();
        if specs.is_empty() {
            return None;
        }
        let at_cap = depth >= UI_MAX_NESTING_DEPTH;
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut capped_empty = false;

        for spec in specs {
            match spec {
                BranchSpec::Chain { key, label } => {
                    let count = branch_count(step, key, None);
                    let disabled = at_cap && count == 0;
                    capped_empty |= disabled;
                    let key = *key;
                    rows.push(drill_in_chip(
                        SharedString::from(format!("actions-drill-{step_index}-{key}")),
                        label,
                        count,
                        disabled,
                        palette,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.enter_branch(step_index, key, None, cx)
                        }),
                    ));
                }
                BranchSpec::Cases { key, label } => {
                    let key = *key;
                    let case_total = step.cases.as_ref().map(Vec::len).unwrap_or(0);
                    rows.push(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(format!("{label}:"))
                            .into_any_element(),
                    );
                    for ci in 0..case_total {
                        rows.push(self.render_case_row(
                            step, step_index, ci, key, case_total, at_cap, palette, cx,
                        ));
                    }
                    rows.push(self.render_add_case(step_index, palette, cx));
                    if at_cap {
                        capped_empty |=
                            (0..case_total).any(|ci| branch_count(step, key, Some(ci)) == 0);
                    }
                }
            }
        }

        if capped_empty {
            rows.push(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.warning)
                    .child("Max nesting depth reached · cannot nest deeper here")
                    .into_any_element(),
            );
        }

        Some(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .children(rows)
                .into_any_element(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_case_row(
        &self,
        step: &EditorStep,
        step_index: usize,
        ci: usize,
        key: &'static str,
        case_total: usize,
        at_cap: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = branch_count(step, key, Some(ci));
        let disabled = at_cap && count == 0;

        let is_multi = step
            .cases
            .as_ref()
            .and_then(|cases| cases.get(ci))
            .map(|c| matches!(c.match_value, CaseMatch::Multi))
            .unwrap_or(false);
        let match_el: AnyElement = if is_multi {
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_faint)
                .child("multi-value match (read-only)")
                .into_any_element()
        } else {
            div()
                .w(CASE_MATCH_W)
                .flex_none()
                .children(
                    self.case_fields
                        .get(&(step_index, ci))
                        .map(|f| f.field.clone()),
                )
                .into_any_element()
        };

        let drill = drill_in_chip(
            SharedString::from(format!("actions-drill-{step_index}-case-{ci}")),
            "Chain",
            count,
            disabled,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.enter_branch(step_index, key, Some(ci), cx)
            }),
        );
        let move_up = step_icon_btn(
            SharedString::from(format!("actions-case-up-{step_index}-{ci}")),
            Icon::ArrowUp,
            ci == 0,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.move_switch_case(step_index, ci, true, cx)
            }),
        );
        let move_down = step_icon_btn(
            SharedString::from(format!("actions-case-down-{step_index}-{ci}")),
            Icon::ArrowDown,
            ci + 1 >= case_total,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.move_switch_case(step_index, ci, false, cx)
            }),
        );
        let remove = step_icon_btn(
            SharedString::from(format!("actions-case-del-{step_index}-{ci}")),
            Icon::Eraser,
            false,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.remove_switch_case(step_index, ci, cx)
            }),
        );

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(match_el)
            .child(drill)
            .child(move_up)
            .child(move_down)
            .child(remove)
            .into_any_element()
    }

    fn render_add_case(
        &self,
        step_index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("actions-add-case-{step_index}")))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.add_switch_case(step_index, cx)
                }),
            )
            .child(icon(Icon::Plus, BRANCH_GLYPH, palette.brand))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.brand)
                    .child("Add case"),
            )
            .into_any_element()
    }

    fn render_step_controls(
        &self,
        i: usize,
        total: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let menu_open = self.step_menu_open == Some(i);
        let view = cx.entity();

        let move_up = step_icon_btn(
            SharedString::from(format!("actions-step-up-{i}")),
            Icon::ArrowUp,
            i == 0,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_up(i, cx)),
        );
        let move_down = step_icon_btn(
            SharedString::from(format!("actions-step-down-{i}")),
            Icon::ArrowDown,
            i + 1 >= total,
            palette,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_down(i, cx)),
        );

        let menu = menu_button(Icon::DotsVertical, menu_open, palette)
            .placement(MenuPlacement::BottomRight)
            .items(vec![
                menu_item(
                    SharedString::from(format!("actions-step-edit-{i}")),
                    "Edit…",
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_edit_sub_action(i, cx)
                    }),
                )
                .icon(Icon::InfoCircle)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-dup-{i}")),
                    "Duplicate",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.duplicate_step(i, cx)),
                )
                .icon(Icon::Copy)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-top-{i}")),
                    "Move to top",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_top(i, cx)),
                )
                .icon(Icon::ArrowBarUp)
                .disabled(i == 0)
                .into(),
                menu_item(
                    SharedString::from(format!("actions-step-bottom-{i}")),
                    "Move to bottom",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.move_step_bottom(i, cx)),
                )
                .icon(Icon::ArrowBarDown)
                .disabled(i + 1 >= total)
                .into(),
                menu_divider(),
                menu_item(
                    SharedString::from(format!("actions-step-del-{i}")),
                    "Delete",
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_step(i, cx)),
                )
                .icon(Icon::Eraser)
                .color(palette.random)
                .into(),
            ])
            .on_toggle(
                SharedString::from(format!("actions-step-menu-{i}")),
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_step_menu(i, cx)),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_step_menu(cx));
            });

        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(move_up)
            .child(move_down)
            .child(menu)
            .into_any_element()
    }

    // --- render: edit-sub-action side sheet -------------------------------

    fn render_sub_action_modal(
        &self,
        form: &EditSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut fields_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(form.kind.label()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child("CONFIGURATION"),
            );
        if form.fields.is_empty() {
            fields_col = fields_col.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("This sub-action has no configuration."),
            );
        }
        for (spec, input) in &form.fields {
            fields_col = fields_col.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, Density::Cozy))
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_faint)
                            .child(spec.label),
                    )
                    .child(input.clone()),
            );
        }

        let body = div()
            .id("actions-sub-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(spacing(Spacing::Md, Density::Cozy))
            .px(spacing(Spacing::Md, Density::Cozy))
            .child(fields_col);

        let cancel = secondary_button("Cancel", palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
        );
        let save = primary_button("Save", palette).on_click(
            "actions-sub-submit",
            cx.listener(|this, _: &ClickEvent, _, cx| this.submit_sub_action(cx)),
        );
        let buttons = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(save);

        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(12.0))
            .px(px(16.0))
            .border_t(HALF_BORDER)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child("ESC to cancel"),
            )
            .child(buttons);

        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .child(body)
            .child(footer);

        let sheet = side_sheet(SUB_SHEET_W, content, palette)
            .position(SheetPosition::Right)
            .header("Edit sub-action")
            .on_close(
                "actions-sub-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
            );

        let view = cx.entity();
        overlay(sheet, palette)
            .position(OverlayPosition::Right(SUB_SHEET_W))
            .on_dismiss("actions-sub-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_sub_action(cx));
            })
            .into_any_element()
    }

    // --- render: add-action modal -----------------------------------------

    fn render_add_modal(
        &self,
        form: &AddActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name_len = form.name.read(cx).content().chars().count().min(NAME_LIMIT);
        let valid = !form.name.read(cx).content().trim().is_empty();

        // NAME: field + N/64 counter.
        let name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(div().flex_1().child(form.name.clone()))
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(format!("{name_len}/{NAME_LIMIT}")),
            );
        let name_section = modal_section(palette, "NAME", name_row);

        // GROUP: field led by a brand dot.
        let group_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(status_dot(palette.brand, GROUP_DOT))
            .child(div().flex_1().child(form.group.clone()));
        let group_section = modal_section(palette, "GROUP", group_field);

        // QUEUE: inline selectable chips (the kit carries no lightweight dropdown, so
        // the queue is chosen from chips — the Globals variant-kind picker approach).
        let mut queue_chips = div()
            .flex()
            .flex_wrap()
            .gap(spacing(Spacing::Xxs, Density::Cozy));
        for (i, name) in form.queue_options.iter().enumerate() {
            queue_chips = queue_chips.child(
                chip(
                    name.clone(),
                    ChipGlyph::Dot(palette.brand),
                    form.selected_queue == i,
                    palette,
                )
                .on_click(
                    SharedString::from(format!("actions-queue-{i}")),
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_queue(i, cx)),
                ),
            );
        }
        let queue_section = modal_section(palette, "QUEUE", queue_chips);

        let two_col = div()
            .flex()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(div().flex_1().child(group_section))
            .child(div().flex_1().child(queue_section));

        let desc_section = modal_section(
            palette,
            "DESCRIPTION",
            div().child(form.description.clone()),
        );

        let behavior_header = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child("BEHAVIOR");

        let enabled = self.modal_toggle_row(
            "Enabled",
            "Action runs when a trigger fires.",
            form.enabled,
            palette.success,
            "actions-modal-enabled",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_enabled(cx)),
            palette,
        );
        let concurrent = self.modal_toggle_row(
            "Concurrent execution",
            "Allow parallel runs in this queue.",
            form.concurrent,
            palette.info,
            "actions-modal-concurrent",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_concurrent(cx)),
            palette,
        );
        let bypass = self.modal_toggle_row(
            "Bypass queue pause",
            "Always run even if queue is paused.",
            form.bypass_pause,
            palette.warning,
            "actions-modal-bypass",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_bypass(cx)),
            palette,
        );
        let random = self.modal_toggle_row(
            "Random pick",
            "Run ONE random sub-action per trigger instead of all.",
            form.random_pick,
            palette.random,
            "actions-modal-random",
            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_modal_random(cx)),
            palette,
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(name_section)
            .child(two_col)
            .child(desc_section)
            .child(behavior_header)
            .child(enabled)
            .child(concurrent)
            .child(bypass)
            .child(random);

        let cancel = secondary_button("Cancel", palette).on_click(
            "actions-modal-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_add_modal(cx)),
        );
        let create = primary_button("Create action", palette)
            .disabled(!valid)
            .on_click(
                "actions-modal-create",
                cx.listener(|this, _: &ClickEvent, _, cx| this.submit_add_modal(cx)),
            );
        let footer = div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(cancel)
            .child(create);

        let card = modal("New action", body, palette)
            .size(ModalSize::Md)
            .footer(footer)
            .kbd_hint("ESC to cancel")
            .on_close(
                "actions-modal-close",
                cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_add_modal(cx)),
            );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-modal-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_add_modal(cx));
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn modal_toggle_row(
        &self,
        label: &'static str,
        description: &'static str,
        on: bool,
        accent: Rgba,
        id: &'static str,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        palette: &ForgePalette,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_primary)
                            .child(label),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XXS)
                            .text_color(palette.text_muted)
                            .child(description),
                    ),
            )
            .child(toggle(on, palette).on_color(accent).on_click(id, handler))
            .into_any_element()
    }

    // --- render: delete confirm -------------------------------------------

    fn render_delete_confirm(
        &self,
        id: ActionId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self.find(id).map(|a| a.name.clone()).unwrap_or_default();
        let card = confirm_modal(
            "Delete action?",
            "This will remove the action and all of its sub-actions and triggers.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "actions-delete-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "actions-delete-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }

    // --- render: trigger-link card + picker + unlink confirm --------------

    /// A trigger-link card: a leading dot + kind glyph, the name / kind / condition
    /// title cluster, and a trailing unlink `X` that arms the two-phase confirm. The
    /// card body carries the navigate-to-registry click (inert until that screen
    /// exists); the `X`'s own handler runs first, so a click on it unlinks without the
    /// inert navigate interfering.
    fn render_trigger_card(
        &self,
        index: usize,
        trigger: &SeededTrigger,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = if trigger.enabled {
            palette.brand
        } else {
            palette.disabled
        };
        let name_color = if trigger.enabled {
            palette.text_primary
        } else {
            palette.text_faint
        };

        let leading = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(status_dot(accent, TRIGGER_DOT))
            .child(icon(trigger.glyph, CARD_GLYPH, accent));

        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_XS)
                    .text_color(name_color)
                    .child(trigger.name.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(trigger.kind_label.clone()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.bits)
                    .child(trigger.condition.clone()),
            );

        let hover = palette.surface_overlay;
        let unlink = div()
            .id(SharedString::from(format!(
                "actions-trigger-unlink-{index}"
            )))
            .flex()
            .items_center()
            .justify_center()
            .size(STEP_BTN)
            .rounded(STEP_BTN_RADIUS)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.request_trigger_unlink(index, cx)
            }))
            .child(icon(Icon::X, CARD_GLYPH, palette.random));

        row_card(title, palette)
            .leading(leading)
            .trailing(unlink)
            .idle_background(palette.elevated)
            .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
            .on_click(
                SharedString::from(format!("actions-trigger-card-{index}")),
                cx.listener(|this, _: &ClickEvent, _, cx| this.trigger_card_clicked(cx)),
            )
            .into_any_element()
    }

    fn render_grid_picker(
        &self,
        form: &GridPickerForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = form.kind.accent(palette);
        let searching = !form.search.trim().is_empty();
        let query = form.search.trim().to_lowercase();

        // Scope + query filter: a live query overrides the scope; each group keeps
        // only its surviving cards, and empty groups drop out.
        let visible: Vec<GridGroup> = self
            .grid_groups(form, palette)
            .into_iter()
            .filter(|g| searching || form.scope.is_none() || form.scope.as_ref() == Some(&g.scope))
            .map(|mut g| {
                if searching {
                    g.items.retain(|it| {
                        it.name.to_lowercase().contains(&query)
                            || it.desc.to_lowercase().contains(&query)
                    });
                }
                g
            })
            .filter(|g| !g.items.is_empty())
            .collect();
        let total: usize = visible.iter().map(|g| g.items.len()).sum();

        let card = div()
            .w(GRID_W)
            .h(GRID_H)
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(palette.elevated)
            .rounded(radius(Radius::Lg))
            .border(BORDER_ACCENT)
            .border_color(palette.border_regular)
            .child(self.render_grid_header(form, accent, palette, cx))
            .child(self.render_grid_search(form, palette, cx))
            .children((!searching).then(|| self.render_grid_chips(form, palette, cx)))
            .child(self.render_grid_body(form, accent, visible, total, palette, cx))
            .child(render_grid_footer(form, palette));

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-grid-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_grid_picker(cx));
            })
            .into_any_element()
    }

    fn render_grid_header(
        &self,
        form: &GridPickerForm,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ctx_name = self
            .detail
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "this action".to_owned());
        let (count, sub_word) = match form.kind {
            PickerKind::Step => (SUB_KINDS.len(), "sub-actions"),
            PickerKind::Trigger => (form.trigger_entries.len(), "trigger types"),
        };

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(GRID_TILE)
            .rounded(GRID_TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(form.kind.header_icon(), GRID_TILE_ICON, accent));

        let subtitle = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .overflow_hidden()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(GRID_META_FS)
            .child(div().text_color(palette.text_faint).child(form.kind.ctx()))
            .child(div().text_color(accent).child(ctx_name))
            .child(
                div()
                    .text_color(palette.text_faint)
                    .child(format!("\u{b7} {count} {sub_word}")),
            );

        let titles = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(form.kind.title()),
            )
            .child(subtitle);

        let close = div()
            .id("actions-grid-close")
            .flex_none()
            .p(px(4.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_grid_picker(cx)))
            .child(icon(Icon::X, GRID_CLOSE_ICON, palette.text_faint));

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(GRID_HEADER_GAP)
            .py(GRID_HEADER_PAD_V)
            .px(GRID_BAND_PAD_H)
            .border_b(BORDER_ACCENT)
            .border_color(palette.surface_overlay)
            .child(tile)
            .child(titles)
            .child(close)
            .into_any_element()
    }

    fn render_grid_search(
        &self,
        form: &GridPickerForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(form.search_field.clone()),
            );
        if !form.search.is_empty() {
            row = row.child(
                div()
                    .id("actions-grid-search-clear")
                    .flex_none()
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear_grid_search(cx)))
                    .child(icon(Icon::X, GRID_SEARCH_ICON, palette.text_faint)),
            );
        }

        div()
            .flex_none()
            .pt(GRID_SEARCH_PAD_T)
            .pb(GRID_SEARCH_PAD_B)
            .px(GRID_BAND_PAD_H)
            .border_b(BORDER_ACCENT)
            .border_color(palette.surface_overlay)
            .child(row)
            .into_any_element()
    }

    fn render_grid_chips(
        &self,
        form: &GridPickerForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .id("actions-grid-chips")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .overflow_x_scroll()
            .child(grid_scope_chip(
                "actions-grid-scope-all",
                "All",
                None,
                form.scope.is_none(),
                palette,
                cx.listener(|this, _: &ClickEvent, _, cx| this.set_grid_scope(None, cx)),
            ));

        for (id, label, dot) in self.grid_scopes(form, palette) {
            let active = form.scope.as_ref() == Some(&id);
            let scope_id = id.clone();
            row = row.child(grid_scope_chip(
                SharedString::from(format!("actions-grid-scope-{id}")),
                label,
                Some(dot),
                active,
                palette,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_grid_scope(Some(scope_id.clone()), cx)
                }),
            ));
        }

        div()
            .flex_none()
            .py(GRID_CHIPS_PAD_V)
            .px(GRID_BAND_PAD_H)
            .border_b(BORDER_ACCENT)
            .border_color(palette.surface_overlay)
            .child(row)
            .into_any_element()
    }

    fn render_grid_body(
        &self,
        form: &GridPickerForm,
        accent: Rgba,
        visible: Vec<GridGroup>,
        total: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = form.search.trim().to_owned();
        let searching = !query.is_empty();
        let mut col = div().flex().flex_col();

        if searching {
            col = col.child(
                div()
                    .pb(spacing(Spacing::Sm, Density::Cozy))
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(GRID_META_FS)
                    .text_color(palette.text_faint)
                    .child(format!(
                        "{total} {} for \u{201c}{query}\u{201d}",
                        if total == 1 { "match" } else { "matches" },
                    )),
            );
        }

        if visible.is_empty() {
            col = col.child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(spacing(Spacing::Sm, Density::Cozy))
                    .py(GRID_EMPTY_PAD_V)
                    .child(icon(Icon::Search, GRID_EMPTY_GLYPH, palette.text_faint))
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(format!("Nothing matches \u{201c}{query}\u{201d}")),
                    ),
            );
        }

        for group in &visible {
            col = col.child(self.render_grid_group(group, accent, palette, cx));
        }

        div()
            .id("actions-grid-body")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .py(GRID_BODY_PAD_V)
            .px(GRID_BAND_PAD_H)
            .child(col)
            .into_any_element()
    }

    fn render_grid_group(
        &self,
        group: &GridGroup,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .pb(GRID_GROUP_HEADER_MB)
            .child(
                div()
                    .flex_none()
                    .size(GRID_GROUP_DOT)
                    .rounded(radius(Radius::Pill))
                    .bg(group.color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(GRID_GROUP_FS)
                    .text_color(palette.text_muted)
                    .child(group.label.to_uppercase()),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(GRID_GROUP_FS)
                    .text_color(palette.text_faint)
                    .child(group.items.len().to_string()),
            );

        let mut rows = div().flex().flex_col().gap(GRID_CARD_GAP);
        for chunk in group.items.chunks(2) {
            let mut pair = div().flex().gap(GRID_CARD_GAP);
            for item in chunk {
                pair = pair.child(self.render_grid_card(item, accent, palette, cx));
            }
            if chunk.len() == 1 {
                pair = pair.child(div().flex_1());
            }
            rows = rows.child(pair);
        }

        div()
            .w_full()
            .pb(GRID_GROUP_GAP)
            .child(header)
            .child(rows)
            .into_any_element()
    }

    fn render_grid_card(
        &self,
        item: &GridItem,
        accent: Rgba,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = item.id.clone();
        let hovered = self.grid_hover.as_ref() == Some(&id);
        let dim = !matches!(item.state, CardState::Add);
        let border = if hovered && !dim {
            palette.border_regular
        } else {
            palette.surface_overlay
        };

        let tile = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(GRID_CARD_TILE)
            .rounded(GRID_CARD_TILE_RADIUS)
            .bg(palette.surface_overlay)
            .child(icon(item.glyph, GRID_CARD_ICON, item.color));

        let name = div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .font_family(DEFAULT_BODY_FAMILY)
            .font_weight(FontWeight::MEDIUM)
            .text_size(GRID_CARD_NAME_FS)
            .text_color(palette.text_primary)
            .child(item.name.clone());

        let trailing: AnyElement = match item.state {
            CardState::Added => badge(
                palette.surface_overlay,
                palette.success,
                "added",
                true,
                GRID_BADGE_FS,
            )
            .into_any_element(),
            CardState::Off => badge(
                palette.surface_overlay,
                palette.text_faint,
                "off",
                true,
                GRID_BADGE_FS,
            )
            .into_any_element(),
            CardState::Add => {
                let tint = if hovered { accent } else { palette.text_faint };
                icon(Icon::Plus, GRID_CARD_ICON, tint).into_any_element()
            }
        };

        let top = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .pb(GRID_CARD_ROW_MB)
            .child(tile)
            .child(name)
            .child(trailing);

        let desc = div()
            .overflow_hidden()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(GRID_META_FS)
            .text_color(palette.text_muted)
            .child(item.desc.clone());

        let card = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .py(GRID_CARD_PAD_V)
            .px(GRID_CARD_PAD_H)
            .rounded(radius(Radius::Md))
            .border(BORDER_ACCENT)
            .border_color(border)
            .bg(palette.shell)
            .child(top)
            .child(desc);

        if dim {
            return card.opacity(0.5).into_any_element();
        }

        let pick = item.pick.clone();
        card.id(id.clone())
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_grid_hover(id.clone(), *hovered, cx)
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.grid_apply_pick(pick.clone(), cx)
            }))
            .into_any_element()
    }

    fn grid_apply_pick(&mut self, pick: GridPick, cx: &mut Context<Self>) {
        match pick {
            GridPick::Step(kind) => self.grid_pick_step(kind, cx),
            GridPick::Trigger(seed) => self.grid_pick_trigger(
                SeededTrigger {
                    name: seed.name,
                    kind_label: seed.kind_label,
                    condition: seed.condition,
                    glyph: seed.glyph,
                    enabled: seed.enabled,
                },
                cx,
            ),
        }
    }

    fn grid_groups(&self, form: &GridPickerForm, palette: &ForgePalette) -> Vec<GridGroup> {
        match form.kind {
            PickerKind::Step => build_step_groups(palette),
            PickerKind::Trigger => {
                build_trigger_groups(&form.trigger_entries, self.detail.as_ref(), palette)
            }
        }
    }

    fn grid_scopes(
        &self,
        form: &GridPickerForm,
        palette: &ForgePalette,
    ) -> Vec<(SharedString, String, Rgba)> {
        let cap = match form.kind {
            PickerKind::Trigger => 6,
            PickerKind::Step => 7,
        };
        let mut seen: Vec<(SharedString, String, Rgba)> = Vec::new();
        for g in self.grid_groups(form, palette) {
            if g.scope.as_ref() == "all" || seen.iter().any(|(id, _, _)| id == &g.scope) {
                continue;
            }
            seen.push((g.scope.clone(), scope_label(&g.label), g.color));
            if seen.len() >= cap {
                break;
            }
        }
        seen
    }

    fn render_trigger_unlink_confirm(
        &self,
        index: usize,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = self
            .detail
            .as_ref()
            .and_then(|d| d.triggers.get(index))
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let card = confirm_modal(
            "Delete trigger link?",
            "This item will be permanently removed. This action cannot be undone.",
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(name)
        .esc_hint("to cancel")
        .on_cancel(
            "actions-trigger-unlink-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_trigger_unlink(cx)),
        )
        .on_confirm(
            "actions-trigger-unlink-confirm",
            "Delete",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_trigger_unlink(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("actions-trigger-unlink-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_trigger_unlink(cx));
            })
            .into_any_element()
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
        let grid_picker = self
            .grid_picker
            .as_ref()
            .map(|form| self.render_grid_picker(form, &palette, cx));
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

// ── view-specific fragments ───────────────────────────────────────────────

/// A left-panel notice line (loading / empty), inked `color`, padded like a group
/// header.
fn tree_notice(label: &'static str, color: Rgba, _palette: &ForgePalette) -> impl IntoElement {
    div()
        .w_full()
        .px(TREE_GUTTER)
        .py(spacing(Spacing::Sm, Density::Cozy))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(color)
        .child(label)
}

/// A modal form block: an uppercase mono caption over `control`.
fn modal_section(
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

// ── seeded stub state ─────────────────────────────────────────────────────

/// The representative action tree the screen seeds before an actions repo is wired,
/// mirroring the design's sample groups so every filter tab, both enabled states, the
/// collapsed-group affordance and the count/menu right slot render populated.
fn seed_groups(mint: &mut impl FnMut() -> ActionId) -> Vec<ActionGroup> {
    let mut action = |name: &str, enabled: bool, sub: usize| ActionSummary {
        id: mint(),
        name: name.to_owned(),
        enabled,
        sub_action_count: sub,
    };
    vec![
        ActionGroup {
            name: "CHAT COMMANDS".into(),
            category: ActionCategory::Chat,
            collapsed: false,
            actions: vec![
                action("!so", true, 7),
                action("!quote", true, 5),
                action("!followage", true, 3),
                action("!stats", false, 5),
                action("!commands", true, 1),
                action("!uptime", true, 2),
                action("!discord", true, 1),
            ],
        },
        ActionGroup {
            name: "TIMERS".into(),
            category: ActionCategory::Timers,
            collapsed: false,
            actions: vec![
                action("SocialReminder", true, 2),
                action("HydrateCheck", true, 1),
                action("GoalProgress", true, 3),
            ],
        },
        ActionGroup {
            name: "CHANNEL POINTS".into(),
            category: ActionCategory::Points,
            collapsed: true,
            actions: vec![
                action("TTS Boost", true, 2),
                action("Hydrate Me", true, 1),
                action("PauseTimer", true, 3),
                action("BangerMode", true, 5),
            ],
        },
    ]
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

/// Step count in the branch a drill-in frame would enter, used to gate descending
/// past the depth cap into an empty branch.
fn branch_count(step: &EditorStep, chain_key: &str, case_index: Option<usize>) -> usize {
    match case_index {
        None => step
            .branches
            .iter()
            .find(|b| b.key == chain_key)
            .map(|b| b.steps.len())
            .unwrap_or(0),
        Some(ci) => step
            .cases
            .as_ref()
            .and_then(|cases| cases.get(ci))
            .map(|c| c.chain.len())
            .unwrap_or(0),
    }
}

/// Human label for a single-sub-chain branch key in the breadcrumb.
fn branch_field_label(chain_key: &str) -> &'static str {
    match chain_key {
        "then_chain" => "Then",
        "else_chain" => "Else",
        "body" => "Body",
        "default_chain" => "Default",
        _ => "Branch",
    }
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

fn platform_group_for(kind_id: &str) -> PlatformGroup {
    if kind_id.starts_with("twitch.") {
        PlatformGroup::Twitch
    } else if kind_id.starts_with("youtube.") {
        PlatformGroup::YouTube
    } else if kind_id.starts_with("kick.") {
        PlatformGroup::Kick
    } else if kind_id.starts_with("obs.") {
        PlatformGroup::Obs
    } else if kind_id.starts_with("vtube.") {
        PlatformGroup::VTube
    } else if kind_id.starts_with("midi.") {
        PlatformGroup::Midi
    } else if kind_id.starts_with("hotkey.") {
        PlatformGroup::Hotkey
    } else if kind_id.starts_with("discord.") {
        PlatformGroup::Discord
    } else if kind_id.starts_with("script.") {
        PlatformGroup::Script
    } else {
        PlatformGroup::Core
    }
}

/// The second `kind_id` segment title-cased into a subgroup label, mirroring the
/// registry's grouping (`obs.scenes.current_changed` → "Scenes").
fn sub_group_label_for(kind_id: &str) -> String {
    let Some(segment) = kind_id.split('.').nth(1) else {
        return "Other".to_owned();
    };
    segment
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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

/// The representative catalog seeded before a trigger registry is wired: a few
/// platforms, each carrying one or two kinds with a default and an occasional custom
/// instance, so every grid group and both the default / saved cards render populated.
fn seed_picker_entries() -> Vec<PickerEntry> {
    let counter = std::cell::Cell::new(0u64);
    let id = || {
        let v = counter.get();
        counter.set(v + 1);
        v
    };
    let entry = |kind_id: &'static str,
                 label: &'static str,
                 desc: &'static str,
                 customs: Vec<PickerCustom>| PickerEntry {
        kind_id,
        label,
        desc,
        sub_group: sub_group_label_for(kind_id),
        default_id: id(),
        customs,
    };
    vec![
        entry(
            "twitch.chat.command",
            "Chat command",
            "Fires on a !command in chat",
            vec![PickerCustom {
                id: id(),
                name: "!hello",
                override_summary: "command=!hello",
                enabled: true,
            }],
        ),
        entry(
            "twitch.support.subscriber",
            "New subscriber",
            "A new paid subscription",
            vec![
                PickerCustom {
                    id: id(),
                    name: "VIP sub alert",
                    override_summary: "tier=3000",
                    enabled: true,
                },
                PickerCustom {
                    id: id(),
                    name: "Gift-bomb alert",
                    override_summary: "min gifts=5",
                    enabled: false,
                },
            ],
        ),
        entry(
            "twitch.points.reward",
            "Channel point reward",
            "A channel-point reward redeemed",
            Vec::new(),
        ),
        entry(
            "youtube.chat.message",
            "Chat message",
            "Every message posted in chat",
            Vec::new(),
        ),
        entry(
            "youtube.support.member",
            "New member",
            "A new channel membership",
            Vec::new(),
        ),
        entry(
            "kick.chat.command",
            "Chat command",
            "Fires on a !command in chat",
            Vec::new(),
        ),
        entry(
            "obs.scenes.current_changed",
            "Scene changed",
            "Active scene switched",
            Vec::new(),
        ),
        entry(
            "obs.stream.started",
            "Stream started",
            "OBS started streaming",
            Vec::new(),
        ),
        entry(
            "core.timer.tick",
            "Timer tick",
            "Every N minutes while live",
            Vec::new(),
        ),
    ]
}

// ── unified grid picker data ────────────────────────────────────────────────

/// Which add flow the unified grid picker drives.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PickerKind {
    Step,
    Trigger,
}

impl PickerKind {
    fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            PickerKind::Step => palette.brand,
            PickerKind::Trigger => palette.warning,
        }
    }

    fn header_icon(self) -> Icon {
        match self {
            PickerKind::Step => Icon::LayoutGrid,
            PickerKind::Trigger => Icon::Bolt,
        }
    }

    fn title(self) -> &'static str {
        match self {
            PickerKind::Step => "Add sub-action",
            PickerKind::Trigger => "Add trigger",
        }
    }

    fn ctx(self) -> &'static str {
        match self {
            PickerKind::Step => "Inserting into",
            PickerKind::Trigger => "Fires",
        }
    }
}

/// The open unified "Add" grid picker: which flow it drives, the action it targets,
/// the live search field + query, the active scope chip, and (trigger flow only) the
/// seeded catalog it groups.
struct GridPickerForm {
    kind: PickerKind,
    action_id: ActionId,
    search_field: Entity<TextInput>,
    search: String,
    scope: Option<SharedString>,
    trigger_entries: Vec<PickerEntry>,
    _search_sub: Subscription,
}

/// A card's addability state in the grid.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CardState {
    Add,
    Added,
    Off,
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

/// One selectable card in the grid.
struct GridItem {
    id: SharedString,
    name: String,
    desc: String,
    glyph: Icon,
    color: Rgba,
    state: CardState,
    pick: GridPick,
}

/// A titled group of cards under one category (steps) or platform · subgroup
/// (triggers).
struct GridGroup {
    label: String,
    color: Rgba,
    scope: SharedString,
    items: Vec<GridItem>,
}

/// The seeded sub-action catalog as grid groups, one per [`SubCategory`] in
/// first-seen order.
fn build_step_groups(palette: &ForgePalette) -> Vec<GridGroup> {
    let mut groups: Vec<GridGroup> = Vec::new();
    for kind in SUB_KINDS {
        let cat = kind.category();
        let scope = SharedString::from(cat.slug());
        let color = cat.color(palette);
        let item = GridItem {
            id: SharedString::from(format!("step-{}", kind.slug())),
            name: kind.label().to_owned(),
            desc: kind.summary_hint().to_owned(),
            glyph: kind.glyph(),
            color,
            state: CardState::Add,
            pick: GridPick::Step(kind),
        };
        match groups.iter_mut().find(|g| g.scope == scope) {
            Some(g) => g.items.push(item),
            None => groups.push(GridGroup {
                label: cat.label().to_owned(),
                color,
                scope,
                items: vec![item],
            }),
        }
    }
    groups
}

/// The seeded trigger catalog as grid groups: a leading "Your saved triggers" group
/// from the custom instances (cards flagged `Added` when already linked), then one
/// group per platform · subgroup of default kinds.
fn build_trigger_groups(
    entries: &[PickerEntry],
    detail: Option<&ActionDetail>,
    palette: &ForgePalette,
) -> Vec<GridGroup> {
    let linked: Vec<&str> = detail
        .map(|d| d.triggers.iter().map(|t| t.name.as_str()).collect())
        .unwrap_or_default();

    let mut groups: Vec<GridGroup> = Vec::new();

    let mut saved: Vec<GridItem> = Vec::new();
    for entry in entries {
        let group = platform_group_for(entry.kind_id);
        for custom in &entry.customs {
            let added = linked.contains(&custom.name);
            let state = if !custom.enabled {
                CardState::Off
            } else if added {
                CardState::Added
            } else {
                CardState::Add
            };
            saved.push(GridItem {
                id: SharedString::from(format!("trig-custom-{}", custom.id)),
                name: custom.name.to_owned(),
                desc: custom.override_summary.to_owned(),
                glyph: group.glyph(),
                color: group.color(palette),
                state,
                pick: GridPick::Trigger(TriggerSeed {
                    name: custom.name.to_owned(),
                    kind_label: entry.label.to_owned(),
                    condition: custom.override_summary.to_owned(),
                    glyph: group.glyph(),
                    enabled: true,
                }),
            });
        }
    }
    if !saved.is_empty() {
        groups.push(GridGroup {
            label: "Your saved triggers".to_owned(),
            color: palette.bits,
            scope: SharedString::from("all"),
            items: saved,
        });
    }

    for entry in entries {
        let group = platform_group_for(entry.kind_id);
        let scope = SharedString::from(group.key());
        let label = format!("{} \u{b7} {}", group.label(), entry.sub_group);
        let item = GridItem {
            id: SharedString::from(format!("trig-default-{}", entry.default_id)),
            name: entry.label.to_owned(),
            desc: entry.desc.to_owned(),
            glyph: group.glyph(),
            color: group.color(palette),
            state: CardState::Add,
            pick: GridPick::Trigger(TriggerSeed {
                name: entry.label.to_owned(),
                kind_label: group.label().to_owned(),
                condition: String::new(),
                glyph: group.glyph(),
                enabled: true,
            }),
        };
        match groups.iter_mut().find(|g| g.label == label) {
            Some(g) => g.items.push(item),
            None => groups.push(GridGroup {
                label,
                color: group.color(palette),
                scope,
                items: vec![item],
            }),
        }
    }

    groups
}

/// The scope-chip label for a group: the segment before the ` · ` platform /
/// category separator (the whole label when there is none).
fn scope_label(group_label: &str) -> String {
    group_label
        .split(" \u{b7} ")
        .next()
        .unwrap_or(group_label)
        .to_owned()
}

/// A scope filter chip: active pills fill `surface_overlay` with a `border_regular`
/// outline; inactive ones stay transparent. An optional leading category dot leads
/// the label.
fn grid_scope_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    dot: Option<Rgba>,
    active: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let (bg, text_color, border): (Rgba, Rgba, Rgba) = if active {
        (
            palette.surface_overlay,
            palette.text_primary,
            palette.border_regular,
        )
    } else {
        (
            gpui::transparent_black().into(),
            palette.text_secondary,
            gpui::transparent_black().into(),
        )
    };
    let mut chip = div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(GRID_CHIP_DOT)
        .py(GRID_CHIP_PAD_V)
        .px(GRID_CHIP_PAD_H)
        .rounded(radius(Radius::Pill))
        .border(BORDER_ACCENT)
        .border_color(border)
        .bg(bg)
        .cursor_pointer()
        .on_click(handler);
    if let Some(dot) = dot {
        chip = chip.child(
            div()
                .flex_none()
                .size(GRID_CHIP_DOT)
                .rounded(radius(Radius::Pill))
                .bg(dot),
        );
    }
    chip.child(
        div()
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(GRID_META_FS)
            .text_color(text_color)
            .child(label.into()),
    )
    .into_any_element()
}

/// The grid modal's footer band: a per-kind hint on the left, an `Esc` chip on the
/// right.
fn render_grid_footer(form: &GridPickerForm, palette: &ForgePalette) -> impl IntoElement {
    let hint = match form.kind {
        PickerKind::Step => "Added with smart defaults \u{2014} edit inline after",
        PickerKind::Trigger => "Pick a trigger \u{2014} configure it in the Triggers registry",
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .py(GRID_FOOTER_PAD_V)
        .px(GRID_BAND_PAD_H)
        .bg(palette.shell)
        .border_t(BORDER_ACCENT)
        .border_color(palette.surface_overlay)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(hint),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xxs, Density::Cozy))
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .py(GRID_KBD_PAD_V)
                        .px(GRID_KBD_PAD_H)
                        .rounded(GRID_KBD_RADIUS)
                        .bg(palette.surface_overlay)
                        .child("Esc"),
                )
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XXS)
                        .text_color(palette.text_faint)
                        .child("to cancel"),
                ),
        )
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

/// Builds the config inputs for `kind`, each seeded from `seed` (an existing step's
/// config when editing, or the kind's defaults when adding).
fn build_sub_fields(
    kind: SubKind,
    seed: &BTreeMap<String, String>,
    palette: ForgePalette,
    cx: &mut Context<ScreenActionsView>,
) -> Vec<(&'static SubField, Entity<TextInput>)> {
    kind.fields()
        .iter()
        .map(|spec| {
            let value = seed.get(spec.key).cloned().unwrap_or_default();
            let placeholder = spec.placeholder;
            let input = cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx).with_palette(palette);
                if !value.is_empty() {
                    input.set_content(value, cx);
                }
                input
            });
            (spec, input)
        })
        .collect()
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

/// Splits `s` into `(chunk, is_variable)` runs, marking `%name%` interpolation tokens
/// (leading letter/underscore, then alphanumerics/`_`/`.`) so the caller can two-tone
/// them.
fn parse_variable_segments(s: &str) -> Vec<(&str, bool)> {
    let bytes = s.as_bytes();
    let mut segs: Vec<(&str, bool)> = Vec::new();
    let mut plain_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let var_start = i + 1;
            let mut j = var_start;
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                j += 1;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
                {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'%' && j > var_start {
                    if plain_start < i {
                        segs.push((&s[plain_start..i], false));
                    }
                    segs.push((&s[i..j + 1], true));
                    i = j + 1;
                    plain_start = i;
                    continue;
                }
            }
        }
        i += 1;
    }
    if plain_start < s.len() {
        segs.push((&s[plain_start..], false));
    }
    segs
}

/// Renders a summary line with `%variable%` tokens tinted `warning` and plain text
/// tinted `text_muted`, wrapping like the source's flowed mono row.
fn variable_text(s: &str, palette: &ForgePalette) -> AnyElement {
    if s.is_empty() {
        return div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child(String::new())
            .into_any_element();
    }
    let mut row = div()
        .flex()
        .flex_wrap()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS);
    for (chunk, is_var) in parse_variable_segments(s) {
        let color = if is_var {
            palette.warning
        } else {
            palette.text_muted
        };
        row = row.child(div().text_color(color).child(chunk.to_owned()));
    }
    row.into_any_element()
}

/// Full-width, centered "Add …" button closing a section (triggers / sub-actions):
/// the deep-panel fill, an accent icon + label and a thin hairline, washing
/// `surface_overlay` on hover.
fn add_row_button(
    id: impl Into<ElementId>,
    glyph: Icon,
    label: &'static str,
    accent: Rgba,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let hover = palette.surface_overlay;
    div()
        .id(id.into())
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(CARD_PAD_V)
        .px(CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .bg(palette.shell)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(handler)
        .child(icon(glyph, CARD_GLYPH, accent))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(accent)
                .child(label),
        )
        .into_any_element()
}

/// A centered, hairline-framed empty-state card for a section with no rows.
fn empty_placeholder_card(
    glyph: Icon,
    glyph_color: Rgba,
    label: &'static str,
    palette: &ForgePalette,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .py(EMPTY_CARD_PAD_V)
        .px(EMPTY_CARD_PAD_H)
        .rounded(radius(Radius::Md))
        .border(HALF_BORDER)
        .border_color(palette.border_input)
        .child(icon(glyph, EMPTY_CARD_GLYPH, glyph_color))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(label),
        )
        .into_any_element()
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

/// A drill-in chip entering a nested sub-chain: a "label · count" caption + a chevron,
/// framed by a 0.5px hairline with a 6px corner, washing `surface_overlay` on hover.
/// Disabled (past the depth cap on an empty branch) it inks `disabled` and takes no
/// click.
fn drill_in_chip(
    id: impl Into<ElementId>,
    label: &str,
    count: usize,
    disabled: bool,
    palette: &ForgePalette,
    handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let color = if disabled {
        palette.disabled
    } else {
        palette.brand
    };
    let base = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, Density::Cozy))
        .py(spacing(Spacing::Xxs, Density::Cozy))
        .px(spacing(Spacing::Xs, Density::Cozy))
        .rounded(CHIP_RADIUS)
        .border(HALF_BORDER)
        .border_color(palette.border_regular)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(color)
                .child(format!("{label} \u{00b7} {count}")),
        )
        .child(icon(Icon::ChevronRight, BRANCH_GLYPH, color));
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
