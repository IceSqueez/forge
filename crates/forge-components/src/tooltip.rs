use gpui::{
    AnyView, App, AppContext, Context, Div, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, div,
};

use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, FONT_XS, Radius, Spacing, radius, spacing};

fn tooltip_frame(cx: &App) -> Div {
    let palette = cx.palette();
    let density = cx.density();
    div()
        .py(spacing(Spacing::Xxs, density))
        .px(spacing(Spacing::Sm, density))
        .bg(palette.elevated)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .rounded(radius(Radius::Sm))
        .font_family(cx.body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_primary)
}

pub struct Tooltip {
    label: SharedString,
}

pub fn tooltip(label: impl Into<SharedString>) -> Tooltip {
    Tooltip {
        label: label.into(),
    }
}

impl Tooltip {
    pub fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tooltip_frame(cx).child(self.label.clone())
    }
}

pub fn tooltip_builder(
    label: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label = label.into();
    move |_window, cx| tooltip(label.clone()).build(cx)
}

pub(crate) struct TooltipLines {
    lines: Vec<SharedString>,
}

pub(crate) fn tooltip_lines(lines: Vec<SharedString>) -> TooltipLines {
    TooltipLines { lines }
}

impl TooltipLines {
    pub(crate) fn build(self, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for TooltipLines {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        let mut col = tooltip_frame(cx)
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
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_window, cx| tooltip_lines(lines.clone()).build(cx)
}
