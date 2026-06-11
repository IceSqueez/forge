use forge_platform_core::BuiltinId;
use forge_types::PlatformId;
use forge_widgets::{
    ForgePalette, Icon, tabler_icon,
    tokens::{FONT_MD, FONT_SM, FONT_XS, FontRole, Radius, Spacing, font, radius, sp, spf},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Padding,
    widget::{button, column, container, row, scrollable, text},
};

use crate::Message;
use crate::local_callback_flow::LocalCallbackFlowMsg;

pub struct GenericPlatform {
    pub name: &'static str,
    pub letter: &'static str,
    pub status_badge: String,
    pub description: String,
    pub features: Vec<String>,
    pub kind: PlatformKind,
    pub connect_platform: Option<PlatformId>,
    pub status: PlatformStatus,
}

pub enum PlatformKind {
    Platform,
    StreamApp,
}

pub enum PlatformStatus {
    Available,
    Coming,
}

pub fn registry(id: &BuiltinId, palette: &ForgePalette) -> Option<(Color, GenericPlatform)> {
    match id.as_str() {
        "youtube" => Some((
            palette.platform_youtube,
            GenericPlatform {
                name: "YouTube",
                letter: "Y",
                status_badge: forge_widgets::tr!("common_status_not_connected"),
                description: forge_widgets::tr!("youtube_description"),
                features: vec![
                    forge_widgets::tr!("youtube_feature_live_chat"),
                    forge_widgets::tr!("youtube_feature_super_chat"),
                    forge_widgets::tr!("youtube_feature_memberships"),
                    forge_widgets::tr!("youtube_feature_subscribers"),
                ],
                kind: PlatformKind::Platform,
                connect_platform: Some(PlatformId::YouTube),
                status: PlatformStatus::Available,
            },
        )),
        "kick" => Some((
            palette.platform_kick,
            GenericPlatform {
                name: "Kick",
                letter: "K",
                status_badge: forge_widgets::tr!("common_status_not_connected"),
                description: forge_widgets::tr!("kick_description"),
                features: vec![
                    forge_widgets::tr!("kick_feature_live_chat"),
                    forge_widgets::tr!("kick_feature_subs"),
                    forge_widgets::tr!("kick_feature_hosts_bans"),
                    forge_widgets::tr!("kick_feature_deleted_replies"),
                ],
                kind: PlatformKind::Platform,
                connect_platform: Some(PlatformId::Kick),
                status: PlatformStatus::Available,
            },
        )),
        "vtube" => Some((
            palette.warning,
            GenericPlatform {
                name: "VTube Studio",
                letter: "V",
                status_badge: forge_widgets::tr!("common_status_coming_soon"),
                description: forge_widgets::tr!("vtube_description"),
                features: vec![
                    forge_widgets::tr!("vtube_feature_hotkeys"),
                    forge_widgets::tr!("vtube_feature_expressions"),
                    forge_widgets::tr!("vtube_feature_item_drops"),
                ],
                kind: PlatformKind::StreamApp,
                connect_platform: None,
                status: PlatformStatus::Coming,
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

    let badge_text_color = match info.status {
        PlatformStatus::Available => p.info,
        PlatformStatus::Coming => p.warning,
    };
    let status_badge = container(
        text(info.status_badge)
            .size(FONT_XS)
            .color(badge_text_color)
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

    let name_row = row![name_text, status_badge]
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

    let features_label_text = match info.status {
        PlatformStatus::Available => forge_widgets::tr!("platform_generic.features.available"),
        PlatformStatus::Coming => forge_widgets::tr!("platform_generic.features.coming"),
    };
    let features_label = text(features_label_text)
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let mut features_col = column![features_label]
        .spacing(spf(Spacing::Xs))
        .padding([sp(Spacing::Sm), 0]);
    for feature in &info.features {
        let check_icon = tabler_icon(Icon::CircleCheck, 14.0, p.text_faint);
        let feature_row = row![
            check_icon,
            text(feature.clone()).size(FONT_SM).color(p.text_secondary),
        ]
        .spacing(spf(Spacing::Xs))
        .align_y(Alignment::Center);
        features_col = features_col.push(feature_row);
    }

    let footer_kind_label = match info.kind {
        PlatformKind::Platform => forge_widgets::tr!("platform_generic.kind.platform"),
        PlatformKind::StreamApp => forge_widgets::tr!("platform_generic.kind.stream_app"),
    };
    let footer_status_label = match info.status {
        PlatformStatus::Available => forge_widgets::tr!("platform_generic.status.available"),
        PlatformStatus::Coming => forge_widgets::tr!("platform_generic.status.coming"),
    };
    let footer = container(
        text(format!(
            "{footer_kind_label} \u{00b7} {footer_status_label}"
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

    let mut body_parts: Vec<Element<'_, Message>> = vec![hero_card.into(), features_col.into()];

    if let Some(target_platform) = info.connect_platform {
        let connect_btn = button(
            row![
                tabler_icon(Icon::Lock, 14.0, p.shell),
                text(forge_widgets::tr!("platform_generic.connect_btn"))
                    .size(FONT_SM)
                    .color(p.shell),
            ]
            .spacing(spf(Spacing::Xs))
            .align_y(Alignment::Center),
        )
        .on_press(Message::LocalCallbackFlow(
            LocalCallbackFlowMsg::ConnectPlatform(target_platform),
        ))
        .padding([sp(Spacing::Xs), sp(Spacing::Md)])
        .style(move |_: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(p.brand)),
            text_color: p.shell,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius(Radius::Sm).into(),
            },
            shadow: iced::Shadow::default(),
            snap: false,
        });

        body_parts.push(
            container(connect_btn)
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into(),
        );
    }

    body_parts.push(footer.into());

    let parent_label = match info.kind {
        PlatformKind::Platform => forge_widgets::tr!("platform_generic.parent.platforms"),
        PlatformKind::StreamApp => forge_widgets::tr!("platform_generic.parent.stream_apps"),
    };

    let body = column(body_parts).spacing(spf(Spacing::Sm));

    let page_header = crate::page_chrome::simple_page_header(
        &[(parent_label, false), (info.name.to_owned(), true)],
        palette,
    );

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
