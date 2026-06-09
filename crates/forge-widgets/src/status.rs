use std::borrow::Cow;

use iced::{Border, Color, Element, widget::container};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, FONT_XS, Spacing, sp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusVariant {
    Positive,
    Negative,
    Neutral,
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

pub fn status_pill<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    variant: StatusVariant,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let color = match variant {
        StatusVariant::Positive => palette.success,
        StatusVariant::Negative => palette.random,
        StatusVariant::Neutral => palette.disabled,
    };
    let bg = Color { a: 0.18, ..color };
    let label_str: Cow<'a, str> = label.into();

    container(iced::widget::text(label_str).size(FONT_XS).color(color))
        .padding([sp(Spacing::Xxs), sp(Spacing::Sm)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: 8.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub fn platform_badge<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    color: Color,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = Color { a: 0.15, ..color };
    let label_str: Cow<'a, str> = label.into();
    let dot_color = color;
    let text_color = palette.text_secondary;

    let dot = container(iced::widget::Space::new().width(5.0).height(5.0))
        .width(5.0)
        .height(5.0)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(dot_color)),
            border: Border {
                radius: 2.5_f32.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let row = iced::widget::row![
        dot,
        iced::widget::text(label_str)
            .size(FONT_XS)
            .color(text_color),
    ]
    .spacing(4)
    .align_y(iced::alignment::Vertical::Center);

    container(row)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(bg)),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub fn role_badge<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_color = palette.text_muted;
    let border_color = palette.border_regular;
    let label_str: Cow<'a, str> = label.into();

    container(
        iced::widget::text(label_str)
            .size(FONT_XS)
            .color(text_color),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_theme: &iced::Theme| container::Style {
        background: None,
        border: Border {
            color: border_color,
            width: BORDER_THIN,
            radius: 3.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}
