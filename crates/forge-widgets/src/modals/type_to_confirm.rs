use iced::{
    Alignment, Border, Color, Element, Length,
    widget::button::{Status, Style},
    widget::{Space, button, column, container, row, stack, text, text_input},
};

use crate::{
    icons::{Icon, tabler_icon},
    palette::ForgePalette,
    tokens::{FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletKind {
    Check,
    Warning,
    Info,
}

pub struct BulletItem {
    pub kind: BulletKind,
    pub text: String,
}

pub struct TypeToConfirmModalParams<'a> {
    pub title: String,
    pub explanation: String,
    pub bullets: Vec<BulletItem>,
    pub confirmation_phrase: &'a str,
    pub current_input: &'a str,
    pub confirm_label: String,
}

fn bullet_icon_and_color(kind: BulletKind, p: ForgePalette) -> (Icon, Color) {
    match kind {
        BulletKind::Check => (Icon::CircleCheck, p.success),
        BulletKind::Warning => (Icon::AlertTriangle, p.warning),
        BulletKind::Info => (Icon::InfoCircle, p.info),
    }
}

fn confirm_active_btn_style(bg: Color, fg: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, status| {
        let adjusted_bg = match status {
            Status::Hovered => Color { a: 0.85, ..bg },
            Status::Pressed => Color { a: 0.7, ..bg },
            _ => bg,
        };
        Style {
            background: Some(iced::Background::Color(adjusted_bg)),
            text_color: fg,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: r.into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        }
    }
}

fn confirm_disabled_btn_style(bg: Color, fg: Color) -> impl Fn(&iced::Theme, Status) -> Style {
    let r = radius(Radius::Md);
    move |_theme, _status| Style {
        background: Some(iced::Background::Color(bg)),
        text_color: fg,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: r.into(),
        },
        shadow: iced::Shadow::default(),
        snap: false,
    }
}

