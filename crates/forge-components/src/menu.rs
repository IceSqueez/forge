use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Corner, Div, ElementId, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, anchored, deferred,
    div, point, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, Radius,
    Spacing, radius, spacing,
};

/// Fixed panel width. The source pins the dropdown to a literal 200px regardless of
/// item content, so it is carried as a literal rather than snapped onto the spacing
/// scale.
const PANEL_WIDTH: Pixels = px(200.0);

/// Side of the square trigger affordance. The offsets that park the panel against the
/// trigger's bottom/top edge are all measured from this fixed size, matching the
/// source's `28px` trigger box.
const TRIGGER_SIZE: Pixels = px(28.0);

/// Draw priority for the deferred popover pass. Any positive value lifts the panel and
/// its click-away backdrop above ordinary sibling content painted in the same frame.
const MENU_PRIORITY: usize = 1;

/// Resolves a spacing token at the density-neutral `Cozy` multiplier. The dropdown is
/// chrome — sized once, not rescaled per instance — so every inset snaps to the
/// `Spacing` scale at `Cozy`, mirroring [`crate::modal`].
fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// Boxed per-item click handler. Mirrors the button family: gpui hands the click event
/// plus the window and app contexts, through which the caller reaches its own entity.
type ItemClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Boxed trigger-toggle handler. Fires only while the menu is closed (an open menu's
/// full-window backdrop covers the trigger, so a click on it dismisses instead of
/// re-toggling — see [`MenuButton`]).
type ToggleHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Boxed dismiss callback. Event-free: one callback answers the backdrop click, the
/// Escape press and every item activation, so it takes only the two contexts through
/// which the caller reaches its own entity to hide the menu. Shared (`Rc`) because it
/// is wired into several listeners in one render.
type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

/// A single actionable row in the dropdown: a label, an optional leading icon, an
/// optional trailing keyboard-shortcut hint, an optional ink override (a danger row
/// passes `palette.random`), a disabled flag and the click handler. Build one with
/// [`menu_item`], then layer the optional parts on through the builder methods, and
/// hand it to [`MenuButton::items`] (it converts into [`MenuItem::Item`]).
pub struct MenuEntry {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    shortcut: Option<SharedString>,
    color: Option<Rgba>,
    disabled: bool,
    on_click: ItemClick,
}

/// Builds an actionable dropdown row keyed by `id` (gpui needs a stable identity to
/// promote the row to a clickable element), labelled `label`, firing `on_click` when
/// activated. Defaults to icon-less, shortcut-less, default-ink and enabled; layer
/// those on through the builder methods.
pub fn menu_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> MenuEntry {
    MenuEntry {
        id: id.into(),
        label: label.into(),
        icon: None,
        shortcut: None,
        color: None,
        disabled: false,
        on_click: Box::new(on_click),
    }
}

impl MenuEntry {
    /// Adds a leading glyph, inked with the row's icon ink (or the color override).
    #[must_use]
    pub fn icon(mut self, glyph: Icon) -> Self {
        self.icon = Some(glyph);
        self
    }

    /// Adds a trailing monospace keyboard-shortcut hint, inked faint.
    #[must_use]
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Overrides the row's label and icon ink — pass `palette.random` for a
    /// destructive (danger) row. Left unset, the label inks `text_primary` and the
    /// icon `text_secondary`.
    #[must_use]
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    /// Renders the row in its dimmed, inert state: faint ink, no hover feedback and no
    /// click handling regardless of the handler.
    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One line in the dropdown: an actionable [`MenuEntry`], a horizontal rule between
/// groups, or a muted monospace section caption.
pub enum MenuItem {
    Item(MenuEntry),
    Divider,
    Header(SharedString),
}

impl From<MenuEntry> for MenuItem {
    fn from(entry: MenuEntry) -> Self {
        MenuItem::Item(entry)
    }
}

/// A horizontal rule separating item groups.
pub fn menu_divider() -> MenuItem {
    MenuItem::Divider
}

/// A muted monospace section caption.
pub fn menu_header(label: impl Into<SharedString>) -> MenuItem {
    MenuItem::Header(label.into())
}

/// Counts the actionable rows in `items` — the enabled [`MenuItem::Item`]s, excluding
/// dividers, headers and disabled rows. Lets a caller decide whether a menu is worth
/// showing at all.
pub fn actionable_count(items: &[MenuItem]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, MenuItem::Item(entry) if !entry.disabled))
        .count()
}

