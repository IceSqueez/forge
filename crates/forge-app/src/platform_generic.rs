use forge_platform_core::BuiltinId;
use forge_widgets::{
    ForgePalette, Icon, tabler_icon,
    tokens::{FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{column, container, row, scrollable, text},
};

use crate::Message;

pub struct GenericPlatform {
    pub name: &'static str,
    pub letter: &'static str,
    pub since: &'static str,
    pub description: &'static str,
    pub features: &'static [&'static str],
    pub kind: PlatformKind,
}

pub enum PlatformKind {
    Platform,
    StreamApp,
}

pub fn registry(id: &BuiltinId, palette: &ForgePalette) -> Option<(Color, GenericPlatform)> {
    match id.as_str() {
        "youtube" => Some((
            palette.random,
            GenericPlatform {
                name: "YouTube",
                letter: "Y",
                since: "Coming in beta-1",
                description: "Live chat, super chats, channel memberships, subscribers.",
                features: &[
                    "Live chat with sentiment markers",
                    "Super Chat alerts with bits-equivalent tiers",
                    "Channel memberships join/upgrade/cancel events",
                    "Subscriber milestone triggers",
                ],
                kind: PlatformKind::Platform,
            },
        )),
        "kick" => Some((
            palette.info,
            GenericPlatform {
                name: "Kick",
                letter: "K",
                since: "Coming in beta-2",
                description: "Chat, channel events, subscribers — newer streaming platform.",
                features: &[
                    "Chat over Kick WebSocket (community implementation)",
                    "Subscription and follow events",
                    "Channel raid detection",
                ],
                kind: PlatformKind::Platform,
            },
        )),
        "trovo" => Some((
            palette.success,
            GenericPlatform {
                name: "Trovo",
                letter: "V",
                since: "Coming in beta-3",
                description: "Chat, spells, mana, subscribers — Tencent streaming platform.",
                features: &[
                    "Chat over Trovo IRC bridge",
                    "Mana spell triggers",
                    "Subscriber and follow events",
                ],
                kind: PlatformKind::Platform,
            },
        )),
        "vtube" => Some((
            palette.warning,
            GenericPlatform {
                name: "VTube Studio",
                letter: "V",
                since: "Coming in beta-6",
                description: "Vtuber avatar control: hotkeys, expressions, item triggers.",
                features: &[
                    "Trigger hotkeys from chat events",
                    "Switch expressions and outfits",
                    "Spawn item drops on bits/subs",
                ],
                kind: PlatformKind::StreamApp,
            },
        )),
        _ => None,
    }
}

pub fn platform_generic_view<'a>(
    color: Color,
    info: GenericPlatform,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mono = font(FontRole::Monospace);

    let letter_box = container(
        text(info.letter)
            .size(22.0)
            .color(p.shell)
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..iced::Font::DEFAULT
            }),
    )
    .width(48.0)
    .height(48.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius(Radius::Md).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let name_text = text(info.name).size(FONT_MD).color(p.text_primary);

    let since_badge = container(
        text(info.since)
            .size(FONT_XS)
            .color(p.warning)
            .font(font(FontRole::Body)),
    )
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    });

    let name_row = row![name_text, since_badge]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);

    let desc = text(info.description).size(FONT_SM).color(p.text_muted);

    let info_col = column![name_row, desc].spacing(spf(Spacing::Xxs));

    let hero_row = row![letter_box, container(info_col).width(Length::Fill)]
        .spacing(spf(Spacing::Md))
        .align_y(Alignment::Center);

    let hero_card = container(hero_row)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.elevated)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        });

    let features_label = text("WHAT YOU'LL BE ABLE TO DO")
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let mut features_col = column![features_label]
        .spacing(spf(Spacing::Xs))
        .padding([sp(Spacing::Sm), 0]);
    for feature in info.features {
        let check_icon = tabler_icon(Icon::CircleCheck, 14.0, p.text_faint);
        let feature_row = row![
            check_icon,
            text(*feature).size(FONT_SM).color(p.text_secondary),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);
        features_col = features_col.push(feature_row);
    }

    let footer_kind_label = match info.kind {
        PlatformKind::Platform => "Streaming platform",
        PlatformKind::StreamApp => "Stream app",
    };
    let footer = container(
        text(format!(
            "{footer_kind_label} \u{00b7} {since} \u{00b7} not yet implemented",
            since = info.since.to_lowercase(),
        ))
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono),
    )
    .padding([sp(Spacing::Xs), sp(Spacing::Sm)])
    .width(Length::Fill)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.shell)),
        border: Border {
            color: p.border_regular,
            width: 0.5,
            radius: radius(Radius::Md).into(),
        },
        ..container::Style::default()
    });

    let body = column![hero_card, features_col, footer].spacing(spf(Spacing::Sm));

    let page_header =
        crate::app::simple_page_header(&[("Builtin", false), (info.name, true)], palette);

    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: spf(Spacing::Md),
            right: spf(Spacing::Lg),
            bottom: spf(Spacing::Md),
            left: spf(Spacing::Lg),
        });

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
