use gpui::{
    App, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, Styled, Window, div, px, relative,
};

use crate::palette::ForgePalette;

const TRACK_HEIGHT: Pixels = px(3.0);
const TRACK_RADIUS: Pixels = px(2.0);

pub fn clamped_fraction(fraction: f32) -> f32 {
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
