use gpui::{
    Animation, AnimationExt, App, ElementId, FontWeight, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, Styled, Window, div, px,
};
use std::time::Duration;

use crate::palette::ForgePalette;
use crate::tokens::{FONT_XXS, Radius, body_family, mono_family, radius};

// Deliberately off the `Spacing`/`Radius` scale: a status chip is fixed, density-neutral pill geometry.
const BADGE_PAD_V: Pixels = px(1.0);
const BADGE_PAD_H: Pixels = px(6.0);
const BADGE_RADIUS: Pixels = px(8.0);
const BADGE_GAP: Pixels = px(4.0);
const CONNECTION_DOT: Pixels = px(5.0);
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

/// Phase comes from the app-wide synced clock, so every live dot pulses together; `id` must still
/// be distinct per instance because gpui keys the element's animation state by it.
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

fn badge_frame(background: Rgba, content: impl IntoElement) -> impl IntoElement {
    div()
        .py(BADGE_PAD_V)
        .px(BADGE_PAD_H)
        .rounded(BADGE_RADIUS)
        .bg(background)
        .child(content)
}

#[derive(IntoElement)]
pub struct Badge {
    background: Rgba,
    text_color: Rgba,
    content: SharedString,
    mono: bool,
    size: Pixels,
    weight: FontWeight,
    pad_v: Pixels,
    pad_h: Pixels,
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
        pad_v: BADGE_PAD_V,
        pad_h: BADGE_PAD_H,
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
        self.pad_v = vertical;
        self.pad_h = horizontal;
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
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
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
            .py(self.pad_v)
            .px(self.pad_h)
            .rounded(self.radius)
            .bg(self.background);
        if self.flex_none {
            root = root.flex_none();
        }
        root.child(label)
    }
}

pub fn connection_status_badge(
    connected: bool,
    label: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let ink = if connected {
        palette.success
    } else {
        palette.text_muted
    };
    let dot_color = if connected {
        palette.success
    } else {
        palette.text_faint
    };
    let row = div()
        .flex()
        .items_center()
        .gap(BADGE_GAP)
        .child(status_dot(dot_color, CONNECTION_DOT))
        .child(
            div()
                .text_size(FONT_XXS)
                .text_color(ink)
                .child(label.into()),
        );
    badge_frame(palette.surface_overlay, row)
}
