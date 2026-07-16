use iced::{
    Background, Border, Element, Length,
    widget::{Space, container, row},
};

use crate::palette::ForgePalette;
use crate::tokens::{Radius, Spacing, radius, spf};

pub const SKELETON_LINE_HEIGHT: f32 = 14.0;

/// Static muted placeholder block shown while real content loads. No shimmer or
/// motion - the animated variant is deferred to a future motion-token pass.
pub fn skeleton<'a, Msg: 'a>(
    width: Length,
    height: f32,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let fill = palette.surface_overlay;
    container(Space::new())
        .width(width)
        .height(Length::Fixed(height))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(fill)),
            border: Border {
                radius: radius(Radius::Sm).into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// One placeholder line built from several fixed-width segments (e.g. an icon
/// tile + a label bar + a trailing count bar).
pub fn skeleton_row<'a, Msg: 'a>(widths: &[f32], palette: &ForgePalette) -> Element<'a, Msg> {
    let mut r = row![].spacing(spf(Spacing::Xs));
    for &w in widths {
        r = r.push(skeleton(Length::Fixed(w), SKELETON_LINE_HEIGHT, palette));
    }
    r.into()
}