/// Which trigger edge the panel drops from, and whether it aligns to the trigger's
/// leading or trailing edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuPlacement {
    /// Drops below the trigger, left edges aligned.
    BottomLeft,
    /// Drops below the trigger, right edges aligned.
    BottomRight,
    /// Rises above the trigger, left edges aligned.
    TopLeft,
    /// Rises above the trigger, right edges aligned.
    TopRight,
}

impl MenuPlacement {
    /// The panel corner pinned to the anchor point and the offset (measured from the
    /// trigger's top-left) that lands that anchor point on the desired trigger edge.
    /// The trigger's top-left is the anchored element's own layout origin, so a
    /// `Bottom*` placement offsets down by the trigger height and a `*Right` placement
    /// offsets right by the trigger width.
    fn anchor_and_offset(self) -> (Corner, Point<Pixels>) {
        match self {
            MenuPlacement::BottomLeft => (Corner::TopLeft, point(px(0.0), TRIGGER_SIZE)),
            MenuPlacement::BottomRight => (Corner::TopRight, point(TRIGGER_SIZE, TRIGGER_SIZE)),
            MenuPlacement::TopLeft => (Corner::BottomLeft, point(px(0.0), px(0.0))),
            MenuPlacement::TopRight => (Corner::BottomRight, point(TRIGGER_SIZE, px(0.0))),
        }
    }
}

/// An overflow / context menu: a square icon trigger that toggles a floating, anchored
/// dropdown of [`MenuItem`]s. The dropdown is a fixed-width `elevated` panel bordered
/// with `border_input` and rounded, lifted above content in a deferred pass and parked
/// against the trigger by [`MenuPlacement`].
///
/// The component is stateless — the caller owns the `open` flag and passes it in, wires
/// [`MenuButton::on_toggle`] to flip it open and [`MenuButton::on_dismiss`] to clear it.
/// While open, a transparent full-window backdrop sits under the panel: a click that
/// misses the panel (including one on the trigger) lands on the backdrop and dismisses,
/// so the trigger cannot re-toggle a menu shut and back open in one gesture. Each item
/// click fires the item's handler and then dismisses; [`MenuButton::dismiss_on_escape`]
/// additionally routes Escape to dismissal.
///
/// Build one with [`menu_button`], then layer on `.items`, `.placement`, `.on_toggle`,
/// `.on_dismiss` and `.dismiss_on_escape`.
#[derive(IntoElement)]
pub struct MenuButton {
    trigger_icon: Icon,
    open: bool,
    placement: MenuPlacement,
    items: Vec<MenuItem>,
    trigger_id: ElementId,
    on_toggle: Option<ToggleHandler>,
    on_dismiss: Option<DismissHandler>,
    escape_focus: Option<FocusHandle>,
    trigger_ink: Rgba,
    trigger_hover_bg: Rgba,
    panel_bg: Rgba,
    panel_border: Rgba,
    divider_ink: Rgba,
    item_hover_bg: Rgba,
    header_ink: Rgba,
    label_ink: Rgba,
    icon_ink: Rgba,
    faint_ink: Rgba,
}

