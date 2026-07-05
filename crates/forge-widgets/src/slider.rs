use std::ops::RangeInclusive;

use iced::widget::slider::{Handle, HandleShape, Rail, Status, Style};
use iced::widget::{Slider, container, row};
use iced::{Background, Border, Color, Element, Length, Theme};

use crate::palette::ForgePalette;

const TRACK_HEIGHT: f32 = 4.0;
const KNOB_DIAMETER: f32 = 11.0;
const KNOB_RADIUS: f32 = KNOB_DIAMETER / 2.0;

/// Shared style closure applied to every forge slider: a thin rail whose filled
/// portion is `brand` and remaining portion is `surface_overlay`, with a circular
/// `text_primary` knob.
pub fn slider_style(palette: &ForgePalette) -> impl Fn(&Theme, Status) -> Style {
    let brand = palette.brand;
    let rail_rest = palette.surface_overlay;
    let knob = palette.text_primary;
    move |_theme, _status| Style {
        rail: Rail {
            backgrounds: (Background::Color(brand), Background::Color(rail_rest)),
            width: TRACK_HEIGHT,
            border: Border {
                radius: (TRACK_HEIGHT / 2.0).into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
        },
        handle: Handle {
            shape: HandleShape::Circle {
                radius: KNOB_RADIUS,
            },
            background: Background::Color(knob),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

/// Interactive styled slider. Returns the iced [`Slider`] builder so callers can
/// still chain `.width(..)` / `.step(..)`.
pub fn slider<'a, Msg: Clone + 'a>(
    range: RangeInclusive<f32>,
    value: f32,
    on_change: impl Fn(f32) -> Msg + 'a,
    palette: &'a ForgePalette,
) -> Slider<'a, f32, Msg> {
    iced::widget::slider(range, value, on_change).style(slider_style(palette))
}

/// Non-interactive track: renders the rail, filled portion, and knob at
/// `fraction` (0.0..=1.0) for displaying a value that has no live editing path yet.
pub fn slider_track<'a, Msg: 'a>(fraction: f32, palette: &'a ForgePalette) -> Element<'a, Msg> {
    let f = fraction.clamp(0.0, 1.0);
    let fill_portion = (f * 1000.0).round() as u16;
    let rest_portion = 1000 - fill_portion;

    let brand = palette.brand;
    let rail_rest = palette.surface_overlay;
    let knob_color = palette.text_primary;

    let fill = container(iced::widget::Space::new())
        .width(Length::FillPortion(fill_portion))
        .height(TRACK_HEIGHT)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(brand)),
            border: Border {
                radius: (TRACK_HEIGHT / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let knob = container(iced::widget::Space::new())
        .width(KNOB_DIAMETER)
        .height(KNOB_DIAMETER)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(knob_color)),
            border: Border {
                radius: KNOB_RADIUS.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let rest = container(iced::widget::Space::new())
        .width(Length::FillPortion(rest_portion))
        .height(TRACK_HEIGHT)
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(rail_rest)),
            border: Border {
                radius: (TRACK_HEIGHT / 2.0).into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    row![fill, knob, rest]
        .align_y(iced::Alignment::Center)
        .height(KNOB_DIAMETER)
        .into()
}