pub fn type_to_confirm_modal<'a, Msg: Clone + 'a>(
    params: TypeToConfirmModalParams<'a>,
    on_input_change: impl Fn(String) -> Msg + 'a,
    on_cancel: Msg,
    on_confirm: Msg,
    palette: &ForgePalette,
) -> Element<'a, Msg> {
    let p = *palette;
    let cancel_for_backdrop = on_cancel.clone();

    let icon_bg = Color {
        a: 0.12,
        ..p.warning
    };
    let icon_box = container(tabler_icon(Icon::AlertTriangle, 20.0, p.warning))
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(icon_bg)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        });

    let title_row = row![
        icon_box,
        text(params.title)
            .size(FONT_MD)
            .color(p.text_primary)
            .font(iced::Font {
                weight: iced::font::Weight::Medium,
                ..font(FontRole::Body)
            }),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let explanation = text(params.explanation).size(FONT_SM).color(p.text_muted);

    let header_section = container(column![title_row, explanation].spacing(8))
        .width(Length::Fill)
        .padding(iced::Padding {
            top: spf(Spacing::Md),
            right: spf(Spacing::Lg),
            bottom: spf(Spacing::Md),
            left: spf(Spacing::Lg),
        });

    let section_cap = text(crate::tr!("widget.confirm.what_this_means"))
        .font(font(FontRole::Monospace))
        .size(FONT_XS)
        .color(p.text_muted);

    let mut bullets_col = column![section_cap].spacing(0);
    for item in params.bullets {
        let (icon, icon_color) = bullet_icon_and_color(item.kind, p);
        let bullet_row = row![
            tabler_icon(icon, 14.0, icon_color),
            text(item.text.clone()).size(FONT_SM).color(p.text_primary),
        ]
        .spacing(10)
        .align_y(Alignment::Start)
        .padding([sp(Spacing::Xxs), 0]);
        bullets_col = bullets_col.push(bullet_row);
    }

    let risk_section = container(bullets_col)
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.shell)),
            ..container::Style::default()
        });

    let phrase_chip = container(
        text(params.confirmation_phrase)
            .font(font(FontRole::Monospace))
            .size(FONT_SM)
            .color(p.warning),
    )
    .padding([1u16, 6u16])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.surface_overlay)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: radius(Radius::Sm).into(),
        },
        ..container::Style::default()
    });

    let confirm_label_row = row![
        text(format!("{} ", crate::tr!("widget.confirm.type_prefix")))
            .size(FONT_SM)
            .color(p.text_primary),
        phrase_chip,
        text(format!(" {}", crate::tr!("widget.confirm.type_suffix")))
            .size(FONT_SM)
            .color(p.text_primary),
    ]
    .align_y(Alignment::Center);

    let phrase_matches = params.current_input == params.confirmation_phrase;
    let input_border_color = if phrase_matches {
        p.brand
    } else {
        p.border_input
    };

    let confirm_input = text_input("", params.current_input)
        .on_input(on_input_change)
        .padding(iced::Padding::from([sp(Spacing::Xs), sp(Spacing::Sm)]))
        .width(Length::Fill)
        .style(move |_theme, _status| text_input::Style {
            background: iced::Background::Color(p.shell),
            border: Border {
                color: input_border_color,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            icon: p.text_muted,
            placeholder: p.text_muted,
            value: p.text_primary,
            selection: Color { a: 0.25, ..p.brand },
        });

    let confirm_section = container(column![confirm_label_row, confirm_input].spacing(8))
        .width(Length::Fill)
        .padding([sp(Spacing::Md), sp(Spacing::Lg)]);

    let esc_hint = row![
        tabler_icon(Icon::Keyboard, 12.0, p.text_faint),
        text("Esc")
            .font(font(FontRole::Monospace))
            .size(FONT_XS)
            .color(p.text_faint),
        text(format!(" {}", crate::tr!("widget.confirm.esc_to_cancel")))
            .size(FONT_XS)
            .color(p.text_faint),
    ]
    .spacing(5)
    .align_y(Alignment::Center);

    let cancel_btn = button(
        text(crate::tr!("widget.confirm.cancel"))
            .size(FONT_SM)
            .color(p.text_secondary),
    )
    .on_press(on_cancel)
    .padding([sp(Spacing::Xs), sp(Spacing::Md)])
    .style(crate::buttons::outline_btn_style(
        p.border_regular,
        p.text_secondary,
        p.text_primary,
    ));

    let confirm_btn: Element<'a, Msg> = if phrase_matches {
        button(
            text(params.confirm_label)
                .size(FONT_SM)
                .color(p.shell)
                .font(iced::Font {
                    weight: iced::font::Weight::Medium,
                    ..font(FontRole::Body)
                }),
        )
        .on_press(on_confirm)
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(confirm_active_btn_style(p.warning, p.shell))
        .into()
    } else {
        button(text(params.confirm_label).size(FONT_SM).color(p.disabled))
            .padding([sp(Spacing::Xs), sp(Spacing::Md)])
            .style(confirm_disabled_btn_style(p.surface_overlay, p.disabled))
            .into()
    };

    let btn_row = row![cancel_btn, confirm_btn].spacing(8);

    let footer_section = container(
        row![esc_hint, Space::new().width(Length::Fill), btn_row,].align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([sp(Spacing::Sm), sp(Spacing::Lg)])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(p.shell)),
        ..container::Style::default()
    });

    let card_content = column![
        header_section,
        crate::sections::divider(&p, crate::sections::DividerAxis::Horizontal),
        risk_section,
        crate::sections::divider(&p, crate::sections::DividerAxis::Horizontal),
        confirm_section,
        crate::sections::divider(&p, crate::sections::DividerAxis::Horizontal),
        footer_section,
    ]
    .spacing(0);

    let card = container(card_content)
        .width(Length::Fixed(520.0))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: Border {
                color: p.border_input,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    let centered_card = container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center);

    let backdrop = button(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(cancel_for_backdrop)
        .padding(0)
        .style(|_theme: &iced::Theme, _status| Style {
            background: Some(iced::Background::Color(Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.55,
            })),
            border: Border::default(),
            text_color: Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        });

    stack![backdrop, centered_card].into()
}
