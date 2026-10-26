use gpui::{
    AnyView, App, AppContext, Context, Div, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div,
};

use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, Density, FONT_XS, Radius, Spacing, body_family, radius, spacing};

pub fn tooltip_surface(
    label: impl Into<SharedString>,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    tooltip_frame(palette, density).child(label.into())
}

fn tooltip_frame(palette: &ForgePalette, density: Density) -> Div {
    div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
}

pub struct Tooltip {
    label: SharedString,
    palette: ForgePalette,
}

pub fn tooltip(label: impl Into<SharedString>, palette: &ForgePalette) -> Tooltip {
    Tooltip {
        label: label.into(),
        palette: *palette,
    }
}

impl Tooltip {
    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_surface(self.label.clone(), &self.palette, cx.density())
    }
}

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
}

pub fn tooltip_lines(lines: Vec<SharedString>, palette: &ForgePalette) -> TooltipLines {
    TooltipLines {
        lines,
        palette: *palette,
    }
}

impl TooltipLines {
    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for TooltipLines {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let mut col = tooltip_frame(&self.palette, density)
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density));
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
