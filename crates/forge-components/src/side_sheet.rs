use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, CursorStyle, DragMoveEvent, ElementId,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderOnce, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY, FONT_MD, FONT_SM, Radius, radius};

const HEADER_H: Pixels = px(56.0);
const PAD_H: Pixels = px(16.0);
const PAD_V: Pixels = px(12.0);
const HEADER_TILE: Pixels = px(28.0);
const HEADER_TILE_GAP: Pixels = px(10.0);
const HEADER_TILE_ICON: Pixels = px(15.0);
const CLOSE_HIT: Pixels = px(32.0);
const DIVIDER_H: Pixels = px(1.0);
const RESIZE_VISUAL_W: Pixels = px(2.0);
const RESIZE_HIT_W: Pixels = px(8.0);

/// A constant group name is safe: side sheets are singular on screen (no hover-group collision).
const RESIZE_GROUP: &str = "forge-side-sheet-resize";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetPosition {
    Right,
    Left,
}

/// The caller owns the live width as its own state; this carries only the seed and clamp bounds.
#[derive(Clone, Copy, Debug)]
pub struct SheetWidth {
    pub initial: f32,
    pub min: f32,
    pub max: f32,
}

impl SheetWidth {
    pub fn new(initial: f32, min: f32, max: f32) -> Self {
        debug_assert!(min >= 200.0, "SheetWidth::min must be >= 200px");
        debug_assert!(min < max, "SheetWidth::min must be < max");
        debug_assert!(
            initial >= min && initial <= max,
            "SheetWidth::initial must be in [min, max]"
        );
        Self { initial, min, max }
    }
}

impl Default for SheetWidth {
    fn default() -> Self {
        Self {
            initial: 360.0,
            min: 280.0,
            max: 560.0,
        }
    }
}

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

type ResizeHandler = Box<dyn Fn(&Pixels, &mut Window, &mut App) + 'static>;

struct ResizeConfig {
    id: ElementId,
    min: Pixels,
    max: Pixels,
    handler: ResizeHandler,
}

struct SheetResizeDrag;

/// Paints nothing: a resize drag has no cursor preview (deliberate, unlike drag-and-drop).
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The panel surface only: open/close visibility, slide animation and scrim are the caller's.
#[derive(IntoElement)]
pub struct SideSheet {
    content: AnyElement,
    width: Pixels,
    position: SheetPosition,
    surface_bg: Rgba,
    border: Rgba,
    handle_rest: Rgba,
    handle_hover: Rgba,
    tile_bg: Rgba,
    title_color: Rgba,
    subtitle_color: Rgba,
    close_color: Rgba,
    title: Option<SharedString>,
    subtitle: Option<SharedString>,
    header_icon: Option<(Icon, Rgba)>,
    close_id: Option<ElementId>,
    on_close: Option<CloseHandler>,
    resize: Option<ResizeConfig>,
}

pub fn side_sheet(width: Pixels, content: impl IntoElement, palette: &ForgePalette) -> SideSheet {
    SideSheet {
        content: content.into_any_element(),
        width,
        position: SheetPosition::Right,
        surface_bg: palette.shell,
        border: palette.border_regular,
        handle_rest: palette.border_input,
        handle_hover: palette.border_active,
        tile_bg: palette.surface_overlay,
        title_color: palette.text_primary,
        subtitle_color: palette.text_secondary,
        close_color: palette.text_muted,
        title: None,
        subtitle: None,
        header_icon: None,
        close_id: None,
        on_close: None,
        resize: None,
    }
}

impl SideSheet {
    #[must_use]
    pub fn position(mut self, position: SheetPosition) -> Self {
        self.position = position;
        self
    }

    #[must_use]
    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Only shows when a header title is set.
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Only shows when a header title is set.
    #[must_use]
    pub fn header_icon(mut self, glyph: Icon, tint: Rgba) -> Self {
        self.header_icon = Some((glyph, tint));
        self
    }

