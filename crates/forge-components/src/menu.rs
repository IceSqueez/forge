use crate::FONT_MICRO;
use crate::FONT_SECTION_LABEL;
use crate::key_activation::KeyActivated;
use crate::presentation::ActivePresentation;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Anchor, AnchoredPositionMode, AnimationExt, AnyElement, App, ClickEvent, Div, ElementId,
    Entity, FocusHandle, InteractiveElement, IntoElement, KeyBinding, KeyDownEvent,
    KeyboardClickEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, RenderOnce,
    Rgba, SharedString, StatefulInteractiveElement, Styled, Window, actions, anchored, deferred,
    div, point, px,
};

use crate::icons::{Icon, icon};
use crate::overlay::{drive_overlay_focus, enter_animation};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DISABLED_OPACITY, Density, FONT_XS, Spacing, body_family, mono_family, scaled,
    spacing,
};

const PANEL_MIN_WIDTH: Pixels = px(180.0);
const PANEL_RADIUS: Pixels = px(8.0);
const PANEL_GAP: Pixels = px(4.0);

const ITEM_GAP: Pixels = px(9.0);
const ITEM_RADIUS: Pixels = px(5.0);
const HEADER_BOTTOM_PAD_RATIO: f32 = 0.5;
const ITEM_ICON_SIZE: Pixels = px(13.0);
const SHORTCUT_FONT_SIZE: Pixels = FONT_MICRO;
const HEADER_FONT_SIZE: Pixels = FONT_SECTION_LABEL;

const TRIGGER_SIZE: Pixels = px(20.0);
const TRIGGER_ICON_SIZE: Pixels = px(14.0);
const TRIGGER_RADIUS: Pixels = px(5.0);

const MENU_PRIORITY: usize = 1;
const MENU_KEY_CONTEXT: &str = "ForgeMenu";

actions!(forge_menu, [MenuSelectPrev, MenuSelectNext, MenuConfirm]);

pub fn bind_menu_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MenuSelectPrev, Some(MENU_KEY_CONTEXT)),
        KeyBinding::new("down", MenuSelectNext, Some(MENU_KEY_CONTEXT)),
        KeyBinding::new("enter", MenuConfirm, Some(MENU_KEY_CONTEXT)),
    ]);
}

type ItemClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

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
        on_click: Rc::new(on_click),
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
            MenuPlacement::BottomLeft => {
                (Anchor::TopLeft, point(px(0.0), TRIGGER_SIZE + PANEL_GAP))
            }
            MenuPlacement::BottomRight => (
                Anchor::TopRight,
                point(TRIGGER_SIZE, TRIGGER_SIZE + PANEL_GAP),
            ),
            MenuPlacement::TopLeft => (Anchor::BottomLeft, point(px(0.0), -PANEL_GAP)),
            MenuPlacement::TopRight => (Anchor::BottomRight, point(TRIGGER_SIZE, -PANEL_GAP)),
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
            header_ink: palette.text_faint,
            label_ink: palette.text_secondary,
            icon_ink: palette.text_secondary,
            faint_ink: palette.text_faint,
        }
    }

    fn render_item(
        &self,
        entry: MenuEntry,
        at_key_cursor: bool,
        dismiss: &Option<DismissHandler>,
        density: Density,
    ) -> AnyElement {
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
            .gap(scaled(ITEM_GAP, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(ITEM_RADIUS)
            .font_family(body_family());

        if let Some(glyph) = entry.icon {
            row = row.child(icon(glyph, ITEM_ICON_SIZE, glyph_ink));
        }

        row = row.child(
            div()
                .flex_grow_1()
                .whitespace_nowrap()
                .text_size(FONT_XS)
                .text_color(text_ink)
                .child(entry.label),
        );

        if let Some(shortcut) = entry.shortcut {
            row = row.child(
                div()
                    .font_family(mono_family())
                    .whitespace_nowrap()
                    .text_size(SHORTCUT_FONT_SIZE)
                    .text_color(self.faint_ink)
                    .child(shortcut),
            );
        }

        if entry.disabled {
            return row.opacity(DISABLED_OPACITY).into_any_element();
        }

        let hover_bg = self.item_hover_bg;
        let handler = entry.on_click;
        let dismiss = dismiss.clone();
        if at_key_cursor {
            row = row.bg(hover_bg);
        }
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

    fn render_divider(&self, density: Density) -> AnyElement {
        div()
            .py(spacing(Spacing::Xxs, density))
            .child(div().w_full().h(BORDER_THIN).bg(self.divider_ink))
            .into_any_element()
    }

    fn render_header(&self, label: SharedString, density: Density) -> AnyElement {
        div()
            .pt(spacing(Spacing::Xs, density))
            .pb(spacing(Spacing::Xs, density) * HEADER_BOTTOM_PAD_RATIO)
            .px(spacing(Spacing::Sm, density))
            .font_family(mono_family())
            .whitespace_nowrap()
            .text_size(HEADER_FONT_SIZE)
            .text_color(self.header_ink)
            .child(label)
            .into_any_element()
    }

    fn render_panel(
        &self,
        items: Vec<MenuItem>,
        keyboard: Option<MenuKeyboard>,
        dismiss: Option<DismissHandler>,
        density: Density,
        cx: &App,
    ) -> Div {
        let mut panel = div()
            .flex()
            .flex_col()
            .min_w(PANEL_MIN_WIDTH)
            .p(spacing(Spacing::Xxs, density))
            .bg(self.panel_bg)
            .rounded(PANEL_RADIUS)
            .border(BORDER_THIN)
            .border_color(self.panel_border)
            .occlude();

        let key_cursor = keyboard
            .as_ref()
            .and_then(|keyboard| keyboard.cursor.read(cx).0);
        let mut pickable: Vec<Option<ItemClick>> = Vec::new();
        for item in items {
            panel = match item {
                MenuItem::Item(entry) => {
                    let at_key_cursor = key_cursor == Some(pickable.len());
                    pickable.push((!entry.disabled).then(|| Rc::clone(&entry.on_click)));
                    panel.child(self.render_item(entry, at_key_cursor, &dismiss, density))
                }
                MenuItem::Divider => panel.child(self.render_divider(density)),
                MenuItem::Header(label) => panel.child(self.render_header(label, density)),
            };
        }

        if let (Some(keyboard), Some(dismiss)) = (keyboard, dismiss) {
            panel = keyboard.attach(panel, pickable, dismiss);
        }

        panel
    }
}

#[derive(Default)]
struct MenuKeyCursor(Option<usize>);

impl MenuKeyCursor {
    fn step(&mut self, pickable: &[bool], forward: bool) {
        let count = pickable.len();
        let start = match (self.0, forward) {
            (Some(current), _) => current,
            (None, true) => count.saturating_sub(1),
            (None, false) => 0,
        };
        let distance = |hop: usize| if forward { hop } else { count - hop };
        self.0 = (1..=count)
            .map(|hop| (start + distance(hop)) % count)
            .find(|index| pickable.get(*index).copied().unwrap_or(false));
    }
}

struct MenuKeyboard {
    focus: FocusHandle,
    cursor: Entity<MenuKeyCursor>,
}

impl MenuKeyboard {
    fn attach(self, panel: Div, pickable: Vec<Option<ItemClick>>, dismiss: DismissHandler) -> Div {
        let enabled: Rc<[bool]> = pickable.iter().map(Option::is_some).collect();
        let (prev_cursor, next_cursor, confirm_cursor) =
            (self.cursor.clone(), self.cursor.clone(), self.cursor);
        let dismiss_on_escape = Rc::clone(&dismiss);
        let enabled_for_prev = Rc::clone(&enabled);
        panel
            .track_focus(&self.focus)
            .key_context(MENU_KEY_CONTEXT)
            .on_action(move |_: &MenuSelectPrev, _, cx| {
                prev_cursor.update(cx, |cursor, cx| {
                    cursor.step(&enabled_for_prev, false);
                    cx.notify();
                });
            })
            .on_action(move |_: &MenuSelectNext, _, cx| {
                next_cursor.update(cx, |cursor, cx| {
                    cursor.step(&enabled, true);
                    cx.notify();
                });
            })
            .on_action(move |_: &MenuConfirm, window, cx| {
                let handler = confirm_cursor
                    .read(cx)
                    .0
                    .and_then(|index| pickable.get(index).cloned().flatten());
                if let Some(handler) = handler {
                    let event = ClickEvent::Keyboard(KeyboardClickEvent::default());
                    handler(&event, window, cx);
                    dismiss(window, cx);
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == crate::tokens::ESCAPE_KEY {
                    cx.stop_propagation();
                    dismiss_on_escape(window, cx);
                }
            })
    }
}

struct MenuFocus {
    handle: FocusHandle,
    restore: Option<FocusHandle>,
}

impl MenuFocus {
    fn drive(focus: &Entity<Self>, open: bool, window: &mut Window, cx: &mut App) {
        focus.update(cx, |state, cx| {
            drive_overlay_focus(open, &state.handle, &mut state.restore, window, cx);
        });
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

    fn menu_focus(&self, window: &mut Window, cx: &mut App) -> Entity<MenuFocus> {
        window.use_keyed_state(
            ElementId::NamedChild(
                Arc::new(self.trigger_id.clone()),
                SharedString::new_static("focus"),
            ),
            cx,
            |_, cx| MenuFocus {
                handle: cx.focus_handle(),
                restore: None,
            },
        )
    }

    fn render_trigger(&mut self) -> AnyElement {
        let mut trigger = div()
            .flex()
            .items_center()
            .justify_center()
            .size(TRIGGER_SIZE)
            .rounded(TRIGGER_RADIUS)
            .child(icon(self.trigger_icon, TRIGGER_ICON_SIZE, self.trigger_ink));

        if self.open {
            trigger = trigger.bg(self.trigger_hover_bg);
        }

        let hover_bg = self.trigger_hover_bg;
        let mut trigger = trigger
            .id(self.trigger_id.clone())
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg));

        if let Some(handler) = self.on_toggle.take() {
            trigger = trigger.key_activated(self.trigger_ink).on_click(handler);
        }

        trigger.into_any_element()
    }
}

impl RenderOnce for MenuButton {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let focus = self.menu_focus(window, cx);
        MenuFocus::drive(&focus, self.open, window, cx);
        let trigger = self.render_trigger();

        let mut root = div().relative().child(trigger);

        if !self.open {
            return root;
        }

        let (anchor_corner, offset) = self.placement.anchor_and_offset();
        let panel_anim_id = ElementId::NamedChild(
            Arc::new(self.trigger_id.clone()),
            SharedString::new_static("panel"),
        );
        let dismiss = self.on_dismiss.clone().map(|dismiss| {
            let focus = focus.clone();
            let release_then_dismiss: DismissHandler = Rc::new(move |window, cx| {
                MenuFocus::drive(&focus, false, window, cx);
                dismiss(window, cx);
            });
            release_then_dismiss
        });
        let focus_handle = focus.read(cx).handle.clone();
        let cursor = window.use_keyed_state(
            ElementId::NamedChild(
                Arc::new(self.trigger_id.clone()),
                SharedString::new_static("key-cursor"),
            ),
            cx,
            |_, _| MenuKeyCursor::default(),
        );
        let panel = self
            .ink
            .render_panel(
                std::mem::take(&mut self.items),
                Some(MenuKeyboard {
                    focus: focus_handle,
                    cursor,
                }),
                dismiss.clone(),
                density,
                cx,
            )
            .with_animation(panel_anim_id, enter_animation(), |el, delta| {
                el.opacity(delta)
            });

        let viewport = window.viewport_size();
        let mut backdrop = div().size_full().occlude();
        if let Some(dismiss) = dismiss {
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
    escape_focus: Option<FocusHandle>,
    ink: MenuInk,
}

pub fn context_menu(position: Point<Pixels>, palette: &ForgePalette) -> ContextMenu {
    ContextMenu {
        position,
        items: Vec::new(),
        on_dismiss: None,
        escape_focus: None,
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

    #[must_use]
    pub fn dismiss_on_escape(mut self, focus_handle: &FocusHandle) -> Self {
        self.escape_focus = Some(focus_handle.clone());
        self
    }
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let keyboard = self.escape_focus.map(|focus| MenuKeyboard {
            focus,
            cursor: window.use_keyed_state("forge-context-menu-key-cursor", cx, |_, _| {
                MenuKeyCursor::default()
            }),
        });
        let panel = self
            .ink
            .render_panel(self.items, keyboard, self.on_dismiss.clone(), density, cx)
            .with_animation(
                ElementId::Name(SharedString::new_static("forge-context-menu-panel")),
                enter_animation(),
                |el, delta| el.opacity(delta),
            );

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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{Context, Render, TestAppContext, VisualTestContext};

    use super::*;
    use crate::palette::{FORGE_DEFAULT, ThemeId};
    use crate::presentation::Presentation;

    #[test]
    fn the_key_cursor_wraps_and_skips_what_cannot_be_picked() {
        const ROWS: [bool; 4] = [false, true, false, true];
        for (start, forward, expected) in [
            (None, true, Some(1)),
            (None, false, Some(3)),
            (Some(1), true, Some(3)),
            (Some(3), true, Some(1)),
            (Some(1), false, Some(3)),
            (Some(3), false, Some(1)),
        ] {
            let mut cursor = MenuKeyCursor(start);
            cursor.step(&ROWS, forward);
            assert_eq!(cursor.0, expected, "{start:?} {forward}");
        }
    }

    #[test]
    fn a_menu_with_nothing_to_pick_keeps_no_key_cursor() {
        for rows in [&[][..], &[false, false][..]] {
            let mut cursor = MenuKeyCursor::default();
            cursor.step(rows, true);
            assert_eq!(cursor.0, None);
        }
    }

    struct MenuHost {
        focus: FocusHandle,
        open: bool,
        picked: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Render for MenuHost {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            if self.open && !self.focus.is_focused(window) {
                window.focus(&self.focus, cx);
            }
            let view = cx.entity();
            let pick = |id: &'static str, picked: &Rc<RefCell<Vec<&'static str>>>| {
                let picked = Rc::clone(picked);
                move |_: &ClickEvent, _: &mut Window, _: &mut App| picked.borrow_mut().push(id)
            };
            let menu = self.open.then(|| {
                context_menu(point(px(0.0), px(0.0)), &FORGE_DEFAULT)
                    .items(vec![
                        menu_header("Viewer"),
                        menu_item("off", "Off", pick("off", &self.picked))
                            .disabled(true)
                            .into(),
                        menu_divider(),
                        menu_item("one", "One", pick("one", &self.picked)).into(),
                        menu_item("two", "Two", pick("two", &self.picked)).into(),
                    ])
                    .dismiss_on_escape(&self.focus)
                    .on_dismiss(move |_, cx| {
                        view.update(cx, |host, cx| {
                            host.open = false;
                            cx.notify();
                        });
                    })
            });
            div().size_full().children(menu)
        }
    }

    fn opened(cx: &mut TestAppContext) -> (Entity<MenuHost>, &mut VisualTestContext) {
        cx.update(|cx| {
            cx.set_global(Presentation::new(ThemeId::ForgeDefault, Density::Cozy));
            bind_menu_keys(cx);
        });
        let (host, vcx) = cx.add_window_view(|_, cx| MenuHost {
            focus: cx.focus_handle(),
            open: true,
            picked: Rc::default(),
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        (host, vcx)
    }

    fn picked(vcx: &mut VisualTestContext, host: &Entity<MenuHost>) -> Vec<&'static str> {
        host.read_with(vcx, |host, _| host.picked.borrow().clone())
    }

    #[gpui::test]
    fn arrows_and_enter_run_the_item_at_the_key_cursor_and_close_the_menu(cx: &mut TestAppContext) {
        let (host, vcx) = opened(cx);

        vcx.simulate_keystrokes("down down enter");

        assert_eq!(picked(vcx, &host), ["two"]);
        assert!(!host.read_with(vcx, |host, _| host.open));
    }

    #[gpui::test]
    fn enter_before_any_arrow_runs_nothing_and_keeps_the_menu(cx: &mut TestAppContext) {
        let (host, vcx) = opened(cx);

        vcx.simulate_keystrokes("enter");

        assert!(picked(vcx, &host).is_empty());
        assert!(host.read_with(vcx, |host, _| host.open));
    }

    #[gpui::test]
    fn escape_still_closes_a_menu_with_a_key_cursor(cx: &mut TestAppContext) {
        let (host, vcx) = opened(cx);

        vcx.simulate_keystrokes("down escape");

        assert!(picked(vcx, &host).is_empty());
        assert!(!host.read_with(vcx, |host, _| host.open));
    }
}
