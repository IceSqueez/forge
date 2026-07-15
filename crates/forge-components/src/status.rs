use gpui::{FontWeight, IntoElement, ParentElement, Pixels, Rgba, SharedString, Styled, div, px};

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

pub fn badge(
    background: Rgba,
    text_color: Rgba,
    content: impl Into<SharedString>,
    mono: bool,
    size: Pixels,
) -> impl IntoElement {
    let family = if mono {
        DEFAULT_MONO_FAMILY
    } else {
        DEFAULT_BODY_FAMILY
    };
    let label = div()
        .font_family(family)
        .font_weight(FontWeight::MEDIUM)
        .text_size(size)
        .text_color(text_color)
        .child(content.into());
    badge_frame(background, label)
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
