use crate::FONT_ROW;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, Rgba, StatefulInteractiveElement, StyleRefinement, Styled,
    Window, div, px, relative,
};

use crate::key_activation::KeyActivated;
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, FONT_XS, Radius, radius, scaled};

type PadClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PadTileShape {
    Stacked,
    Bar,
}

const PAD_TILE_RADIUS: Pixels = px(10.0);
const PAD_TILE_PAD_Y: Pixels = px(12.0);
const PAD_TILE_PAD_X: Pixels = px(13.0);
const PAD_TILE_ICON_SIZE: Pixels = px(30.0);
const PAD_TILE_ICON_RADIUS: Pixels = px(8.0);
const PAD_TILE_TITLE_FS: Pixels = FONT_ROW;
const PAD_TILE_PROGRESS_H: Pixels = px(2.0);
const PAD_TILE_PROGRESS_RADIUS: Pixels = px(2.0);
const PAD_TILE_PROGRESS_INSET: Pixels = px(5.0);
const PAD_BAR_RADIUS: Pixels = radius(Radius::Md);
const PAD_BAR_PAD_Y: Pixels = px(9.0);
const PAD_BAR_PAD_X: Pixels = px(12.0);
const PAD_BAR_GAP: Pixels = px(6.0);

#[derive(IntoElement)]
pub struct PadTile {
    id: ElementId,
    shape: PadTileShape,
    glyph: AnyElement,
    tile_bg: Rgba,
    tile_size: Pixels,
    tile_radius: Pixels,
    top_right: Option<AnyElement>,
    title: AnyElement,
    title_color: Rgba,
    sublabel: Option<AnyElement>,
    progress: Option<(f32, Rgba)>,
    idle_bg: Rgba,
    idle_border: Rgba,
    hover_border: Option<Rgba>,
    hover_background: Option<Rgba>,
    selected: bool,
    selected_bg: Rgba,
    selected_border: Rgba,
    radius: Pixels,
    pad_v: Pixels,
    pad_h: Pixels,
    gap: Pixels,
    on_click: Option<PadClick>,
}

pub fn pad_tile(
    id: impl Into<ElementId>,
    glyph: impl IntoElement,
    title: impl IntoElement,
    palette: &ForgePalette,
) -> PadTile {
    PadTile {
        id: id.into(),
        shape: PadTileShape::Stacked,
        glyph: glyph.into_any_element(),
        tile_bg: palette.surface_overlay,
        tile_size: PAD_TILE_ICON_SIZE,
        tile_radius: PAD_TILE_ICON_RADIUS,
        top_right: None,
        title: title.into_any_element(),
        title_color: palette.text_primary,
        sublabel: None,
        progress: None,
        idle_bg: palette.elevated,
        idle_border: palette.surface_overlay,
        hover_border: None,
        hover_background: None,
        selected: false,
        selected_bg: palette.surface_overlay,
        selected_border: palette.brand,
        radius: PAD_TILE_RADIUS,
        pad_v: PAD_TILE_PAD_Y,
        pad_h: PAD_TILE_PAD_X,
        gap: PAD_BAR_GAP,
        on_click: None,
    }
}

impl PadTile {
    #[must_use]
    pub fn top_right(mut self, el: impl IntoElement) -> Self {
        self.top_right = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn sublabel(mut self, el: impl IntoElement) -> Self {
        self.sublabel = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn progress(mut self, fraction: f32, color: Rgba) -> Self {
        self.progress = Some((fraction, color));
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn accent(mut self, color: Rgba) -> Self {
        self.selected_border = color;
        self
    }

    #[must_use]
    pub fn title_color(mut self, color: Rgba) -> Self {
        self.title_color = color;
        self
    }

    #[must_use]
    pub fn hover_border(mut self, color: Rgba) -> Self {
        self.hover_border = Some(color);
        self
    }

    #[must_use]
    pub fn bar(mut self, palette: &ForgePalette) -> Self {
        self.shape = PadTileShape::Bar;
        self.idle_bg = palette.shell;
        self.idle_border = palette.border_input;
        self.hover_background = Some(palette.surface_overlay);
        self.radius = PAD_BAR_RADIUS;
        self.pad_v = PAD_BAR_PAD_Y;
        self.pad_h = PAD_BAR_PAD_X;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for PadTile {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let (bg, border) = if self.selected {
            (self.selected_bg, self.selected_border)
        } else {
            (self.idle_bg, self.idle_border)
        };

        let root = div()
            .rounded(self.radius)
            .border(BORDER_THIN)
            .border_color(border)
            .bg(bg)
            .py(scaled(self.pad_v, density))
            .px(scaled(self.pad_h, density));

        let root = match self.shape {
            PadTileShape::Stacked => {
                let tile = div()
                    .flex_none()
                    .size(self.tile_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(self.tile_radius)
                    .bg(self.tile_bg)
                    .child(self.glyph);
                let mut header = div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(self.pad_v)
                    .child(tile);
                if let Some(top_right) = self.top_right {
                    header = header.child(top_right);
                }
                let title = div()
                    .w_full()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(cx.body_family())
                    .text_size(PAD_TILE_TITLE_FS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(self.title_color)
                    .child(self.title);

                let mut body = root.relative().flex().flex_col().child(header).child(title);
                if let Some(sub) = self.sublabel {
                    body = body.child(sub);
                }
                if let Some((fraction, color)) = self.progress {
                    body = body.child(pad_progress(fraction, color));
                }
                body
            }
            PadTileShape::Bar => root
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .gap(self.gap)
                .child(self.glyph)
                .child(
                    div()
                        .font_family(cx.body_family())
                        .text_size(FONT_XS)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(self.title_color)
                        .child(self.title),
                ),
        };

        match self.on_click {
            Some(handler) => {
                let mut r = root.id(self.id).cursor_pointer();
                if !self.selected
                    && (self.hover_border.is_some() || self.hover_background.is_some())
                {
                    let hover_border = self.hover_border;
                    let hover_background = self.hover_background;
                    r = r
                        .hover(move |s| hover_tone(s, hover_border, hover_background))
                        .active(move |s| hover_tone(s, hover_border, hover_background));
                }
                r.key_activated(cx.palette().text_primary)
                    .on_click(handler)
                    .into_any_element()
            }
            None => root.into_any_element(),
        }
    }
}

fn pad_progress(fraction: f32, color: Rgba) -> Div {
    div()
        .absolute()
        .left(PAD_TILE_PROGRESS_INSET)
        .right(PAD_TILE_PROGRESS_INSET)
        .bottom(PAD_TILE_PROGRESS_INSET)
        .h(PAD_TILE_PROGRESS_H)
        .child(
            div()
                .h_full()
                .w(relative(fraction))
                .rounded(PAD_TILE_PROGRESS_RADIUS)
                .bg(color),
        )
}

fn hover_tone(
    mut style: StyleRefinement,
    border: Option<Rgba>,
    background: Option<Rgba>,
) -> StyleRefinement {
    if let Some(color) = border {
        style = style.border_color(color);
    }
    if let Some(color) = background {
        style = style.bg(color);
    }
    style
}
