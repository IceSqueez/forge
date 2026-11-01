use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString,
    Styled, Window, div, px,
};

use crate::icons::{Icon, icon, spinner};
use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{Density, FONT_SM, Spacing, spacing};

const EMPTY_STATE_GLYPH: Pixels = px(24.0);
const EMPTY_STATE_SPINNER: Pixels = px(20.0);

#[derive(IntoElement)]
pub struct EmptyState {
    message: SharedString,
    glyph: Option<Icon>,
    loading: Option<ElementId>,
    cta: Option<AnyElement>,
    density: Option<Density>,
    message_color: Rgba,
    glyph_color: Rgba,
    spinner_color: Rgba,
}

pub fn empty_state(message: impl Into<SharedString>, palette: &ForgePalette) -> EmptyState {
    EmptyState {
        message: message.into(),
        glyph: None,
        loading: None,
        cta: None,
        density: None,
        message_color: palette.text_muted,
        glyph_color: palette.text_faint,
        spinner_color: palette.text_muted,
    }
}

impl EmptyState {
    #[must_use]
    pub fn glyph(mut self, glyph: Icon) -> Self {
        self.glyph = Some(glyph);
        self
    }

    #[must_use]
    pub fn loading(mut self, id: impl Into<ElementId>) -> Self {
        self.loading = Some(id.into());
        self
    }

    #[must_use]
    pub fn cta(mut self, cta: impl IntoElement) -> Self {
        self.cta = Some(cta.into_any_element());
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let d = self.density.unwrap_or_else(|| cx.density());

        let leading: Option<AnyElement> = match self.loading {
            Some(id) => Some(
                spinner(id, Icon::Loader2, EMPTY_STATE_SPINNER, self.spinner_color)
                    .into_any_element(),
            ),
            None => self
                .glyph
                .map(|g| icon(g, EMPTY_STATE_GLYPH, self.glyph_color).into_any_element()),
        };

        let mut col = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(spacing(Spacing::Sm, d))
            .py(spacing(Spacing::Lg, d));

        if let Some(leading) = leading {
            col = col.child(leading);
        }
        col = col.child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(self.message_color)
                .child(self.message),
        );
        if let Some(cta) = self.cta {
            col = col.child(cta);
        }
        col
    }
}
