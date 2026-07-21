use gpui::{
    App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString, Styled,
    Window, div, px,
};

use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XXS, Radius, radius};

// Deliberately off the `Spacing`/`Radius` scale: a status chip is fixed, density-neutral pill geometry.
const BADGE_PAD_V: Pixels = px(1.0);
const BADGE_PAD_H: Pixels = px(6.0);
const BADGE_RADIUS: Pixels = px(8.0);
const BADGE_GAP: Pixels = px(4.0);
const CONNECTION_DOT: Pixels = px(5.0);

pub fn status_dot(color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .rounded(radius(Radius::Pill))
        .bg(color)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusVariant {
    Positive,
    Negative,
    Neutral,
}

impl StatusVariant {
    /// Returns `(fill, ink)`: a translucent tint of the hue for the fill, the full-strength hue for text/dot.
    pub fn colors(self, palette: &ForgePalette) -> (Rgba, Rgba) {
        let ink = match self {
            StatusVariant::Positive => palette.success,
            StatusVariant::Negative => palette.random,
            StatusVariant::Neutral => palette.disabled,
        };
        (with_alpha(ink, 0.18), ink)
    }
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
            DEFAULT_MONO_FAMILY
        } else {
            DEFAULT_BODY_FAMILY
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    /// Variant→field is a hand-written match with plausible wrong wirings (Negative→`warning`,
    /// Neutral→`text_muted`), so a field swap fails here. `fill.a` is pinned as a literal float,
    /// not recomputed via `with_alpha` (which would only restate the impl).
    #[test]
    fn colors_map_each_variant_to_its_hue_with_a_translucent_tint_fill() {
        let p = &CATPPUCCIN_MOCHA;
        for (variant, expected_ink) in [
            (StatusVariant::Positive, p.success),
            (StatusVariant::Negative, p.random),
            (StatusVariant::Neutral, p.disabled),
        ] {
            let (fill, ink) = variant.colors(p);

            assert_eq!(ink, expected_ink, "{variant:?} ink hue");

            assert_eq!(
                (fill.r, fill.g, fill.b),
                (ink.r, ink.g, ink.b),
                "{variant:?} fill hue"
            );
            assert!(
                (fill.a - 0.18).abs() < 1e-6,
                "{variant:?} fill alpha: got {}, want 0.18",
                fill.a
            );
        }
    }
}
