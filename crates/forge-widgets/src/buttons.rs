use std::borrow::Cow;

use iced::{
    Border, Color, Element,
    widget::button::{Status, Style},
};

use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, FONT_MD, FONT_SM, FontRole, FontWeight, Radius, Spacing, font_weighted, radius, sp,
};

fn primary_style(bg: Color, text_color: Color, status: Status) -> Style {
    let r = radius(Radius::Sm);
    match status {
        Status::Active | Status::Pressed => Style {
            background: Some(iced::Background::Color(bg)),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Hovered => Style {
            background: Some(iced::Background::Color(Color {
                a: bg.a * 0.92,
                ..bg
            })),
            text_color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Disabled => Style {
            background: Some(iced::Background::Color(Color { a: 0.4, ..bg })),
            text_color: Color {
                a: 0.5,
                ..text_color
            },
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    }
}

pub fn primary_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.brand;
    let text_color = palette.shell;
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    iced::widget::button(
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color)
            .font(font_weighted(FontRole::Body, FontWeight::SemiBold)),
    )
    .on_press(on_press)
    .padding([v, h])
    .style(move |_theme: &iced::Theme, status| primary_style(bg, text_color, status))
    .into()
}

pub fn primary_button_small<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.brand;
    let text_color = palette.shell;
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    iced::widget::button(
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color)
            .font(font_weighted(FontRole::Body, FontWeight::SemiBold)),
    )
    .on_press(on_press)
    .padding([v, h])
    .style(move |_theme: &iced::Theme, status| primary_style(bg, text_color, status))
    .into()
}

pub fn primary_button_with_icon_right<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    icon_char: char,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.brand;
    let text_color = palette.shell;
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);
    let gap = sp(Spacing::Xs);

    let content = iced::widget::row![
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color)
            .font(font_weighted(FontRole::Body, FontWeight::SemiBold)),
        iced::widget::text(icon_char.to_string())
            .size(FONT_SM)
            .color(text_color),
    ]
    .spacing(f32::from(gap));

    iced::widget::button(content)
        .on_press(on_press)
        .padding([v, h])
        .style(move |_theme: &iced::Theme, status| primary_style(bg, text_color, status))
        .into()
}

pub fn primary_button_with_icon<'a, Msg: 'a + Clone>(
    icon: crate::icons::Icon,
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use crate::icons::tabler_icon;

    let bg = palette.brand;
    let text_color = palette.shell;
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    let content = iced::widget::row![
        tabler_icon::<Msg>(icon, FONT_SM, text_color),
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color)
            .font(font_weighted(FontRole::Body, FontWeight::SemiBold)),
    ]
    .spacing(5)
    .align_y(iced::Alignment::Center);

    iced::widget::button(content)
        .on_press(on_press)
        .padding([v, h])
        .style(move |_theme: &iced::Theme, status| primary_style(bg, text_color, status))
        .into()
}

pub fn destructive_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let bg = palette.random;
    let text_color = palette.shell;
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    iced::widget::button(
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color)
            .font(font_weighted(FontRole::Body, FontWeight::SemiBold)),
    )
    .on_press(on_press)
    .padding([v, h])
    .style(move |_theme: &iced::Theme, status| primary_style(bg, text_color, status))
    .into()
}

