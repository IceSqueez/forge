use gpui::{
    AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div,
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_XS, Radius, Spacing, radius, spacing,
};

pub fn tooltip_surface(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
        .child(label.into())
}

pub struct Tooltip {
    label: SharedString,
    palette: ForgePalette,
    density: Density,
}

/// Defaults to `Density::Cozy`.
pub fn tooltip(label: impl Into<SharedString>, palette: &ForgePalette) -> Tooltip {
    Tooltip {
        label: label.into(),
        palette: *palette,
        density: Density::Cozy,
    }
}

impl Tooltip {
    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Call inside a `.tooltip(...)` builder closure, where gpui hands you the `App`.
    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_surface(self.label.clone(), &self.palette, self.density)
    }
}

/// Re-materialises a fresh [`Tooltip`] view on each hover, as gpui's tooltip contract requires.
pub fn tooltip_builder(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    let palette = *palette;
    move |_window, cx| tooltip(label.clone(), &palette).build(cx)
}

pub struct TooltipLines {
    lines: Vec<SharedString>,
    palette: ForgePalette,
    density: Density,
}

pub fn tooltip_lines(lines: Vec<SharedString>, palette: &ForgePalette) -> TooltipLines {
    TooltipLines {
        lines,
        palette: *palette,
        density: Density::Cozy,
    }
}

impl TooltipLines {
    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for TooltipLines {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, self.density))
            .py(spacing(Spacing::Xxs, self.density))
            .px(spacing(Spacing::Sm, self.density))
            .bg(self.palette.elevated)
            .border(BORDER_THIN)
            .border_color(self.palette.border_regular)
            .rounded(radius(Radius::Sm))
            .font_family(DEFAULT_BODY_FAMILY)
            .text_size(FONT_XS)
            .text_color(self.palette.text_primary);
        for line in &self.lines {
            col = col.child(div().child(line.clone()));
        }
        col
    }
}

pub fn tooltip_lines_builder(
    lines: Vec<SharedString>,
    palette: &ForgePalette,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let palette = *palette;
    move |_window, cx| tooltip_lines(lines.clone(), &palette).build(cx)
}
