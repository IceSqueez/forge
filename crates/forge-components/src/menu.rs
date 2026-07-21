use std::rc::Rc;

use gpui::{
    Anchor, AnchoredPositionMode, AnyElement, App, ClickEvent, Div, ElementId, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement,
    Pixels, Point, RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window,
    anchored, deferred, div, point, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS, Radius,
    Spacing, radius, spacing,
};

const PANEL_WIDTH: Pixels = px(200.0);

const TRIGGER_SIZE: Pixels = px(28.0);

const MENU_PRIORITY: usize = 1;

fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

type ItemClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

type ToggleHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

pub struct MenuEntry {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    shortcut: Option<SharedString>,
    color: Option<Rgba>,
    disabled: bool,
    on_click: ItemClick,
}

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
    #[must_use]
    pub fn icon(mut self, glyph: Icon) -> Self {
        self.icon = Some(glyph);
        self
    }

    #[must_use]
    pub fn color(mut self, color: Rgba) -> Self {
        self.color = Some(color);
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

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

pub fn menu_divider() -> MenuItem {
    MenuItem::Divider
}

pub fn menu_header(label: impl Into<SharedString>) -> MenuItem {
    MenuItem::Header(label.into())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuPlacement {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

impl MenuPlacement {
    fn anchor_and_offset(self) -> (Anchor, Point<Pixels>) {
        match self {
            MenuPlacement::BottomLeft => (Anchor::TopLeft, point(px(0.0), TRIGGER_SIZE)),
            MenuPlacement::BottomRight => (Anchor::TopRight, point(TRIGGER_SIZE, TRIGGER_SIZE)),
            MenuPlacement::TopLeft => (Anchor::BottomLeft, point(px(0.0), px(0.0))),
            MenuPlacement::TopRight => (Anchor::BottomRight, point(TRIGGER_SIZE, px(0.0))),
        }
    }
}

#[derive(Clone, Copy)]
struct MenuInk {
    panel_bg: Rgba,
    panel_border: Rgba,
    divider_ink: Rgba,
    item_hover_bg: Rgba,
    header_ink: Rgba,
    label_ink: Rgba,
    icon_ink: Rgba,
    faint_ink: Rgba,
}

impl MenuInk {
    fn from_palette(palette: &ForgePalette) -> Self {
        Self {
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

    fn render_item(&self, entry: MenuEntry, dismiss: &Option<DismissHandler>) -> AnyElement {
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
        let dismiss = dismiss.clone();
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

    fn render_panel(
        &self,
        items: Vec<MenuItem>,
        escape_focus: Option<&FocusHandle>,
        dismiss: Option<DismissHandler>,
    ) -> Div {
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
                MenuItem::Item(entry) => panel.child(self.render_item(entry, &dismiss)),
                MenuItem::Divider => panel.child(self.render_divider()),
                MenuItem::Header(label) => panel.child(self.render_header(label)),
            };
        }

        if let (Some(focus), Some(dismiss)) = (escape_focus, dismiss) {
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

#[derive(IntoElement)]
pub struct MenuButton {
    trigger_icon: Icon,
    open: bool,
    anchor_at: Option<Point<Pixels>>,
    placement: MenuPlacement,
    items: Vec<MenuItem>,
    trigger_id: ElementId,
    on_toggle: Option<ToggleHandler>,
    on_dismiss: Option<DismissHandler>,
    escape_focus: Option<FocusHandle>,
    trigger_ink: Rgba,
    trigger_hover_bg: Rgba,
    ink: MenuInk,
}

pub fn menu_button(trigger_icon: Icon, open: bool, palette: &ForgePalette) -> MenuButton {
    MenuButton {
        trigger_icon,
        open,
        anchor_at: None,
        placement: MenuPlacement::BottomRight,
        items: Vec::new(),
        trigger_id: ElementId::Name(SharedString::new_static("forge-menu-trigger")),
        on_toggle: None,
        on_dismiss: None,
        escape_focus: None,
        trigger_ink: palette.text_faint,
        trigger_hover_bg: palette.surface_overlay,
        ink: MenuInk::from_palette(palette),
    }
}

impl MenuButton {
    #[must_use]
    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    #[must_use]
    pub fn placement(mut self, placement: MenuPlacement) -> Self {
        self.placement = placement;
        self
    }

    #[must_use]
    pub fn open_at(mut self, position: Option<Point<Pixels>>) -> Self {
        self.anchor_at = position;
        self
    }

    /// Fires only while the menu is closed; while open the backdrop intercepts the
    /// trigger and dismisses instead.
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

    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    /// Caller must focus this handle when the menu opens, or Escape stays inert (gpui
    /// routes keys only down the focus path).
    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }

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
}

impl RenderOnce for MenuButton {
    fn render(mut self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let trigger = self.render_trigger();

        let mut root = div().relative().child(trigger);

        if !self.open {
            return root;
        }

        let (anchor_corner, offset) = self.placement.anchor_and_offset();
        let panel = self.ink.render_panel(
            std::mem::take(&mut self.items),
            self.escape_focus.as_ref(),
            self.on_dismiss.clone(),
        );

        let viewport = window.viewport_size();
        let mut backdrop = div().size_full().occlude();
        if let Some(dismiss) = self.on_dismiss.clone() {
            backdrop =
                backdrop.on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    dismiss(window, cx);
                });
        }
        let backdrop_layer = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .anchor(Anchor::TopLeft)
            .child(div().w(viewport.width).h(viewport.height).child(backdrop));

        let panel_layer = match self.anchor_at {
            Some(position) => anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(position)
                .anchor(anchor_corner)
                .snap_to_window()
                .child(panel),
            None => anchored()
                .anchor(anchor_corner)
                .offset(offset)
                .snap_to_window()
                .child(panel),
        };

        root = root.child(
            deferred(div().child(backdrop_layer).child(panel_layer)).with_priority(MENU_PRIORITY),
        );

        root
    }
}

#[derive(IntoElement)]
pub struct ContextMenu {
    position: Point<Pixels>,
    items: Vec<MenuItem>,
    on_dismiss: Option<DismissHandler>,
    ink: MenuInk,
}

pub fn context_menu(position: Point<Pixels>, palette: &ForgePalette) -> ContextMenu {
    ContextMenu {
        position,
        items: Vec::new(),
        on_dismiss: None,
        ink: MenuInk::from_palette(palette),
    }
}

impl ContextMenu {
    #[must_use]
    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let panel = self
            .ink
            .render_panel(self.items, None, self.on_dismiss.clone());

        let viewport = window.viewport_size();
        let mut backdrop = div().size_full().occlude();
        if let Some(dismiss) = self.on_dismiss.clone() {
            let dismiss_right = dismiss.clone();
            backdrop = backdrop
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    dismiss(window, cx);
                })
                .on_mouse_down(MouseButton::Right, move |_: &MouseDownEvent, window, cx| {
                    dismiss_right(window, cx);
                });
        }
        let backdrop_layer = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(point(px(0.0), px(0.0)))
            .anchor(Anchor::TopLeft)
            .child(div().w(viewport.width).h(viewport.height).child(backdrop));

        let panel_layer = anchored()
            .position_mode(AnchoredPositionMode::Window)
            .position(self.position)
            .anchor(Anchor::TopLeft)
            .snap_to_window()
            .child(panel);

        deferred(div().child(backdrop_layer).child(panel_layer)).with_priority(MENU_PRIORITY)
    }
}