pub fn secondary_button<'a, Msg: 'a + Clone>(
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let border_color = palette.border_regular;
    let text_color = palette.text_secondary;
    let text_hover = palette.text_primary;
    let r = radius(Radius::Sm);
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    iced::widget::button(
        iced::widget::text(label.into())
            .size(FONT_SM)
            .color(text_color),
    )
    .on_press(on_press)
    .padding([v, h])
    .style(move |_theme: &iced::Theme, status| match status {
        Status::Active | Status::Pressed => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color,
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Hovered => Style {
            background: Some(iced::Background::Color(Color {
                a: 0.06,
                ..border_color
            })),
            text_color: text_hover,
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
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
                width: BORDER_THIN,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    })
    .into()
}

pub fn ghost_button_with_icon<'a, Msg: 'a + Clone>(
    icon: crate::icons::Icon,
    label: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    use crate::icons::tabler_icon;
    use crate::tokens::{FontRole, font};

    let text_color = palette.text_muted;
    let text_hover = palette.text_primary;
    let border_color = palette.border_regular;
    let border_hover = palette.border_input;
    let r = radius(Radius::Sm);
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    let content = iced::widget::row![
        tabler_icon::<Msg>(icon, FONT_SM, text_color),
        iced::widget::text(label.into())
            .size(FONT_SM)
            .font(font(FontRole::Body)),
    ]
    .spacing(5)
    .align_y(iced::Alignment::Center);

    iced::widget::button(content)
        .on_press(on_press)
        .padding([v, h])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: None,
                text_color,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: None,
                text_color: text_hover,
                border: Border {
                    color: border_hover,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: None,
                text_color: Color {
                    a: 0.4,
                    ..text_color
                },
                border: Border {
                    color: Color {
                        a: 0.4,
                        ..border_color
                    },
                    width: 0.5,
                    radius: r.into(),
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
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let text_color = palette.text_muted;
    let text_hover = palette.text_primary;
    let border_color = palette.border_regular;
    let border_hover = palette.border_input;
    let r = radius(Radius::Sm);
    let v = sp(Spacing::Xxs);
    let h = sp(Spacing::Sm);

    iced::widget::button(iced::widget::text(label.into()).size(FONT_SM))
        .on_press(on_press)
        .padding([v, h])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: None,
                text_color,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: None,
                text_color: text_hover,
                border: Border {
                    color: border_hover,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Disabled => Style {
                background: None,
                text_color: Color {
                    a: 0.4,
                    ..text_color
                },
                border: Border {
                    color: Color {
                        a: 0.4,
                        ..border_color
                    },
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

pub fn icon_button<'a, Msg: 'a + Clone>(
    icon: char,
    tooltip: impl Into<Cow<'a, str>>,
    on_press: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let _ = tooltip;
    let icon_color = palette.text_secondary;
    let icon_hover = palette.text_primary;
    let bg_hover = Color {
        a: 0.08,
        ..palette.brand
    };
    let r = radius(Radius::Sm);

    iced::widget::button(iced::widget::text(icon.to_string()).size(FONT_MD))
        .on_press(on_press)
        .padding([sp(Spacing::Xs), sp(Spacing::Xs)])
        .style(move |_theme: &iced::Theme, status| match status {
            Status::Active | Status::Pressed => Style {
                background: None,
                text_color: icon_color,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
            Status::Hovered => Style {
                background: Some(iced::Background::Color(bg_hover)),
                text_color: icon_hover,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: r.into(),
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
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: r.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        })
        .into()
}

/// Outlined button style: transparent fill, thin border, text brightens on hover.
/// Shared chrome for modal cancel buttons and inline token controls.
pub(crate) fn outline_btn_style(
    border_color: Color,
    normal_text: Color,
    hover_text: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, status| match status {
        Status::Active | Status::Pressed => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: normal_text,
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Hovered => Style {
            background: Some(iced::Background::Color(Color {
                a: 0.06,
                ..border_color
            })),
            text_color: hover_text,
            border: Border {
                color: border_color,
                width: BORDER_THIN,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
        Status::Disabled => Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: Color {
                a: 0.4,
                ..normal_text
            },
            border: Border {
                color: Color {
                    a: 0.4,
                    ..border_color
                },
                width: BORDER_THIN,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        },
    }
}

/// Ghost icon-button style: no fill at rest, faint tint on hover.
pub(crate) fn ghost_icon_style(
    normal: Color,
    hover: Color,
) -> impl Fn(&iced::Theme, Status) -> Style {
    move |_theme, status| Style {
        background: match status {
            Status::Hovered => Some(iced::Background::Color(Color { a: 0.06, ..hover })),
            _ => None,
        },
        text_color: match status {
            Status::Hovered => hover,
            _ => normal,
        },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;

    #[test]
    fn primary_disabled_differs_from_active() {
        let bg = CATPPUCCIN_MOCHA.brand;
        let text = CATPPUCCIN_MOCHA.shell;
        let active = primary_style(bg, text, Status::Active);
        let disabled = primary_style(bg, text, Status::Disabled);
        assert_ne!(active.text_color.a, disabled.text_color.a);
        if let (
            Some(iced::Background::Color(active_bg)),
            Some(iced::Background::Color(disabled_bg)),
        ) = (active.background, disabled.background)
        {
            assert_ne!(active_bg.a, disabled_bg.a);
        }
    }
}
