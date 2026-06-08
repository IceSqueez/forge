use std::borrow::Cow;

use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{Space, button, column, container, row, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Waiting,
    Success,
    Error,
}

fn banner_dot<'a, Msg: 'a>(color: Color) -> Element<'a, Msg> {
    container(Space::new())
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

pub fn live_status_banner<'a, Msg: 'a>(
    kind: BannerKind,
    message: &'a str,
    hint: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let (dot_color, bg_color, border_color) = match kind {
        BannerKind::Waiting => (palette.brand, palette.surface_overlay, palette.brand),
        BannerKind::Success => (
            palette.success,
            Color {
                a: 0.18,
                ..palette.success
            },
            palette.success,
        ),
        BannerKind::Error => (
            palette.random,
            Color {
                a: 0.18,
                ..palette.random
            },
            palette.random,
        ),
    };

    let dot = banner_dot::<Msg>(dot_color);

    let mut content_col = column![
        row![dot, text(message).size(FONT_SM).color(palette.text_primary)]
            .spacing(10)
            .align_y(Alignment::Center),
    ];

    if let Some(hint_text) = hint {
        content_col = content_col.push(
            text(hint_text)
                .font(font(FontRole::Monospace))
                .size(FONT_XS)
                .color(palette.text_faint)
                .wrapping(iced::widget::text::Wrapping::Word),
        );
    }

    container(content_col)
        .padding(Padding::from([sp(Spacing::Sm), sp(Spacing::Md)]))
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(bg_color)),
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastVariant {
    Info,
    Success,
    Warning,
    Error,
}

pub fn section_header<'a, Msg: 'a>(
    label: impl Into<Cow<'a, str>>,
    count: Option<u32>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let label_str: Cow<'a, str> = label.into();
    let display = match count {
        Some(n) => format!("{} · {}", label_str.to_uppercase(), n),
        None => label_str.to_uppercase(),
    };

    container(
        text(display)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_muted),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .into()
}

/// Chevron rotates to `▾` when expanded and `▸` when collapsed.
pub fn section_header_expandable<'a, Msg: 'a + Clone>(
    palette: &'a ForgePalette,
    label: impl Into<Cow<'a, str>>,
    count: u32,
    expanded: bool,
    on_toggle: Msg,
) -> Element<'a, Msg> {
    let label_str: Cow<'a, str> = label.into();
    let chevron_char = chevron_for(expanded);
    let surface_overlay = palette.surface_overlay;

    let inner = row![
        text(chevron_char)
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_muted),
        text(label_str.to_uppercase())
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(palette.text_muted),
        counter_badge_inline(count, palette),
        Space::new().width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    button(inner)
        .on_press(on_toggle)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => {
                    Some(Background::Color(Color {
                        a: 0.08,
                        ..surface_overlay
                    }))
                }
                _ => None,
            };
            button::Style {
                background: bg,
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .into()
}

pub(crate) fn chevron_for(expanded: bool) -> char {
    if expanded { '▾' } else { '▸' }
}

fn counter_badge_inline<'a, Msg: 'a>(count: u32, palette: &ForgePalette) -> Element<'a, Msg> {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    text(label)
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(palette.text_faint)
        .into()
}

pub fn empty_state<'a, Msg: 'a + Clone>(
    headline: impl Into<Cow<'a, str>>,
    body: impl Into<Cow<'a, str>>,
    action: Option<(impl Into<Cow<'a, str>>, Msg)>,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let headline_str: Cow<'a, str> = headline.into();
    let body_str: Cow<'a, str> = body.into();

    let mut col = column![
        text(headline_str)
            .size(FONT_MD)
            .color(palette.text_secondary),
        text(body_str).size(FONT_SM).color(palette.text_muted),
    ]
    .spacing(6)
    .align_x(Alignment::Center);

    if let Some((lbl, msg)) = action {
        col = col.push(crate::ghost_button(lbl, msg, palette));
    }

    container(col)
        .padding(sp(Spacing::Lg))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

pub fn toast_banner<'a, Msg: 'a + Clone>(
    message: impl Into<Cow<'a, str>>,
    variant: ToastVariant,
    on_dismiss: Msg,
    palette: &'a ForgePalette,
) -> Element<'a, Msg> {
    let accent = match variant {
        ToastVariant::Info => palette.info,
        ToastVariant::Success => palette.success,
        ToastVariant::Warning => palette.warning,
        ToastVariant::Error => palette.random,
    };

    let message_str: Cow<'a, str> = message.into();

    let dismiss_btn = iced::widget::button(text("✕").size(FONT_XS).color(palette.text_muted))
        .on_press(on_dismiss)
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: None,
                text_color: palette.text_muted,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        );

    let content_row = row![
        container(iced::widget::Space::new().width(3.0))
            .width(3.0)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(accent)),
                ..container::Style::default()
            }),
        container(text(message_str).size(FONT_SM).color(palette.text_primary))
            .padding([sp(Spacing::Sm), sp(Spacing::Sm)])
            .width(iced::Length::Fill),
        container(dismiss_btn).padding([sp(Spacing::Xs), sp(Spacing::Xs)]),
    ]
    .align_y(Alignment::Center);

    container(content_row)
        .width(iced::Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(Color { a: 0.12, ..accent })),
            border: Border {
                color: Color { a: 0.25, ..accent },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub fn counter_badge<'a, Msg: 'a>(count: u32, palette: &ForgePalette) -> Element<'a, Msg> {
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };

    let bg = Color {
        a: 0.20,
        ..palette.brand
    };
    let text_color = palette.brand;

    container(text(label).size(FONT_XS).color(text_color))
        .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_header_expandable_uses_chevron_down_when_expanded() {
        assert_eq!(chevron_for(true), '▾');
        assert_eq!(chevron_for(false), '▸');
    }
}
