use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, CursorStyle, DragMoveEvent, ElementId,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, RenderOnce, Rgba, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY, FONT_MD, FONT_SM, Radius, radius};

/// Header row height — the title band above the divider.
const HEADER_H: Pixels = px(56.0);
/// Horizontal inset shared by the header band and the content region.
const PAD_H: Pixels = px(16.0);
/// Vertical inset of the content region below the header.
const PAD_V: Pixels = px(12.0);
/// Side of the square tile behind the optional header icon.
const HEADER_TILE: Pixels = px(28.0);
/// Gap between the header icon tile and the title block.
const HEADER_TILE_GAP: Pixels = px(10.0);
/// Rendered size of the glyph centred in the header icon tile.
const HEADER_TILE_ICON: Pixels = px(15.0);
/// Side of the square close-button hit area.
const CLOSE_HIT: Pixels = px(32.0);
/// Thickness of the 1px rule under the header.
const DIVIDER_H: Pixels = px(1.0);
/// Painted width of the resize edge line.
const RESIZE_VISUAL_W: Pixels = px(2.0);
/// Grab width of the (mostly transparent) resize hit strip straddling the edge line.
const RESIZE_HIT_W: Pixels = px(8.0);

/// Group name tying the resize hit strip to its edge line so hovering the wider
/// grab area recolours the thin painted line. Side sheets are singular on screen,
/// so a constant name is safe.
const RESIZE_GROUP: &str = "forge-side-sheet-resize";

/// Which window edge the sheet docks to. Drives which side the header close button,
/// the resize edge line and the resize direction sit on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SheetPosition {
    /// Docked to the right edge; the resize line sits on the sheet's left.
    Right,
    /// Docked to the left edge; the resize line sits on the sheet's right.
    Left,
}

/// The width envelope of a resizable sheet: the seed the caller stores as its live
/// width plus the `[min, max]` the drag clamps to. The caller owns the current
/// width as its own state (feeding it back through [`side_sheet`]); this type only
/// carries the seed and the clamp bounds.
#[derive(Clone, Copy, Debug)]
pub struct SheetWidth {
    /// Width the caller seeds its state with before the first resize.
    pub initial: f32,
    /// Lower clamp bound the drag will not shrink past.
    pub min: f32,
    /// Upper clamp bound the drag will not grow past.
    pub max: f32,
}

impl SheetWidth {
    /// Builds a width envelope. Debug-asserts `min >= 200`, `min < max`, and
    /// `initial` within `[min, max]` — a misconfigured envelope is a caller bug,
    /// not a runtime condition, so it trips only in debug.
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

/// Boxed close-button handler. Mirrors the button family: gpui hands the click
/// event plus the window and app contexts, through which the caller reaches its
/// own entity to flip the sheet closed.
type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Boxed resize handler, fired continuously while the edge is dragged. Takes the
/// already-clamped new width by reference so it composes with `cx.listener`
/// (which yields `Fn(&E, …)`); the caller stores the width and repaints.
type ResizeHandler = Box<dyn Fn(&Pixels, &mut Window, &mut App) + 'static>;

/// Everything the resize edge needs: a stable id to promote the grab strip to a
/// draggable element, the clamp bounds, and the width sink.
struct ResizeConfig {
    id: ElementId,
    min: Pixels,
    max: Pixels,
    handler: ResizeHandler,
}

/// Zero-size drag payload marker. Presence of an active drag of this type is what
/// [`gpui::InteractiveElement::on_drag_move`] keys on to deliver move events even
/// once the cursor leaves the grab strip.
struct SheetResizeDrag;

/// Invisible ghost view gpui renders at the cursor for the duration of a resize
/// drag. It paints nothing — resizing has no drag preview, unlike a drag-and-drop.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// A docked inspector panel: a bordered surface of caller-owned `width` carrying an
/// optional header (icon tile, title, subtitle, close button), a divider and a
/// padded content region, with an optional draggable resize edge.
///
/// Build one with [`side_sheet`], then layer on `.position`, `.header`, `.subtitle`,
/// `.header_icon`, `.on_close` and `.on_resize`. The panel's width is **caller
/// state**: pass the current width in, and (when resizable) the caller's
/// [`SideSheet::on_resize`] handler receives each new width to store and feed back.
///
/// The open/close visibility, slide animation, backdrop scrim and click-outside
/// dismissal are intentionally the caller's (the screen router shows or hides the
/// panel, and a scrim is a separate overlay concern) — this component is the panel
/// surface itself, not the modal shell around it.
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

/// Wrap `content` in a docked inspector panel of the given `width`, resolving the
/// surface, border, handle and header inks from `palette` up front so the built
/// value carries no palette borrow. Defaults to right-docked with no header and no
/// resize edge; layer those on through the builder methods.
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
    /// Sets which window edge the sheet docks to (default [`SheetPosition::Right`]).
    #[must_use]
    pub fn position(mut self, position: SheetPosition) -> Self {
        self.position = position;
        self
    }

    /// Adds the header band with `title`. Without this the sheet is header-less and
    /// the content region fills the whole panel.
    #[must_use]
    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Adds a secondary line under the title. Only shows when a header title is set.
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Adds a tinted icon tile at the leading edge of the header. Only shows when a
    /// header title is set. The `tint` is the caller's accent for this sheet.
    #[must_use]
    pub fn header_icon(mut self, glyph: Icon, tint: Rgba) -> Self {
        self.header_icon = Some((glyph, tint));
        self
    }

    /// Makes the header close button live. gpui needs a stable [`ElementId`] to
    /// promote it to a clickable element; the `handler` mutates the caller's entity
    /// through the passed `cx` to dismiss the sheet.
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

    /// Makes the docked edge a draggable resize handle clamped to `bounds`. `id`
    /// promotes the grab strip to a draggable element; `handler` receives each new
    /// (already-clamped) width as the drag moves, for the caller to store and feed
    /// back through [`side_sheet`]. Compose the handler with `cx.listener` so it
    /// mutates the caller's entity.
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

    /// Builds the header band: optional icon tile, the title/subtitle stack, and the
    /// optional close button, over a 1px bottom rule.
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

    /// Builds the draggable resize edge: a wide transparent grab strip pinned to the
    /// docked edge, carrying a thin painted line that lights up on hover. `id`
    /// promotes the strip to a draggable element that starts the resize drag; the
    /// width computation itself rides the panel's `on_drag_move` in [`render`].
    fn render_resize_edge(&self, id: ElementId) -> AnyElement {
        let position = self.position;
        let rest = self.handle_rest;
        let hover = self.handle_hover;

        // Thin painted line, pinned to the docked edge, recoloured while the wider
        // grab strip is hovered.
        let line = div()
            .w(RESIZE_VISUAL_W)
            .h_full()
            .bg(rest)
            .group_hover(RESIZE_GROUP, move |s| s.bg(hover));

        let mut strip = div().absolute().top_0().h_full().w(RESIZE_HIT_W).flex();
        strip = match position {
            // Right-docked: edge on the left, grab strip extends inward (rightward).
            SheetPosition::Right => strip.left_0().justify_start(),
            // Left-docked: edge on the right, grab strip extends inward (leftward).
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

        // While a resize drag is active the move events arrive here (in the capture
        // phase, even once the cursor leaves the grab strip). The docked opposite
        // edge is fixed, so the new width is that edge minus the live cursor x
        // (right-docked) or the cursor x minus that edge (left-docked), clamped.
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
