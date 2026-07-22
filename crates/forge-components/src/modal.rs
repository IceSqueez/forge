use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_MD, FONT_XS, ModalSize, Radius, Spacing,
    modal_width, radius, spacing,
};

const HEADER_TILE: Pixels = px(28.0);
const HEADER_TILE_ICON: Pixels = px(15.0);
const TITLE_GAP: Pixels = px(2.0);
const FOOTER_GAP: Pixels = px(6.0);

fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Only the surface - wrap in [`crate::overlay()`] for the scrim, enter animation, and scrim/Escape dismissal.
#[derive(IntoElement)]
pub struct Modal {
    title: SharedString,
    subtitle: Option<SharedString>,
    title_slot: Option<AnyElement>,
    body: AnyElement,
    pad_body: bool,
    footer: Option<AnyElement>,
    header_icon: Option<(Icon, Rgba)>,
    tile_size: Option<(Pixels, Pixels)>,
    size: ModalSize,
    width_override: Option<Pixels>,
    close_id: Option<ElementId>,
    on_close: Option<CloseHandler>,
    card_bg: Rgba,
    footer_bg: Rgba,
    border: Rgba,
    tile_bg: Rgba,
    title_color: Rgba,
    subtitle_color: Rgba,
    close_color: Rgba,
    close_hover: Rgba,
}

pub fn modal(
    title: impl Into<SharedString>,
    body: impl IntoElement,
    palette: &ForgePalette,
) -> Modal {
    Modal {
        title: title.into(),
        subtitle: None,
        title_slot: None,
        body: body.into_any_element(),
        pad_body: true,
        footer: None,
        header_icon: None,
        tile_size: None,
        size: ModalSize::Md,
        width_override: None,
        close_id: None,
        on_close: None,
        card_bg: palette.elevated,
        footer_bg: palette.shell,
        border: palette.border_regular,
        tile_bg: palette.surface_overlay,
        title_color: palette.text_primary,
        subtitle_color: palette.text_muted,
        close_color: palette.text_faint,
        close_hover: palette.surface_overlay,
    }
}

impl Modal {
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Replaces the whole title/subtitle column with a caller-owned element; when set, `title` and `subtitle` are ignored.
    #[must_use]
    pub fn title_slot(mut self, slot: impl IntoElement) -> Self {
        self.title_slot = Some(slot.into_any_element());
        self
    }

    #[must_use]
    pub fn header_icon(mut self, glyph: Icon, tint: Rgba) -> Self {
        self.header_icon = Some((glyph, tint));
        self
    }

    #[must_use]
    pub fn header_tile_size(mut self, tile: Pixels, glyph: Pixels) -> Self {
        self.tile_size = Some((tile, glyph));
        self
    }

    /// Drops the default body padding so the caller owns the body's own padding/scroll frame.
    #[must_use]
    pub fn flush_body(mut self) -> Self {
        self.pad_body = false;
        self
    }

    #[must_use]
    pub fn size(mut self, size: ModalSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn width(mut self, width: Pixels) -> Self {
        self.width_override = Some(width);
        self
    }

    #[must_use]
    pub fn footer(mut self, actions: impl IntoElement) -> Self {
        self.footer = Some(actions.into_any_element());
        self
    }

    /// Retained for call-site compatibility; keyboard hints are no longer rendered.
    #[must_use]
    pub fn kbd_hint(self, _hint: impl Into<SharedString>) -> Self {
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

    fn render_header(&mut self) -> AnyElement {
        let mut row = div()
            .flex()
            .items_center()
            .gap(pad(Spacing::Sm))
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Md))
            .border(BORDER_THIN)
            .border_color(self.border);

        if let Some((glyph, tint)) = self.header_icon {
            let (tile, glyph_size) = self.tile_size.unwrap_or((HEADER_TILE, HEADER_TILE_ICON));
            row = row.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(tile)
                    .rounded(radius(Radius::Sm))
                    .bg(self.tile_bg)
                    .child(icon(glyph, glyph_size, tint)),
            );
        }

        if let Some(slot) = self.title_slot.take() {
            row = row.child(slot);
        } else {
            let mut titles = div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(TITLE_GAP)
                .overflow_hidden()
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_MD)
                        .text_color(self.title_color)
                        .child(self.title.clone()),
                );
            if let Some(subtitle) = self.subtitle.clone() {
                titles = titles.child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_XS)
                        .text_color(self.subtitle_color)
                        .child(subtitle),
                );
            }
            row = row.child(titles);
        }

        if let (Some(id), Some(handler)) = (self.close_id.take(), self.on_close.take()) {
            let hover = self.close_hover;
            row = row.child(
                div()
                    .id(id)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(pad(Spacing::Xxs))
                    .px(pad(Spacing::Xs))
                    .rounded(radius(Radius::Sm))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(handler)
                    .child(icon(Icon::X, FONT_MD, self.close_color)),
            );
        }

        row.into_any_element()
    }

    fn render_footer(&mut self) -> Option<AnyElement> {
        let footer = self.footer.take()?;

        let band = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FOOTER_GAP)
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Md))
            .bg(self.footer_bg)
            .border(BORDER_THIN)
            .border_color(self.border)
            .child(footer);

        Some(band.into_any_element())
    }
}

impl RenderOnce for Modal {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let header = self.render_header();
        let footer = self.render_footer();

        let mut body = div().w_full().flex().flex_col();
        if self.pad_body {
            body = body.p(pad(Spacing::Md));
        }
        let body = body.child(std::mem::replace(&mut self.body, div().into_any_element()));

        // `overflow_hidden` clips the header/footer bands to the rounded card so the footer's `shell` fill does not square off the bottom corners.
        div()
            .flex()
            .flex_col()
            .w(self
                .width_override
                .unwrap_or_else(|| modal_width(self.size)))
            .bg(self.card_bg)
            .rounded(radius(Radius::Lg))
            .overflow_hidden()
            .border(BORDER_THIN)
            .border_color(self.border)
            .child(header)
            .child(body)
            .children(footer)
    }
}
