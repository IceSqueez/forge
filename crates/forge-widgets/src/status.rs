use std::borrow::Cow;

use iced::{Border, Color, Element, widget::container};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_XXS, FontRole, font};

const BADGE_PAD_V: f32 = 1.0;
const BADGE_PAD_H: f32 = 6.0;
const BADGE_RADIUS: f32 = 8.0;
const BADGE_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusVariant {
    Positive,
    Negative,
    Neutral,
}

impl StatusVariant {
    /// `(background, foreground)` - the tinted fill and the text/dot hue.
    pub fn colors(self, palette: &ForgePalette) -> (Color, Color) {
        let fg = match self {
            StatusVariant::Positive => palette.success,
            StatusVariant::Negative => palette.random,
            StatusVariant::Neutral => palette.disabled,
        };
        (Color { a: 0.18, ..fg }, fg)
    }
}

pub fn status_dot<'a, Msg: 'a>(color: Color, size: f32) -> Element<'a, Msg> {
    let radius = size / 2.0;
    container(iced::widget::Space::new().width(size).height(size))
        .width(size)
        .height(size)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(color)),
            border: Border {
                radius: radius.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn badge_frame<'a, Msg: 'a>(background: Color, content: Element<'a, Msg>) -> Element<'a, Msg> {
    container(content)
        .padding([BADGE_PAD_V, BADGE_PAD_H])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(background)),
            border: Border {
                radius: BADGE_RADIUS.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub fn badge<'a, Msg: 'a>(
    background: Color,
    text_color: Color,
    content: impl Into<Cow<'a, str>>,
    mono: bool,
    size: f32,
) -> Element<'a, Msg> {
    let content: Cow<'a, str> = content.into();
    let role = if mono {
        FontRole::Monospace
    } else {
        FontRole::Body
    };
    let text_font = iced::Font {
        weight: iced::font::Weight::Medium,
        ..font(role)
    };
    let label = iced::widget::text(content)
        .size(size)
        .color(text_color)
        .font(text_font);
    badge_frame(background, label.into())
}

pub fn connection_status_badge<'a, Msg: 'a>(
    connected: bool,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let fg = if connected {
        palette.success
    } else {
        palette.text_muted
    };
    let dot_color = if connected {
        palette.success
    } else {
        palette.text_faint
    };
    let label = if connected {
        crate::tr!("platforms.status.connected")
    } else {
        crate::tr!("platforms.status.not_connected")
    };

    let row = iced::widget::row![
        status_dot(dot_color, 5.0),
        iced::widget::text(label).size(FONT_XXS).color(fg),
    ]
    .spacing(BADGE_GAP)
    .align_y(iced::Alignment::Center);

    badge_frame(palette.surface_overlay, row.into())
}
