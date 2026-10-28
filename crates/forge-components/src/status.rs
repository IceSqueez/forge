use gpui::{
    Animation, AnimationExt, App, ElementId, FontWeight, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, Styled, Window, div, px,
};
use std::time::Duration;

use crate::ActivePresentation;
use crate::tokens::{Radius, Spacing, body_family, mono_family, radius, scaled, spacing};

const BADGE_PAD_V: Pixels = px(1.0);
const BADGE_RADIUS: Pixels = px(8.0);
const PULSE_PERIOD: Duration = Duration::from_millis(1400);
const PULSE_DEPTH: f32 = 0.6;
const PULSE_FPS: f32 = 20.0;

pub fn status_dot(color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .rounded(radius(Radius::Pill))
        .bg(color)
}

pub fn pulse_dot(id: impl Into<ElementId>, color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .rounded(radius(Radius::Pill))
        .bg(color)
        .with_animation(
            id.into(),
            Animation::new(PULSE_PERIOD)
                .repeat_synced()
                .with_max_fps(PULSE_FPS),
            |el, delta| el.opacity(1.0 - (delta * 2.0 - 1.0).abs() * PULSE_DEPTH),
        )
}

#[derive(IntoElement)]
pub struct Badge {
    background: Rgba,
    text_color: Rgba,
    content: SharedString,
    mono: bool,
    size: Pixels,
    weight: FontWeight,
    padding: Option<(Pixels, Pixels)>,
    radius: Pixels,
    flex_none: bool,
}

pub fn badge(
    background: Rgba,
    text_color: Rgba,
    content: impl Into<SharedString>,
    mono: bool,
    size: Pixels,
) -> Badge {
    Badge {
        background,
        text_color,
        content: content.into(),
        mono,
        size,
        weight: FontWeight::MEDIUM,
        padding: None,
        radius: BADGE_RADIUS,
        flex_none: false,
    }
}

impl Badge {
    #[must_use]
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    #[must_use]
    pub fn padding_xy(mut self, vertical: Pixels, horizontal: Pixels) -> Self {
        self.padding = Some((vertical, horizontal));
        self
    }

    #[must_use]
    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    #[must_use]
    pub fn flex_none(mut self) -> Self {
        self.flex_none = true;
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let density = cx.density();
        let (pad_v, pad_h) = self
            .padding
            .unwrap_or_else(|| (scaled(BADGE_PAD_V, density), spacing(Spacing::Xs, density)));
        let family = if self.mono {
            mono_family()
        } else {
            body_family()
        };
        let label = div()
            .font_family(family)
            .font_weight(self.weight)
            .text_size(self.size)
            .text_color(self.text_color)
            .child(self.content);
        let mut root = div()
            .py(pad_v)
            .px(pad_h)
            .rounded(self.radius)
            .bg(self.background);
        if self.flex_none {
            root = root.flex_none();
        }
        root.child(label)
    }
}