/// Builds an overflow menu whose trigger shows `trigger_icon` (typically
/// [`Icon::DotsVertical`]), reflecting the caller-owned `open` flag, resolving every
/// ink from `palette` up front so the built value carries no palette borrow. Defaults
/// to [`MenuPlacement::BottomRight`], empty, with no handlers wired; layer those on
/// through the builder methods.
pub fn menu_button(trigger_icon: Icon, open: bool, palette: &ForgePalette) -> MenuButton {
    MenuButton {
        trigger_icon,
        open,
        placement: MenuPlacement::BottomRight,
        items: Vec::new(),
        trigger_id: ElementId::Name(SharedString::new_static("forge-menu-trigger")),
        on_toggle: None,
        on_dismiss: None,
        escape_focus: None,
        trigger_ink: palette.text_faint,
        trigger_hover_bg: palette.surface_overlay,
        panel_bg: palette.elevated,
        panel_border: palette.border_input,
        divider_ink: palette.border_regular,
        item_hover_bg: palette.surface_overlay,
        header_ink: palette.text_muted,
        label_ink: palette.text_primary,
        icon_ink: palette.text_secondary,
        faint_ink: palette.text_faint,
    }
}

impl MenuButton {
    /// Sets the dropdown rows (default: empty).
    #[must_use]
    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    /// Sets which trigger edge the panel drops from (default
    /// [`MenuPlacement::BottomRight`]).
    #[must_use]
    pub fn placement(mut self, placement: MenuPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Wires the trigger click that opens the menu. gpui needs a stable [`ElementId`]
    /// to promote the trigger to a clickable element (and to keep several menus'
    /// triggers distinct); the `handler` mutates the caller's entity through the passed
    /// contexts to flip the `open` flag. The handler fires only while the menu is
    /// closed — an open menu's backdrop intercepts the trigger and dismisses.
    #[must_use]
    pub fn on_toggle(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.trigger_id = id.into();
        self.on_toggle = Some(Box::new(handler));
        self
    }

    /// Wires dismissal — the callback the backdrop click, each item activation and
    /// (once [`MenuButton::dismiss_on_escape`] is set) Escape all invoke to clear the
    /// caller's `open` flag.
    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    /// Routes Escape to the [`MenuButton::on_dismiss`] handler. gpui delivers key
    /// events only down the focus path, so the panel tracks `focus_handle` and the
    /// caller must focus it when the menu opens; without a focused handle the backdrop
    /// click still dismisses but Escape stays inert.
    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }

