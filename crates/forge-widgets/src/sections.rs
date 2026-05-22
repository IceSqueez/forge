use std::borrow::Cow;

use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{Space, button, column, container, row, text},
};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, font, radius};

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
    .padding([6, 14])
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
        .padding([6, 14])
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
        .padding(24)
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
        .padding([2, 6])
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
            .padding([12, 12])
            .width(iced::Length::Fill),
        container(dismiss_btn).padding([8, 8]),
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
        .padding([2, 6])
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
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn section_header_compiles_without_count() {
        let _: Element<'_, ()> = section_header("TRIGGERS", None, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn section_header_compiles_with_count() {
        let _: Element<'_, ()> = section_header("SUB-ACTIONS", Some(5), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn empty_state_compiles_without_action() {
        let _: Element<'_, ()> = empty_state(
            "No triggers yet",
            "Add your first trigger to get started.",
            None::<(&str, ())>,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn empty_state_compiles_with_action() {
        let _: Element<'_, u32> = empty_state(
            "No actions",
            "Create an action to begin.",
            Some(("Add Action", 1u32)),
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn toast_banner_compiles_for_all_variants() {
        let _: Element<'_, u32> = toast_banner(
            "Connected to Twitch.",
            ToastVariant::Success,
            0u32,
            &CATPPUCCIN_MOCHA,
        );
        let _: Element<'_, u32> = toast_banner(
            "Rate limit approaching.",
            ToastVariant::Warning,
            0u32,
            &CATPPUCCIN_MOCHA,
        );
        let _: Element<'_, u32> = toast_banner(
            "Connection lost.",
            ToastVariant::Error,
            0u32,
            &CATPPUCCIN_MOCHA,
        );
        let _: Element<'_, u32> = toast_banner(
            "EventSub active.",
            ToastVariant::Info,
            0u32,
            &CATPPUCCIN_MOCHA,
        );
    }

    #[test]
    fn counter_badge_shows_count_below_limit() {
        let _: Element<'_, ()> = counter_badge(42, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn counter_badge_clamps_above_99() {
        let _: Element<'_, ()> = counter_badge(150, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn counter_badge_zero_is_valid() {
        let _: Element<'_, ()> = counter_badge(0, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn section_header_expandable_uses_chevron_down_when_expanded() {
        assert_eq!(chevron_for(true), '▾');
        assert_eq!(chevron_for(false), '▸');
    }

    #[test]
    fn section_header_expandable_compiles_expanded() {
        let _: Element<'_, u32> =
            section_header_expandable(&CATPPUCCIN_MOCHA, "CHAT COMMANDS", 7, true, 0u32);
    }

    #[test]
    fn section_header_expandable_compiles_collapsed() {
        let _: Element<'_, u32> =
            section_header_expandable(&CATPPUCCIN_MOCHA, "TIMERS", 3, false, 0u32);
    }
}
