use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_MD, FONT_XS, ModalSize,
    Radius, Spacing, modal_width, radius, spacing,
};

/// Side of the square tile behind the optional header icon.
const HEADER_TILE: Pixels = px(28.0);
/// Rendered size of the glyph centred in the header icon tile.
const HEADER_TILE_ICON: Pixels = px(15.0);
/// Gap between the title and its subtitle when both show.
const TITLE_GAP: Pixels = px(2.0);
/// Gap between the footer actions row and the keyboard hint below it.
const FOOTER_GAP: Pixels = px(6.0);

/// Resolves a spacing token at the fixed default density. The modal carries no
/// per-instance density knob (its bands are chrome, sized once), so every inset
/// snaps to the `Spacing` scale at `Cozy` — the density-neutral multiplier.
fn pad(s: Spacing) -> Pixels {
    spacing(s, Density::Cozy)
}

/// Boxed close-button handler. Mirrors the button family: gpui hands the click
/// event plus the window and app contexts, through which the caller reaches its
/// own entity to dismiss the modal.
type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A modal dialog card: a bordered, `elevated` surface of `ModalSize` width carrying
/// a header band (optional icon tile, title, subtitle, close button over a rule), a
/// padded body region, and an optional `shell`-tinted footer band (actions row plus
/// an optional monospace keyboard hint).
///
/// Build one with [`modal`], then layer on `.subtitle`, `.header_icon`, `.size`,
/// `.footer`, `.kbd_hint` and `.on_close`. The card is only the surface: wrap it in
/// [`crate::overlay`] centred to gain the dimming scrim, the enter animation, and the
/// scrim-click / Escape dismissal —
/// `overlay(modal(...), palette).position(OverlayPosition::Center).on_dismiss(...)`.
/// The header close button ([`Modal::on_close`]) is the in-card dismissal affordance,
/// distinct from the scrim/Escape dismissal the overlay supplies.
#[derive(IntoElement)]
pub struct Modal {
    title: SharedString,
    subtitle: Option<SharedString>,
    body: AnyElement,
    footer: Option<AnyElement>,
    header_icon: Option<(Icon, Rgba)>,
    size: ModalSize,
    kbd_hint: Option<SharedString>,
    close_id: Option<ElementId>,
    on_close: Option<CloseHandler>,
    card_bg: Rgba,
    footer_bg: Rgba,
    border: Rgba,
    tile_bg: Rgba,
    title_color: Rgba,
    subtitle_color: Rgba,
    close_color: Rgba,
    kbd_color: Rgba,
}

/// Build a modal card titled `title` wrapping `body`, resolving every ink from
/// `palette` up front so the built value carries no palette borrow. Defaults to
/// [`ModalSize::Md`], header-icon-less, footer-less and with no close button; layer
/// those on through the builder methods.
pub fn modal(
    title: impl Into<SharedString>,
    body: impl IntoElement,
    palette: &ForgePalette,
) -> Modal {
    Modal {
        title: title.into(),
        subtitle: None,
        body: body.into_any_element(),
        footer: None,
        header_icon: None,
        size: ModalSize::Md,
        kbd_hint: None,
        close_id: None,
        on_close: None,
        card_bg: palette.elevated,
        footer_bg: palette.shell,
        border: palette.border_regular,
        tile_bg: palette.surface_overlay,
        title_color: palette.text_primary,
        subtitle_color: palette.text_muted,
        close_color: palette.text_faint,
        kbd_color: palette.text_faint,
    }
}

impl Modal {
    /// Adds a secondary line under the title in the header band.
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Adds a tinted icon tile at the leading edge of the header. The `tint` is the
    /// caller's accent for this dialog.
    #[must_use]
    pub fn header_icon(mut self, glyph: Icon, tint: Rgba) -> Self {
        self.header_icon = Some((glyph, tint));
        self
    }

    /// Sets the card width envelope (default [`ModalSize::Md`]).
    #[must_use]
    pub fn size(mut self, size: ModalSize) -> Self {
        self.size = size;
        self
    }

    /// Adds the footer band carrying the caller's actions row (typically kit
    /// [`crate::buttons`]). Without this — and without [`Modal::kbd_hint`] — the card
    /// ends at the body region and no footer band renders.
    #[must_use]
    pub fn footer(mut self, actions: impl IntoElement) -> Self {
        self.footer = Some(actions.into_any_element());
        self
    }

    /// Adds a monospace keyboard hint under the footer actions row (e.g. the submit
    /// chord). Shows even with no footer actions, rendering its own footer band.
    #[must_use]
    pub fn kbd_hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.kbd_hint = Some(hint.into());
        self
    }

    /// Makes the header close button live. gpui needs a stable [`ElementId`] to
    /// promote it to a clickable element; the `handler` mutates the caller's entity
    /// through the passed `cx` to dismiss the modal.
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

    /// Builds the header band: optional icon tile, the title/subtitle stack, and the
    /// optional close button, over a thin `border_regular` rule.
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

        if let (Some(id), Some(handler)) = (self.close_id.take(), self.on_close.take()) {
            row = row.child(
                div()
                    .id(id)
                    .flex_none()
                    .py(pad(Spacing::Xxs))
                    .px(pad(Spacing::Xs))
                    .cursor_pointer()
                    .on_click(handler)
                    .child(icon(Icon::X, FONT_MD, self.close_color)),
            );
        }

        row.into_any_element()
    }

    /// Builds the footer band when there is an actions row and/or a keyboard hint to
    /// carry: a `shell`-tinted, thin-bordered band stacking the actions over the hint.
    fn render_footer(&mut self) -> Option<AnyElement> {
        let footer = self.footer.take();
        let kbd = self.kbd_hint.take();
        if footer.is_none() && kbd.is_none() {
            return None;
        }

        let mut band = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(FOOTER_GAP)
            .py(pad(Spacing::Sm))
            .px(pad(Spacing::Md))
            .bg(self.footer_bg)
            .border(BORDER_THIN)
            .border_color(self.border);

        if let Some(actions) = footer {
            band = band.child(actions);
        }
        if let Some(hint) = kbd {
            band = band.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(self.kbd_color)
                    .child(hint),
            );
        }

        Some(band.into_any_element())
    }
}

impl RenderOnce for Modal {
    fn render(mut self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let header = self.render_header();
        let footer = self.render_footer();

        let body = div()
            .p(pad(Spacing::Md))
            .child(std::mem::replace(&mut self.body, div().into_any_element()));

        // `overflow_hidden` clips the header/footer bands to the rounded card so the
        // footer's `shell` fill does not square off the bottom corners.
        div()
            .flex()
            .flex_col()
            .w(modal_width(self.size))
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
