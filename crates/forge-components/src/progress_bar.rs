use gpui::{
    App, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window, div, px, relative,
};

use crate::palette::ForgePalette;

const TRACK_HEIGHT: Pixels = px(3.0);
const TRACK_RADIUS: Pixels = px(2.0);

pub(crate) fn clamped_fraction(fraction: f32) -> f32 {
    if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(IntoElement)]
pub struct ProgressBar {
    fraction: f32,
    fill: Rgba,
    track: Rgba,
}

pub fn progress_bar(fraction: f32, fill: Rgba, palette: &ForgePalette) -> ProgressBar {
    ProgressBar {
        fraction: clamped_fraction(fraction),
        fill,
        track: palette.surface_overlay,
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .w_full()
            .h(TRACK_HEIGHT)
            .rounded(TRACK_RADIUS)
            .bg(self.track)
            .child(
                div()
                    .h_full()
                    .w(relative(self.fraction))
                    .rounded(TRACK_RADIUS)
                    .bg(self.fill),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::clamped_fraction;

    #[test]
    fn clamped_fraction_keeps_the_fill_inside_the_track() {
        for (input, expected) in [
            (0.0, 0.0),
            (0.42, 0.42),
            (1.0, 1.0),
            (-0.0, 0.0),
            (-0.5, 0.0),
            (1.5, 1.0),
            (f32::NAN, 0.0),
            (f32::INFINITY, 0.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            assert_eq!(clamped_fraction(input), expected, "{input}");
        }
    }
}
