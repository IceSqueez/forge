use gpui::{IntoElement, Pixels, Rgba, Styled, div};

use crate::tokens::{Radius, radius};

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