    /// Builds the square icon trigger. It carries the `surface_overlay` wash while the
    /// menu is open and on hover; the open wash shows through the transparent backdrop
    /// so the trigger stays visibly active.
    fn render_trigger(&mut self) -> AnyElement {
        let mut trigger = div()
            .flex()
            .items_center()
            .justify_center()
            .size(TRIGGER_SIZE)
            .rounded(radius(Radius::Sm))
            .child(icon(self.trigger_icon, FONT_SM, self.trigger_ink));

        if self.open {
            trigger = trigger.bg(self.trigger_hover_bg);
        }

        let hover_bg = self.trigger_hover_bg;
        let mut trigger = trigger
            .id(self.trigger_id.clone())
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg));

        if let Some(handler) = self.on_toggle.take() {
            trigger = trigger.on_click(handler);
        }

        trigger.into_any_element()
    }

    /// Builds one actionable row: an optional leading icon, the fill label and an
    /// optional trailing shortcut hint inside a padded, `Spacing::Sm`-gapped row that
    /// washes `surface_overlay` on hover. A disabled row inks faint, drops the hover
    /// wash and takes no click; an enabled row fires its handler then dismisses.
    fn render_item(&self, entry: MenuEntry) -> AnyElement {
        let text_ink = if entry.disabled {
            self.faint_ink
        } else {
            entry.color.unwrap_or(self.label_ink)
        };
        let glyph_ink = if entry.disabled {
            self.faint_ink
        } else {
            entry.color.unwrap_or(self.icon_ink)
        };

        let mut row = div()
            .flex()
            .items_center()
            .w_full()
            .gap(pad(Spacing::Sm))
            .py(pad(Spacing::Xs))
            .px(pad(Spacing::Sm))
            .rounded(radius(Radius::Sm))
            .font_family(DEFAULT_BODY_FAMILY);

        if let Some(glyph) = entry.icon {
            row = row.child(icon(glyph, FONT_SM, glyph_ink));
        }

        row = row.child(
            div()
                .flex_1()
                .text_size(FONT_SM)
                .text_color(text_ink)
                .child(entry.label),
        );

        if let Some(shortcut) = entry.shortcut {
            row = row.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(self.faint_ink)
                    .child(shortcut),
            );
        }

        if entry.disabled {
            return row.into_any_element();
        }

        let hover_bg = self.item_hover_bg;
        let handler = entry.on_click;
        let dismiss = self.on_dismiss.clone();
        row.hover(move |style| style.bg(hover_bg))
            .id(entry.id)
            .cursor_pointer()
            .on_click(move |event, window, cx| {
                handler(event, window, cx);
                if let Some(dismiss) = &dismiss {
                    dismiss(window, cx);
                }
            })
            .into_any_element()
    }

    /// Builds one non-actionable line: a group rule or a section caption.
    fn render_divider(&self) -> AnyElement {
        div()
            .py(pad(Spacing::Xs) * 0.5)
            .child(div().w_full().h(BORDER_THIN).bg(self.divider_ink))
            .into_any_element()
    }

    fn render_header(&self, label: SharedString) -> AnyElement {
        div()
            .pt(pad(Spacing::Xs))
            .pb(pad(Spacing::Xs) * 0.5)
            .px(pad(Spacing::Sm))
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(self.header_ink)
            .child(label)
            .into_any_element()
    }

    /// Builds the floating panel: the bordered `elevated` surface stacking the item
    /// lines, occluding the mouse (so a click on it is swallowed, not treated as a
    /// dismiss), wired for click-away and — when a focus handle is present — Escape.
    fn render_panel(&mut self) -> Div {
        let items = std::mem::take(&mut self.items);
        let mut panel = div()
            .flex()
            .flex_col()
            .w(PANEL_WIDTH)
            .py(pad(Spacing::Xs))
            .bg(self.panel_bg)
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(self.panel_border)
            .occlude();

        for item in items {
            panel = match item {
                MenuItem::Item(entry) => panel.child(self.render_item(entry)),
                MenuItem::Divider => panel.child(self.render_divider()),
                MenuItem::Header(label) => panel.child(self.render_header(label)),
            };
        }

        if let (Some(focus), Some(dismiss)) = (self.escape_focus.as_ref(), self.on_dismiss.clone())
        {
            panel =
                panel
                    .track_focus(focus)
                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                        if event.keystroke.key.as_str() == "escape" {
                            dismiss(window, cx);
                        }
                    });
        }

        panel
    }
}

impl RenderOnce for MenuButton {
    fn render(mut self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let trigger = self.render_trigger();

        // The wrapper is the trigger's positioning context: the anchored panel takes
        // this element's top-left as its anchor origin, so the placement offsets land
        // the panel against the trigger's edges.
        let mut root = div().relative().child(trigger);

        if !self.open {
            return root;
        }

        let (anchor_corner, offset) = self.placement.anchor_and_offset();
        let panel = self.render_panel();

        // A transparent full-window backdrop under the panel catches every click that
        // misses the panel — including one on the trigger, which it covers — and
        // dismisses. Sizing it to the viewport and anchoring it to the window origin
        // keeps it full-window regardless of how deep in the tree the menu renders.
        let viewport = window.viewport_size();
        let mut backdrop = div().size_full().occlude();
        if let Some(dismiss) = self.on_dismiss.clone() {
            backdrop =
                backdrop.on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    dismiss(window, cx);
                });
        }
        let backdrop_layer = anchored()
            .position_mode(gpui::AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .anchor(Corner::TopLeft)
            .child(div().w(viewport.width).h(viewport.height).child(backdrop));

        let panel_layer = anchored()
            .anchor(anchor_corner)
            .offset(offset)
            .snap_to_window()
            .child(panel);

        root = root.child(
            deferred(div().child(backdrop_layer).child(panel_layer)).with_priority(MENU_PRIORITY),
        );

        root
    }
}
