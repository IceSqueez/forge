use std::collections::BTreeMap;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_LG, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement,
    ModalSize, OverlayPosition, Radius, SheetPosition, Spacing, TextArea, TextInput, breadcrumb,
    chip, confirm_modal, ghost_button_with_icon, icon, menu_button, menu_divider, menu_item, modal,
    overlay, primary_button, primary_button_with_icon, radius, row_card, search_input,
    secondary_button, side_sheet, spacing, status_dot, toggle,
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
/// Right side-sheet width for the add-sub-action panel (fixed 480px seed in the source).
const SUB_SHEET_W: Pixels = px(480.0);

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
    sub_form: Option<AddSubActionForm>,
    step_menu_open: Option<usize>,
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
        self.step_menu_open = None;
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
    /// summary borrow ends before the mutable group iteration begins.
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

    fn move_step(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut()
            && from < detail.steps.len()
            && to < detail.steps.len()
            && from != to
        {
            let step = detail.steps.remove(from);
            detail.steps.insert(to, step);
        }
        self.step_menu_open = None;
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
        let last = self.detail.as_ref().map(|d| d.steps.len()).unwrap_or(0);
        if last > 0 {
            self.move_step(i, last - 1, cx);
        }
    }

    fn duplicate_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut()
            && let Some(src) = detail.steps.get(i)
        {
            let clone = EditorStep {
                kind: src.kind,
                config: src.config.clone(),
            };
            detail.steps.insert(i + 1, clone);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        cx.notify();
    }

    fn remove_step(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.detail.as_mut()
            && i < detail.steps.len()
        {
            detail.steps.remove(i);
        }
        self.sync_selected_count();
        self.step_menu_open = None;
        cx.notify();
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

    // --- editor: add-sub-action side sheet --------------------------------

    fn open_sub_action(
        &mut self,
        editing: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let search_field = cx.new(|cx| search_input("Search sub-actions...", palette, cx));
        let search_sub = cx.subscribe(&search_field, Self::on_sub_search_event);

        let mut mode = SubFormStep::PickKind;
        let mut selected_kind = None;
        let mut fields: Vec<(&'static SubField, Entity<TextInput>)> = Vec::new();
        if let Some(i) = editing
            && let Some(step) = self.detail.as_ref().and_then(|d| d.steps.get(i))
        {
            let kind = step.kind;
            let seed = step.config.clone();
            fields = build_sub_fields(kind, &seed, palette, cx);
            selected_kind = Some(kind);
            mode = SubFormStep::FillForm;
        }

        if editing.is_none() {
            search_field.read(cx).focus(window);
        }
        self.step_menu_open = None;
        self.sub_form = Some(AddSubActionForm {
            mode,
            search_field,
            search: String::new(),
            selected_kind,
            fields,
            editing_index: editing,
            _search_sub: search_sub,
        });
        cx.notify();
    }

    fn on_sub_search_event(
        &mut self,
        _field: Entity<TextInput>,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        if let InputEvent::Changed(text) = event
            && let Some(form) = self.sub_form.as_mut()
        {
            form.search = text.to_string();
            cx.notify();
        }
    }

    fn pick_sub_kind(&mut self, kind: SubKind, cx: &mut Context<Self>) {
        let palette = cx.palette();
        let seed = kind.seed_config();
        let fields = build_sub_fields(kind, &seed, palette, cx);
        if let Some(form) = self.sub_form.as_mut() {
            form.selected_kind = Some(kind);
            form.fields = fields;
            form.mode = SubFormStep::FillForm;
        }
        cx.notify();
    }

    fn back_to_kind_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.sub_form.as_mut() {
            form.mode = SubFormStep::PickKind;
            form.selected_kind = None;
            form.fields.clear();
        }
        cx.notify();
    }

    fn cancel_sub_action(&mut self, cx: &mut Context<Self>) {
        self.sub_form = None;
        cx.notify();
    }

    fn submit_sub_action(&mut self, cx: &mut Context<Self>) {
        let (kind, editing, fields) = {
            let Some(form) = self.sub_form.as_ref() else {
                return;
            };
            let Some(kind) = form.selected_kind else {
                return;
            };
            (kind, form.editing_index, form.fields.clone())
        };

        let mut config = BTreeMap::new();
        for (spec, input) in &fields {
            config.insert(spec.key.to_owned(), input.read(cx).content().to_owned());
        }
        let step = EditorStep { kind, config };

        if let Some(detail) = self.detail.as_mut() {
            match editing {
                Some(i) if i < detail.steps.len() => detail.steps[i] = step,
                _ => detail.steps.push(step),
            }
        }
        self.sync_selected_count();
        self.sub_form = None;
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
            for trigger in &detail.triggers {
                col = col.child(render_trigger_card(trigger, palette));
            }
        }
        // Spawn C wires the trigger picker; the button renders in its final visual but
        // is intentionally inert here.
        col = col.child(add_row_button(
            "actions-add-trigger",
            Icon::Plus,
            "Add trigger",
            palette.warning,
            palette,
            cx.listener(|_this, _: &ClickEvent, _, cx| cx.notify()),
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
        let header = div().flex().items_center().child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(format!("{} sub-actions", detail.steps.len())),
        );

        let total = detail.steps.len();
        let mut steps_col = div().flex().flex_col();
        if detail.steps.is_empty() {
            steps_col = steps_col.child(empty_placeholder_card(
                Icon::Plus,
                palette.brand,
                "This action has no steps yet",
                palette,
            ));
        }
        for (i, step) in detail.steps.iter().enumerate() {
            steps_col = steps_col.child(self.render_step_block(step, i, total, palette, cx));
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
                        this.open_sub_action(None, window, cx)
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

        div()
            .w_full()
            .pb(if is_last { px(0.0) } else { STEP_GAP })
            .child(step_row)
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
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_sub_action(Some(i), window, cx)
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

    // --- render: add-sub-action side sheet --------------------------------

    fn render_sub_action_modal(
        &self,
        form: &AddSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = if form.editing_index.is_some() {
            "Edit sub-action"
        } else {
            "Add sub-action"
        };

        let body_inner = match form.mode {
            SubFormStep::PickKind => self.render_kind_picker(form, palette, cx),
            SubFormStep::FillForm => self.render_kind_form(form, palette, cx),
        };
        let body = div()
            .id("actions-sub-scroll")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .child(body_inner);

        let cancel = secondary_button("Cancel", palette).on_click(
            "actions-sub-cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_sub_action(cx)),
        );
        let buttons = if matches!(form.mode, SubFormStep::FillForm) {
            let label = if form.editing_index.is_some() {
                "Save"
            } else {
                "Add sub-action"
            };
            let valid = form.selected_kind.is_some();
            div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, Density::Cozy))
                .child(cancel)
                .child(primary_button(label, palette).disabled(!valid).on_click(
                    "actions-sub-submit",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.submit_sub_action(cx)),
                ))
                .into_any_element()
        } else {
            div().flex().child(cancel).into_any_element()
        };

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
            .header(title)
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

    fn render_kind_picker(
        &self,
        form: &AddSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search_lower = form.search.to_lowercase();

        let header = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Select a sub-action type"),
            )
            .child(form.search_field.clone());

        let mut list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, Density::Cozy));
        let mut any = false;
        for kind in SUB_KINDS {
            if !search_lower.is_empty()
                && !kind.label().to_lowercase().contains(&search_lower)
                && !kind.summary_hint().to_lowercase().contains(&search_lower)
            {
                continue;
            }
            any = true;
            list = list.child(render_kind_row(kind, palette, cx));
        }
        if !any {
            list = list.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("No matching sub-actions"),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(header)
            .child(list)
            .into_any_element()
    }

    fn render_kind_form(
        &self,
        form: &AddSubActionForm,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind_label = form.selected_kind.map(SubKind::label).unwrap_or("");

        let back = div()
            .id("actions-sub-back")
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.back_to_kind_picker(cx)))
            .child(icon(Icon::ArrowBackUp, CARD_GLYPH, palette.text_secondary))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child("Back"),
            );

        let header = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(back)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(kind_label),
            );

        let mut fields_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
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

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Sm, Density::Cozy))
            .child(header)
            .child(fields_col)
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
}

