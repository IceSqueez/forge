use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, ChipGlyph, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY,
    Density, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, InputEvent, MenuPlacement, ModalSize,
    OverlayPosition, Radius, Spacing, TextArea, TextInput, breadcrumb, chip, confirm_modal, icon,
    menu_button, menu_divider, menu_item, modal, overlay, primary_button, primary_button_with_icon,
    search_input, secondary_button, spacing, status_dot, toggle,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Pixels, Rgba, SharedString, Subscription, Window, div,
    prelude::*, px,
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

    // --- render: right editor pane (empty placeholder) --------------------

    fn render_editor_pane(&self, palette: &ForgePalette) -> AnyElement {
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
        let editor = self.render_editor_pane(&palette);

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

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(body)
            .children(add_modal)
            .children(delete_modal)
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
