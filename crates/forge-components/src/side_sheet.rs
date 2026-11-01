use crate::key_activation::KeyActivated;
use crate::presentation::ActivePresentation;
use crate::resize_handle::{ResizeEdge, ResizeRange, install_resize};
use crate::tokens::Density;
use crate::tokens::Spacing;
use crate::tokens::spacing;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_MD, FONT_SM, Radius, radius, scaled};

const HEADER_H: Pixels = px(56.0);
const PAD_V: Pixels = px(12.0);
const HEADER_TILE: Pixels = px(28.0);
const HEADER_TILE_ICON: Pixels = px(15.0);
const CLOSE_HIT: Pixels = px(32.0);
const DIVIDER_H: Pixels = BORDER_THIN;

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
    id: &'static str,
    min: Pixels,
    max: Pixels,
    handler: ResizeHandler,
}

struct SheetResizeDrag;

#[derive(IntoElement)]
pub struct SideSheet {
    content: AnyElement,
    width: Pixels,
    palette: ForgePalette,
    surface_bg: Rgba,
    border: Rgba,
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
        palette: *palette,
        surface_bg: palette.shell,
        border: palette.border_regular,
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
    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

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

    #[must_use]
    pub fn on_resize(
        mut self,
        id: &'static str,
        bounds: SheetWidth,
        handler: impl Fn(&Pixels, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.resize = Some(ResizeConfig {
            id,
            min: px(bounds.min),
            max: px(bounds.max),
            handler: Box::new(handler),
        });
        self
    }

    fn render_header(&mut self, density: Density, cx: &App) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .h(HEADER_H)
            .px(spacing(Spacing::Md, density))
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
                .font_family(cx.body_family())
                .text_size(FONT_MD)
                .text_color(self.title_color)
                .child(self.title.clone().unwrap_or_default()),
        );
        if let Some(subtitle) = self.subtitle.clone() {
            titles = titles.child(
                div()
                    .font_family(cx.body_family())
                    .text_size(FONT_SM)
                    .text_color(self.subtitle_color)
                    .child(subtitle),
            );
        }
        row = row.child(titles);

        if let (Some(id), Some(handler)) = (self.close_id.take(), self.on_close.take()) {
            let hover = self.tile_bg;
            row = row.child(
                div()
                    .id(id)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(CLOSE_HIT)
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .key_activated(self.close_color)
                    .on_click(handler)
                    .child(icon(Icon::X, FONT_MD, self.close_color)),
            );
        }

        row.into_any_element()
    }
}

impl RenderOnce for SideSheet {
    fn render(mut self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let header = self
            .title
            .is_some()
            .then(|| self.render_header(density, cx));

        let content = div()
            .flex_1()
            .px(spacing(Spacing::Md, density))
            .py(scaled(PAD_V, density))
            .overflow_hidden()
            .child(std::mem::replace(
                &mut self.content,
                div().into_any_element(),
            ));

        let resize = self.resize.take();

        let palette = self.palette;
        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .w(self.width)
            .h_full()
            .bg(self.surface_bg)
            .border(BORDER_THIN)
            .border_color(self.border)
            .children(header)
            .child(content);

        match resize {
            Some(ResizeConfig {
                id,
                min,
                max,
                handler,
            }) => install_resize(
                panel,
                SheetResizeDrag,
                id,
                ResizeEdge::Left,
                ResizeRange { min, max },
                &palette,
                handler,
            ),
            None => panel,
        }
    }
}
