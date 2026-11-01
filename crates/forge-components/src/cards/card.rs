use gpui::{
    AnyElement, App, Div, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window,
    div, px,
};

use crate::palette::ForgePalette;
use crate::presentation::ActivePresentation;
use crate::tokens::{BORDER_THIN, Radius, Spacing, radius, spacing};

#[derive(IntoElement)]
pub struct Card {
    child: AnyElement,
    pad_v: Option<Pixels>,
    pad_h: Option<Pixels>,
    top_radius: Pixels,
    bottom_radius: Pixels,
    full_width: bool,
    full_height: bool,
    background: Rgba,
    border: Rgba,
    text_color: Rgba,
}

pub fn card(child: impl IntoElement, palette: &ForgePalette) -> Card {
    let r = radius(Radius::Md);
    Card {
        child: child.into_any_element(),
        pad_v: None,
        pad_h: None,
        top_radius: r,
        bottom_radius: r,
        full_width: false,
        full_height: false,
        background: palette.elevated,
        border: palette.border_regular,
        text_color: palette.text_primary,
    }
}

impl Card {
    #[must_use]
    pub fn padding(mut self, padding: Pixels) -> Self {
        self.pad_v = Some(padding);
        self.pad_h = Some(padding);
        self
    }

    #[must_use]
    pub fn padding_xy(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.pad_v = Some(vertical);
        self.pad_h = Some(horizontal);
        self
    }

    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    #[must_use]
    pub fn full_height(mut self) -> Self {
        self.full_height = true;
        self
    }

    #[must_use]
    pub fn background(mut self, color: Rgba) -> Self {
        self.background = color;
        self
    }

    #[must_use]
    pub fn border_color(mut self, color: Rgba) -> Self {
        self.border = color;
        self
    }

    #[must_use]
    pub fn radius(mut self, r: Radius) -> Self {
        let v = radius(r);
        self.top_radius = v;
        self.bottom_radius = v;
        self
    }

    #[must_use]
    pub fn split_radius(mut self, top: Pixels, bottom: Pixels) -> Self {
        self.top_radius = top;
        self.bottom_radius = bottom;
        self
    }
}

impl RenderOnce for Card {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let pad = spacing(Spacing::Md, cx.density());
        let mut root = div()
            .py(self.pad_v.unwrap_or(pad))
            .px(self.pad_h.unwrap_or(pad))
            .rounded_t(self.top_radius)
            .rounded_b(self.bottom_radius)
            .border(BORDER_THIN)
            .border_color(self.border)
            .bg(self.background)
            .text_color(self.text_color)
            .child(self.child);

        if self.full_width {
            root = root.w_full().min_w(px(0.0));
        }
        if self.full_height {
            root = root.h_full();
        }

        root
    }
}

pub fn divider(palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .flex_none()
        .h(BORDER_THIN)
        .bg(palette.border_regular)
}

pub fn card_shell(palette: &ForgePalette) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(radius(Radius::Md))
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
}
