use std::rc::Rc;

use gpui::{
    App, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Rgba,
    StatefulInteractiveElement, Styled, Window, WindowControlArea, div, prelude::FluentBuilder, px,
    svg,
};

use crate::icons::Icon;
use crate::palette::ForgePalette;
use crate::tokens::{Radius, radius};

type CloseHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;

const CAPTION_BUTTON_WIDTH: Pixels = px(46.0);
const COMPACT_BUTTON_SIZE: Pixels = px(24.0);
const COMPACT_GAP: Pixels = px(8.0);
const COMPACT_PAD_H: Pixels = px(10.0);
const GLYPH_SIZE: Pixels = px(14.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WindowControlsLayout {
    Caption,
    Compact,
}

impl WindowControlsLayout {
    pub fn native() -> Self {
        if cfg!(target_os = "windows") {
            Self::Caption
        } else {
            Self::Compact
        }
    }

    fn os_handles_clicks(self) -> bool {
        self == Self::Caption && cfg!(target_os = "windows")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlKind {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl ControlKind {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "window-control-minimize",
            Self::Maximize => "window-control-maximize",
            Self::Restore => "window-control-restore",
            Self::Close => "window-control-close",
        }
    }

    fn group(self) -> &'static str {
        match self {
            Self::Minimize => "window-control-minimize-group",
            Self::Maximize => "window-control-maximize-group",
            Self::Restore => "window-control-restore-group",
            Self::Close => "window-control-close-group",
        }
    }

    fn glyph(self) -> Icon {
        match self {
            Self::Minimize => Icon::Minus,
            Self::Maximize => Icon::Square,
            Self::Restore => Icon::Squares,
            Self::Close => Icon::X,
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

#[derive(IntoElement)]
pub struct WindowControls {
    layout: WindowControlsLayout,
    idle_glyph: Rgba,
    hover_glyph: Rgba,
    hover_fill: Rgba,
    close_glyph: Rgba,
    close_fill: Rgba,
    on_close: CloseHandler,
}

pub fn window_controls(
    layout: WindowControlsLayout,
    palette: &ForgePalette,
    on_close: impl Fn(&mut Window, &mut App) + 'static,
) -> WindowControls {
    WindowControls {
        layout,
        idle_glyph: palette.text_secondary,
        hover_glyph: palette.text_primary,
        hover_fill: palette.surface_overlay,
        close_glyph: palette.shell,
        close_fill: palette.danger,
        on_close: Rc::new(on_close),
    }
}

impl WindowControls {
    fn button(&self, kind: ControlKind) -> impl IntoElement + use<> {
        let (hover_fill, hover_glyph) = if kind == ControlKind::Close {
            (self.close_fill, self.close_glyph)
        } else {
            (self.hover_fill, self.hover_glyph)
        };
        let group = kind.group();
        let layout = self.layout;
        let on_close = Rc::clone(&self.on_close);

        div()
            .id(kind.id())
            .group(group)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .map(|button| match layout {
                WindowControlsLayout::Caption => button.w(CAPTION_BUTTON_WIDTH).h_full(),
                WindowControlsLayout::Compact => button
                    .size(COMPACT_BUTTON_SIZE)
                    .rounded(radius(Radius::Pill)),
            })
            .cursor_pointer()
            .hover(move |style| style.bg(hover_fill))
            .window_control_area(kind.area())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if layout.os_handles_clicks() {
                    return;
                }
                match kind {
                    ControlKind::Minimize => window.minimize_window(),
                    ControlKind::Maximize | ControlKind::Restore => window.zoom_window(),
                    ControlKind::Close => on_close(window, cx),
                }
            })
            .child(
                svg()
                    .flex_none()
                    .size(GLYPH_SIZE)
                    .path(kind.glyph().path())
                    .text_color(self.idle_glyph)
                    .group_hover(group, move |glyph| glyph.text_color(hover_glyph)),
            )
    }
}

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let supported = window.window_controls();
        let mut kinds = Vec::new();
        if supported.minimize && window.is_minimizable() {
            kinds.push(ControlKind::Minimize);
        }
        if supported.maximize && window.is_resizable() {
            kinds.push(if window.is_maximized() {
                ControlKind::Restore
            } else {
                ControlKind::Maximize
            });
        }
        kinds.push(ControlKind::Close);

        div()
            .id("window-controls")
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .when(self.layout == WindowControlsLayout::Compact, |row| {
                row.gap(COMPACT_GAP).px(COMPACT_PAD_H)
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(kinds.into_iter().map(|kind| self.button(kind)))
    }
}