const SUB_KINDS: [SubKind; 9] = [
    SubKind::SendChat,
    SubKind::Speak,
    SubKind::PlaySound,
    SubKind::SetGlobal,
    SubKind::RandomInt,
    SubKind::Delay,
    SubKind::Log,
    SubKind::ReadFile,
    SubKind::SubAction,
];

/// One editable config entry a sub-action kind exposes in the add-sub-action form.
struct SubField {
    key: &'static str,
    label: &'static str,
    placeholder: &'static str,
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
        }
    }

    fn accent(self, palette: &ForgePalette) -> Rgba {
        match self {
            SubKind::SendChat => palette.brand,
            SubKind::Speak | SubKind::PlaySound => palette.success,
            SubKind::SetGlobal | SubKind::RandomInt => palette.warning,
            SubKind::ReadFile => palette.random,
            SubKind::Delay | SubKind::Log | SubKind::SubAction => palette.info,
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
        };
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
}

/// A single sub-action step in the editor chain: its kind plus a string-keyed config
/// bag the summary line and the edit form read.
struct EditorStep {
    kind: SubKind,
    config: BTreeMap<String, String>,
}

impl EditorStep {
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
        }
    }
}

/// A read-only trigger link shown under the editor's TRIGGERS section (spawn C wires
/// the interactive picker, unlink and drill-in).
struct SeededTrigger {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubFormStep {
    PickKind,
    FillForm,
}

/// The open add/edit-sub-action side sheet. Its config inputs are child [`TextInput`]
/// entities owning their own edit state; the picker search, the selected kind and the
/// edit index are plain fields.
struct AddSubActionForm {
    mode: SubFormStep,
    search_field: Entity<TextInput>,
    search: String,
    selected_kind: Option<SubKind>,
    fields: Vec<(&'static SubField, Entity<TextInput>)>,
    editing_index: Option<usize>,
    _search_sub: Subscription,
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
    let steps = (0..summary.sub_action_count)
        .map(|i| {
            let kind = ORDER[i % ORDER.len()];
            EditorStep {
                kind,
                config: kind.seed_config(),
            }
        })
        .collect();

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

/// A read-only trigger-link card: a leading dot + kind glyph, the name / kind /
/// condition title cluster, and a faint (inert) unlink glyph.
fn render_trigger_card(trigger: &SeededTrigger, palette: &ForgePalette) -> AnyElement {
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

    let unlink = div()
        .flex()
        .items_center()
        .justify_center()
        .size(STEP_BTN)
        .child(icon(Icon::X, CARD_GLYPH, palette.text_faint));

    row_card(title, palette)
        .leading(leading)
        .trailing(unlink)
        .idle_background(palette.elevated)
        .bordered(palette.border_regular, BORDER_THIN, radius(Radius::Md))
        .into_any_element()
}

/// A sub-action-kind picker row: a leading accent dot, the kind label and its
/// one-line summary, selecting the kind on click.
fn render_kind_row(
    kind: SubKind,
    palette: &ForgePalette,
    cx: &mut Context<ScreenActionsView>,
) -> AnyElement {
    let title = div()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(kind.label());
    let meta = div()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_faint)
        .child(kind.summary_hint());

    row_card(title, palette)
        .leading(status_dot(kind.accent(palette), TRIGGER_DOT))
        .meta(meta)
        .on_click(
            SharedString::from(format!("actions-kind-{}", kind.label())),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_sub_kind(kind, cx)),
        )
        .into_any_element()
}
