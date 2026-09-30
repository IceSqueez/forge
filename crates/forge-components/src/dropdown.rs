use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    Anchor, AnchoredPositionMode, AnimationExt, AnyElement, App, BoxShadow, ClickEvent, Div,
    ElementId, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement, Pixels, RenderOnce, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, anchored, deferred, div, hsla, point, px, relative,
};

use crate::icons::{Icon, icon};
use crate::overlay::enter_animation;
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, body_family};

pub(crate) const DROPDOWN_ROW_HEIGHT: Pixels = px(27.0);
pub(crate) const DROPDOWN_PADDING: Pixels = px(4.0);
pub(crate) const DROPDOWN_MAX_HEIGHT: Pixels = px(200.0);

const TRIGGER_GAP: Pixels = px(4.0);
const PANEL_RADIUS: Pixels = px(8.0);
const SHADOW_OFFSET_Y: Pixels = px(10.0);
const SHADOW_BLUR: Pixels = px(30.0);
const SHADOW_ALPHA: f32 = 0.4;

const ROW_PAD_X: Pixels = px(9.0);
const ROW_RADIUS: Pixels = px(5.0);
const ROW_GAP: Pixels = px(8.0);
const ROW_FONT: Pixels = px(12.5);
const SUFFIX_FONT: Pixels = px(11.0);
const CHECK_SIZE: Pixels = px(12.0);
const ICON_SIZE: Pixels = px(13.0);

const ESCAPE_KEY: &str = "escape";
const DROPDOWN_PRIORITY: usize = 1;

type DismissHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;
type RowClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub fn dropdown_surface(palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .p(DROPDOWN_PADDING)
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_input)
        .rounded(PANEL_RADIUS)
        .shadow(vec![BoxShadow {
            color: hsla(0.0, 0.0, 0.0, SHADOW_ALPHA),
            offset: point(px(0.0), SHADOW_OFFSET_Y),
            blur_radius: SHADOW_BLUR,
            spread_radius: px(0.0),
            inset: false,
        }])
        .occlude()
}

pub fn dropdown_list(id: impl Into<ElementId>, palette: &ForgePalette) -> Stateful<Div> {
    dropdown_surface(palette)
        .id(id)
        .max_h(DROPDOWN_MAX_HEIGHT)
        .overflow_y_scroll()
}

#[derive(IntoElement)]
pub struct DropdownRow {
    id: ElementId,
    label: SharedString,
    suffix: Option<SharedString>,
    glyph: Option<Icon>,
    label_family: Option<SharedString>,
    current: bool,
    highlighted: bool,
    on_click: Option<RowClick>,
    palette: ForgePalette,
}

pub fn dropdown_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> DropdownRow {
    DropdownRow {
        id: id.into(),
        label: label.into(),
        suffix: None,
        glyph: None,
        label_family: None,
        current: false,
        highlighted: false,
        on_click: None,
        palette: *palette,
    }
}

impl DropdownRow {
    #[must_use]
    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    #[must_use]
    pub fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }

    #[must_use]
    pub fn suffix(mut self, suffix: Option<SharedString>) -> Self {
        self.suffix = suffix;
        self
    }

    #[must_use]
    pub fn icon(mut self, glyph: Option<Icon>) -> Self {
        self.glyph = glyph;
        self
    }

    #[must_use]
    pub fn label_family(mut self, family: impl Into<SharedString>) -> Self {
        self.label_family = Some(family.into());
        self
    }

    #[must_use]
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for DropdownRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let p = self.palette;
        let ink = if self.current {
            p.text_primary
        } else {
            p.text_secondary
        };
        let check_slot = div()
            .flex_none()
            .w(CHECK_SIZE)
            .flex()
            .items_center()
            .justify_center()
            .when(self.current, |slot| {
                slot.child(icon(Icon::Check, CHECK_SIZE, p.brand))
            });

        let mut label = div()
            .flex_1()
            .min_w(px(0.0))
            .truncate()
            .font_family(self.label_family.unwrap_or_else(body_family))
            .text_size(ROW_FONT)
            .text_color(ink)
            .child(self.label);
        if self.suffix.is_some() {
            label = label.flex_none().max_w(relative(0.6));
        }

        let mut row = div()
            .id(self.id)
            .w_full()
            .flex_none()
            .h(DROPDOWN_ROW_HEIGHT)
            .flex()
            .items_center()
            .gap(ROW_GAP)
            .px(ROW_PAD_X)
            .rounded(ROW_RADIUS)
            .cursor_pointer()
            .child(check_slot);
        if let Some(glyph) = self.glyph {
            row = row.child(icon(glyph, ICON_SIZE, ink));
        }
        row = row.child(label);
        if let Some(suffix) = self.suffix {
            row = row.child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(body_family())
                    .text_size(SUFFIX_FONT)
                    .text_color(p.text_faint)
                    .child(suffix),
            );
        }

        if self.current {
            row = row.bg(p.surface_overlay);
        } else {
            if self.highlighted {
                row = row.bg(p.shell);
            }
            let hover_bg = p.shell;
            row = row.hover(move |style| style.bg(hover_bg));
        }
        if let Some(handler) = self.on_click {
            row = row.on_click(handler);
        }
        row
    }
}

#[derive(IntoElement)]
pub struct Dropdown {
    content: AnyElement,
    on_dismiss: Option<DismissHandler>,
    escape_focus: Option<FocusHandle>,
}

pub fn dropdown(content: impl IntoElement) -> Dropdown {
    Dropdown {
        content: content.into_any_element(),
        on_dismiss: None,
        escape_focus: None,
    }
}

impl Dropdown {
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

impl RenderOnce for Dropdown {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
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

        let panel = div().w_full().child(self.content).with_animation(
            ElementId::Name(SharedString::new_static("forge-dropdown-panel")),
            enter_animation(),
            |el, delta| el.opacity(delta),
        );

        let mut root = div()
            .absolute()
            .left_0()
            .right_0()
            .top(relative(1.0))
            .mt(TRIGGER_GAP)
            .child(backdrop_layer)
            .child(panel);
        if let (Some(handle), Some(dismiss)) = (self.escape_focus.as_ref(), self.on_dismiss.clone())
        {
            root = root
                .track_focus(handle)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key.as_str() == ESCAPE_KEY {
                        dismiss(window, cx);
                    }
                });
        }

        deferred(root).with_priority(DROPDOWN_PRIORITY)
    }
}