    #[must_use]
    pub fn on_close(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close_id = Some(id.into());
        self.on_close = Some(Box::new(handler));
        self
    }

    /// The handler receives each new clamped width as the drag moves; the caller stores it and feeds it back through [`side_sheet`].
    #[must_use]
    pub fn on_resize(
        mut self,
        id: impl Into<ElementId>,
        bounds: SheetWidth,
        handler: impl Fn(&Pixels, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.resize = Some(ResizeConfig {
            id: id.into(),
            min: px(bounds.min),
            max: px(bounds.max),
            handler: Box::new(handler),
        });
        self
    }

    fn render_header(&mut self) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(HEADER_TILE_GAP)
            .h(HEADER_H)
            .px(PAD_H)
            .border_b(DIVIDER_H)
            .border_color(self.border);

        if let Some((glyph, tint)) = self.header_icon {
            row = row.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(HEADER_TILE)
                    .rounded(radius(Radius::Sm))
                    .bg(self.tile_bg)
                    .child(icon(glyph, HEADER_TILE_ICON, tint)),
            );
        }

        let mut titles = div().flex().flex_col().flex_1().overflow_hidden().child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_MD)
                .text_color(self.title_color)
                .child(self.title.clone().unwrap_or_default()),
        );
        if let Some(subtitle) = self.subtitle.clone() {
            titles = titles.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(self.subtitle_color)
                    .child(subtitle),
            );
        }
        row = row.child(titles);

        if let (Some(id), Some(handler)) = (self.close_id.take(), self.on_close.take()) {
            row = row.child(
                div()
                    .id(id)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(CLOSE_HIT)
                    .cursor_pointer()
                    .on_click(handler)
                    .child(icon(Icon::X, FONT_MD, self.close_color)),
            );
        }

        row.into_any_element()
    }

    fn render_resize_edge(&self, id: ElementId) -> AnyElement {
        let position = self.position;
        let rest = self.handle_rest;
        let hover = self.handle_hover;

        let line = div()
            .w(RESIZE_VISUAL_W)
            .h_full()
            .bg(rest)
            .group_hover(RESIZE_GROUP, move |s| s.bg(hover));

        let mut strip = div().absolute().top_0().h_full().w(RESIZE_HIT_W).flex();
        strip = match position {
            SheetPosition::Right => strip.left_0().justify_start(),
            SheetPosition::Left => strip.right_0().justify_end(),
        };

        strip
            .group(RESIZE_GROUP)
            .cursor(CursorStyle::ResizeLeftRight)
            .id(id)
            .on_drag(SheetResizeDrag, |_, _, _, cx| cx.new(|_| DragGhost))
            .child(line)
            .into_any_element()
    }
}

impl RenderOnce for SideSheet {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let header = self.title.is_some().then(|| self.render_header());

        let content =
            div()
                .flex_1()
                .px(PAD_H)
                .py(PAD_V)
                .overflow_hidden()
                .child(std::mem::replace(
                    &mut self.content,
                    div().into_any_element(),
                ));

        let resize = self.resize.take();
        let position = self.position;

        let mut panel = div()
            .relative()
            .flex()
            .flex_col()
            .w(self.width)
            .h_full()
            .bg(self.surface_bg)
            .border(BORDER_THIN)
            .border_color(self.border);

        if let Some(ResizeConfig {
            id,
            min,
            max,
            handler,
        }) = resize
        {
            let edge = self.render_resize_edge(id);
            panel = panel
                .on_drag_move(move |e: &DragMoveEvent<SheetResizeDrag>, window, cx| {
                    let cursor_x = e.event.position.x;
                    let raw = match position {
                        SheetPosition::Right => e.bounds.right() - cursor_x,
                        SheetPosition::Left => cursor_x - e.bounds.left(),
                    };
                    handler(&raw.clamp(min, max), window, cx);
                })
                .children(header)
                .child(content)
                .child(edge);
        } else {
            panel = panel.children(header).child(content);
        }

        panel
    }
}
