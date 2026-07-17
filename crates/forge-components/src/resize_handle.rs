use gpui::{
    App, AppContext, Context, CursorStyle, Div, DragMoveEvent, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::palette::ForgePalette;

const RESIZE_VISUAL_W: Pixels = px(2.0);
const RESIZE_HIT_W: Pixels = px(8.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResizeEdge {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct ResizeRange {
    pub min: Pixels,
    pub max: Pixels,
}

struct ResizeGhost;

impl Render for ResizeGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// `marker` must be a type unique to this handle: `on_drag_move::<T>` fans out to every
/// listener of type `T`, so two panels sharing a marker would resize together.
pub fn install_resize<T: 'static>(
    panel: Div,
    marker: T,
    id: &'static str,
    edge: ResizeEdge,
    range: ResizeRange,
    palette: &ForgePalette,
    handler: impl Fn(&Pixels, &mut Window, &mut App) + 'static,
) -> Div {
    let line = div()
        .w(RESIZE_VISUAL_W)
        .h_full()
        .bg(palette.border_input)
        .group_hover(id, {
            let hover = palette.border_active;
            move |s| s.bg(hover)
        });

    let mut strip = div().absolute().top_0().h_full().w(RESIZE_HIT_W).flex();
    strip = match edge {
        ResizeEdge::Left => strip.left_0().justify_start(),
        ResizeEdge::Right => strip.right_0().justify_end(),
    };
    let strip = strip
        .group(id)
        .cursor(CursorStyle::ResizeLeftRight)
        .id(id)
        .on_drag(marker, |_, _, _, cx| cx.new(|_| ResizeGhost))
        .child(line);

    panel
        .relative()
        .on_drag_move(move |e: &DragMoveEvent<T>, window, cx| {
            let cursor_x = e.event.position.x;
            let raw = match edge {
                ResizeEdge::Left => e.bounds.right() - cursor_x,
                ResizeEdge::Right => cursor_x - e.bounds.left(),
            };
            handler(&raw.clamp(range.min, range.max), window, cx);
        })
        .child(strip)
}
