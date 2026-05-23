use std::borrow::Cow;

use iced::{
    Border, Color, Element,
    widget::button::{Status, Style},
};

use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, Density, FONT_MD, Radius, Spacing, radius, sp, spacing};

fn primary_style(bg: Color, text_color: Color, status: Status) -> Style {
    let r = radius(Radius::Md);
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
    let v = spacing(Spacing::Sm, Density::Cozy);
    let h = spacing(Spacing::Md, Density::Cozy);

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
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
    let v = spacing(Spacing::Xs, Density::Cozy);
    let h = spacing(Spacing::Md, Density::Cozy);

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
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
    let v = spacing(Spacing::Sm, Density::Cozy);
    let h = spacing(Spacing::Md, Density::Cozy);
    let gap = spacing(Spacing::Xs, Density::Cozy);

    let content = iced::widget::row![
        iced::widget::text(label.into()).color(text_color),
        iced::widget::text(icon_char.to_string()).color(text_color),
    ]
    .spacing(f32::from(gap));

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
    let v = spacing(Spacing::Sm, Density::Cozy);
    let h = spacing(Spacing::Md, Density::Cozy);

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
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
    let r = radius(Radius::Md);
    let v = spacing(Spacing::Sm, Density::Cozy);
    let h = spacing(Spacing::Md, Density::Cozy);

    iced::widget::button(iced::widget::text(label.into()).color(text_color))
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
    use crate::tokens::{FONT_SM, FontRole, font};

    let text_color = palette.text_muted;
    let text_hover = palette.text_primary;
    let border_color = palette.border_regular;
    let border_hover = palette.border_input;
    let r = radius(Radius::Sm);
    let v = spacing(Spacing::Xs, Density::Cozy);
    let h = spacing(Spacing::Sm, Density::Cozy);

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
    let v = spacing(Spacing::Sm, Density::Cozy);
    let h = spacing(Spacing::Sm, Density::Cozy);

    iced::widget::button(iced::widget::text(label.into()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::CATPPUCCIN_MOCHA;
    use crate::tokens::{Density, Spacing, spacing};

    #[test]
    fn primary_button_compiles_with_unit_msg() {
        let _: Element<'_, ()> = primary_button("Primary", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn secondary_button_compiles_with_unit_msg() {
        let _: Element<'_, ()> = secondary_button("Secondary", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn ghost_button_compiles_with_unit_msg() {
        let _: Element<'_, ()> = ghost_button("Ghost", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn icon_button_compiles_with_tabler_char() {
        let _: Element<'_, u32> = icon_button('\u{F231}', "tooltip", 42_u32, &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn primary_button_with_icon_right_compiles() {
        let _: Element<'_, ()> =
            primary_button_with_icon_right("Continue", '→', (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn destructive_button_compiles() {
        let _: Element<'_, ()> = destructive_button("Delete", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn primary_button_small_compiles() {
        let _: Element<'_, ()> = primary_button_small("OK", (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn buttons_accept_string_labels() {
        let label = String::from("Dynamic Label");
        let _: Element<'_, ()> = primary_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = secondary_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
        let _: Element<'_, ()> = ghost_button(label.as_str(), (), &CATPPUCCIN_MOCHA);
    }

    #[test]
    fn primary_padding_matches_design_tokens() {
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 12);
        assert_eq!(spacing(Spacing::Md, Density::Cozy), 16);
    }

    #[test]
    fn primary_small_padding_matches_design_tokens() {
        assert_eq!(spacing(Spacing::Xs, Density::Cozy), 8);
        assert_eq!(spacing(Spacing::Md, Density::Cozy), 16);
    }

    #[test]
    fn secondary_padding_matches_design_tokens() {
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 12);
        assert_eq!(spacing(Spacing::Md, Density::Cozy), 16);
    }

    #[test]
    fn ghost_padding_matches_design_tokens() {
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 12);
        assert_eq!(spacing(Spacing::Sm, Density::Cozy), 12);
    }

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

    #[test]
    fn primary_radius_is_md() {
        let bg = CATPPUCCIN_MOCHA.brand;
        let text = CATPPUCCIN_MOCHA.shell;
        let style = primary_style(bg, text, Status::Active);
        assert_eq!(style.border.radius, radius(Radius::Md).into());
    }
}
