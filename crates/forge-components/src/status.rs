use gpui::{FontWeight, IntoElement, ParentElement, Pixels, Rgba, SharedString, Styled, div, px};

use crate::palette::{ForgePalette, with_alpha};
use crate::tokens::{DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, FONT_XXS, Radius, radius};

/// Chip geometry. These four values sit deliberately off the shared
/// `Spacing`/`Radius` token scale: a status chip is a fixed, density-neutral
/// pill, so its 1px vertical inset and 8px corner (which already fully rounds at
/// chip height) are carried as literals rather than snapped to the nearest
/// scale step, which would alter the shape.
const BADGE_PAD_V: Pixels = px(1.0);
const BADGE_PAD_H: Pixels = px(6.0);
const BADGE_RADIUS: Pixels = px(8.0);
const BADGE_GAP: Pixels = px(4.0);
const CONNECTION_DOT: Pixels = px(5.0);

/// A small filled circle used as a status indicator — connection dots, health
/// lights, presence markers.
///
/// The caller supplies the hue (always a `ForgePalette` field, so the dot picks
/// up the active theme) and the diameter. The circle keeps a fixed square size
/// even inside a flex row and is fully rounded, so any diameter renders as a
/// clean disc.
pub fn status_dot(color: Rgba, size: Pixels) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .rounded(radius(Radius::Pill))
        .bg(color)
}

/// The three semantic states a status chip can express. Each maps to a fixed
/// `ForgePalette` hue, so a chip re-tints automatically with the active theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusVariant {
    Positive,
    Negative,
    Neutral,
}

impl StatusVariant {
    /// Returns `(fill, ink)` for the variant: a translucent tint of the hue for
    /// the chip fill, and the full-strength hue for its text or dot.
    pub fn colors(self, palette: &ForgePalette) -> (Rgba, Rgba) {
        let ink = match self {
            StatusVariant::Positive => palette.success,
            StatusVariant::Negative => palette.random,
            StatusVariant::Neutral => palette.disabled,
        };
        (with_alpha(ink, 0.18), ink)
    }
}

/// The shared padded, rounded, filled box every chip wraps its content in. Kept
/// free of text styling so a caller passing a multi-element row (e.g. a dot plus
/// a caption) is not forced into the plain badge's weight and family.
fn badge_frame(background: Rgba, content: impl IntoElement) -> impl IntoElement {
    div()
        .py(BADGE_PAD_V)
        .px(BADGE_PAD_H)
        .rounded(BADGE_RADIUS)
        .bg(background)
        .child(content)
}

/// A small filled label chip. The caller supplies the fill and ink colors
/// (always `ForgePalette` fields — see [`StatusVariant::colors`]), the text, a
/// monospace flag, and the text size. Text renders at medium weight.
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

/// A connection-state chip: a status dot plus a caption, tinted with the success
/// hue when connected and muted when not. The caller passes the already-localized
/// caption (the kit carries no i18n of its own), while the chip owns the
/// connected/disconnected color and dot logic.
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

    /// `colors` maps each variant to a distinct semantic hue and derives the
    /// chip fill as a translucent tint of that same hue.
    ///
    /// Why this is not tautological: the variant→field wiring is a hand-written
    /// match with plausible wrong alternatives (Negative could be mis-wired to
    /// `warning`, Neutral to `text_muted`), so a field swap that ships a bug
    /// flips these assertions. The fill contract pins the magic tint alpha and
    /// the "fill shares the ink's hue" relationship — `fill.a` is asserted as a
    /// literal float, not recomputed via `with_alpha` (which would only restate
    /// the impl).
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

            // Fill is the same hue as the ink, only more transparent.
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
