use std::borrow::Cow;

use iced::{
    Border, Color, Element,
    widget::button::{Status, Style},
};

use crate::palette::LoomPalette;

pub fn primary_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let bg = palette.brand;
    let text_color = palette.shell;
    let bg_hover = Color { a: 0.85, ..bg };

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
        .on_press(on_press)
        .padding([8, 12])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: Some(iced::Background::Color(bg)),
                text_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: Some(iced::Background::Color(bg_hover)),
                text_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: Some(iced::Background::Color(Color { a: 0.4, ..bg })),
                text_color: Color {
                    a: 0.4,
                    ..text_color
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

pub fn secondary_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let border_color = palette.brand;
    let text_color = palette.brand;
    let bg_hover = Color {
        a: 0.1,
        ..border_color
    };

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
        .on_press(on_press)
        .padding([8, 12])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                text_color,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: Some(iced::Background::Color(bg_hover)),
                text_color,
                border: Border {
                    color: border_color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                text_color: Color {
                    a: 0.4,
                    ..text_color
                },
                border: Border {
                    color: Color {
                        a: 0.4,
                        ..border_color
                    },
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

pub fn ghost_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let text_color = palette.text_secondary;
    let text_hover = palette.text_primary;

    iced::widget::button(iced::widget::text(label.into()))
        .on_press(on_press)
        .padding([6, 8])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: None,
                text_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: None,
                text_color: text_hover,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: None,
                text_color: Color {
                    a: 0.4,
                    ..text_color
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

/// `icon` is a char from the iced_fonts Tabler Icons set.
pub fn icon_button<'a, Msg: 'a + Clone>(
    icon: char,
    tooltip: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &LoomPalette,
) -> Element<'a, Msg> {
    let _ = tooltip;
    let icon_color = palette.text_secondary;
    let icon_hover = palette.text_primary;
    let bg_hover = Color {
        a: 0.08,
        ..palette.brand
    };

    iced::widget::button(iced::widget::text(icon.to_string()).size(16))
        .on_press(on_press)
        .padding([6, 6])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: None,
                text_color: icon_color,
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: Some(iced::Background::Color(bg_hover)),
                text_color: icon_hover,
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: None,
                text_color: Color {
                    a: 0.4,
                    ..icon_color
                },
                border: Border::default(),
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn buttons_compile_with_unit_msg() {
        let _: Element<'_, ()> = primary_button("Primary", (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = secondary_button("Secondary", (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = ghost_button("Ghost", (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = icon_button('\u{F231}', "Copy", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn buttons_accept_string_labels() {
        let label = String::from("Dynamic Label");
        let _: Element<'_, ()> = primary_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = secondary_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = ghost_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn icon_button_accepts_tabler_char() {
        let _: Element<'_, u32> = icon_button('\u{F231}', "tooltip", 42_u32, &CATPPUCCIN_MOCHA);
    }
}
