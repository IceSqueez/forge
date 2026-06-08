use forge_platform_core::BuiltinId;
use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_MD, FONT_SM, FONT_XS, Radius, Spacing, radius, spf};
use iced::Element;

use crate::app::App;
use crate::page_chrome::simple_page_header;
use crate::{Message, Screen};

#[allow(clippy::too_many_arguments)]
fn platform_overview_card<'a>(
    letter: &'static str,
    color: iced::Color,
    name: &'a str,
    desc: &'a str,
    features: &'static [&'static str],
    connected: bool,
    target: BuiltinId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let p = *palette;

    let letter_box = container(text(letter).size(22.0).color(p.shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(44.0)
    .height(44.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius(Radius::Md).into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let dot_color = if connected { p.success } else { p.text_faint };
    let dot = container(iced::widget::Space::new())
        .width(5.0)
        .height(5.0)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 2.5.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

    let badge_label = if connected {
        "Connected"
    } else {
        "Not connected"
    };
    let badge_text_color = if connected { p.success } else { p.text_muted };
    let badge = container(
        row![
            dot,
            text(badge_label.to_owned())
                .size(FONT_XS)
                .color(badge_text_color),
        ]
        .spacing(spf(Spacing::Xxs))
        .align_y(Alignment::Center),
    )
    .padding([2_u16, 7_u16])
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(p.surface_overlay)),
        border: Border {
            radius: radius(Radius::Sm).into(),
            ..Border::default()
        },
        ..container::Style::default()
    });

    let title_row = row![
        text(name.to_owned()).size(FONT_SM).color(p.text_primary),
        badge,
    ]
    .spacing(spf(Spacing::Xs))
    .align_y(Alignment::Center);

    let desc_text = text(desc.to_owned()).size(FONT_SM).color(p.text_muted);

    let mut chip_row = iced::widget::Row::new().spacing(spf(Spacing::Xxs));
    for f in features {
        let chip = container(text(*f).size(FONT_XS).color(p.text_secondary))
            .padding([2_u16, 7_u16])
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(p.shell)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });
        chip_row = chip_row.push(chip);
    }

    let info_col = column![title_row, desc_text, chip_row.wrap()].spacing(spf(Spacing::Xs));

    let inner = row![
        letter_box,
        container(info_col).width(Length::Fill),
        tabler_icon(Icon::ChevronRight, 16.0, p.text_faint),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(Alignment::Start);

    button(inner)
        .padding([16_u16, 18_u16])
        .width(Length::Fill)
        .on_press(Message::Navigate(Screen::BuiltinDetail(target)))
        .style(
            move |_: &iced::Theme, status: iced::widget::button::Status| {
                let hovered = matches!(
                    status,
                    iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                );
                iced::widget::button::Style {
                    background: Some(Background::Color(p.elevated)),
                    border: Border {
                        color: if hovered {
                            p.border_input
                        } else {
                            p.border_regular
                        },
                        width: 0.5,
                        radius: radius(Radius::Md).into(),
                    },
                    text_color: p.text_primary,
                    shadow: iced::Shadow::default(),
                    snap: false,
                }
            },
        )
        .into()
}

pub(crate) fn platforms_overview_view<'a>(
    app: &'a App,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};
    use iced::{Length, Padding};

    let p = *palette;

    let title = text("Streaming platforms")
        .size(FONT_MD)
        .color(p.text_primary);
    let subtitle = text("Connect once, Forge listens to all chats and events in one place.")
        .size(FONT_SM)
        .color(p.text_muted);
    let header = column![title, subtitle].spacing(spf(Spacing::Xxs));

    let twitch_connected = app.rt.twitch_chat_handle.is_some();

    let twitch_card = platform_overview_card(
        "T",
        p.platform_twitch,
        "Twitch",
        "Chat, EventSub subscriptions, channel points, bits, raids",
        &["IRC chat", "EventSub", "Channel points", "Bits & subs"],
        twitch_connected,
        BuiltinId::new("twitch"),
        palette,
    );
    let youtube_card = platform_overview_card(
        "Y",
        p.platform_youtube,
        "YouTube",
        "Live chat, super chats, channel memberships, subscribers",
        &["Live chat", "Super chat", "Memberships"],
        false,
        BuiltinId::new("youtube"),
        palette,
    );
    let kick_card = platform_overview_card(
        "K",
        p.platform_kick,
        "Kick",
        "Chat, channel events, subscribers — newer streaming platform",
        &["Chat", "Subs", "Channel events"],
        false,
        BuiltinId::new("kick"),
        palette,
    );
    let grid_row_1 = row![twitch_card, youtube_card]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);
    let grid_row_2 = row![kick_card]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);
    let grid = column![grid_row_1, grid_row_2].spacing(spf(Spacing::Sm));

    let body = column![header, grid].spacing(spf(Spacing::Md));
    let page_header = simple_page_header(&[("Platforms", true)], palette);
    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 22.0,
            right: 28.0,
            bottom: 22.0,
            left: 28.0,
        });

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
