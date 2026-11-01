use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Rgba,
    SharedString, Styled, Window, div,
};

use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{Density, FONT_SM, FONT_XS, FONT_XXS, Spacing, spacing};

#[derive(IntoElement)]
pub struct FieldLabel {
    label: SharedString,
    control: AnyElement,
    tone: Rgba,
    size: Pixels,
    density: Option<Density>,
}

pub fn field_label(
    palette: &ForgePalette,
    label: impl Into<SharedString>,
    control: impl IntoElement,
) -> FieldLabel {
    FieldLabel {
        label: label.into(),
        control: control.into_any_element(),
        tone: palette.text_faint,
        size: FONT_XXS,
        density: None,
    }
}

impl FieldLabel {
    #[must_use]
    pub fn tone(mut self, color: Rgba) -> Self {
        self.tone = color;
        self
    }

    #[must_use]
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = Some(density);
        self
    }
}

impl RenderOnce for FieldLabel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = self.density.unwrap_or_else(|| cx.density());
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(self.size)
                    .text_color(self.tone)
                    .child(self.label),
            )
            .child(self.control)
    }
}

pub fn field_title(
    text: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &App,
) -> impl IntoElement {
    div()
        .font_family(cx.body_family())
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(text.into())
}

pub fn field_hint(
    text: impl Into<SharedString>,
    palette: &ForgePalette,
    cx: &App,
) -> impl IntoElement {
    div()
        .font_family(cx.body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text.into())
}
