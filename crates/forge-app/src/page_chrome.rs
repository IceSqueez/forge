use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_MD, FONT_SM, Spacing, sp, spf};
use forge_widgets::{ForgePalette, Radius, radius};
use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Element, Length};

use crate::message::Message;

pub(crate) fn simple_page_header<'a>(
    crumbs: &[(&'a str, bool)],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    page_header_with_actions(crumbs, None, palette)
}

pub(crate) fn page_header_with_actions<'a>(
    crumbs: &[(&'a str, bool)],
    right: Option<Element<'a, Message>>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    let mut crumb_row = row![tabler_icon(Icon::Home, 13.0, p.text_faint)]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    for (label, is_last) in crumbs {
        crumb_row = crumb_row.push(tabler_icon(Icon::ChevronRight, 11.0, p.text_faint));
        let color = if *is_last {
            p.text_primary
        } else {
            p.text_muted
        };
        crumb_row = crumb_row.push(text(label.to_string()).size(FONT_SM).color(color));
    }

    let inner: Element<'a, Message> = if let Some(right_el) = right {
        row![
            crumb_row,
            iced::widget::Space::new().width(Length::Fill),
            right_el,
        ]
        .align_y(iced::alignment::Vertical::Center)
        .into()
    } else {
        crumb_row.into()
    };

    container(inner)
        .width(Length::Fill)
        .padding([10_u16, 16_u16])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub(crate) fn header_divider<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    iced::widget::container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        })
        .into()
}

pub(crate) fn sheet_chrome<'a>(
    title: &'a str,
    on_close: Message,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let title_el = text(title).size(FONT_MD).color(p.text_primary);

    let close_btn = button(tabler_icon(Icon::X, 14.0, p.text_muted))
        .on_press(on_close)
        .padding(sp(Spacing::Xs))
        .style(move |_t: &iced::Theme, status| {
            let bg = match status {
                iced::widget::button::Status::Hovered => {
                    Some(iced::Background::Color(p.surface_overlay))
                }
                _ => None,
            };
            iced::widget::button::Style {
                background: bg,
                border: iced::Border {
                    radius: radius(Radius::Sm).into(),
                    ..Default::default()
                },
                text_color: iced::Color::TRANSPARENT,
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    let header = container(
        row![
            title_el,
            iced::widget::Space::new().width(Length::Fill),
            close_btn,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([12_u16, 16_u16])
    .width(Length::Fill)
    .style(move |_t: &iced::Theme| container::Style {
        border: iced::Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let body_wrap = container(body).width(Length::Fill).height(Length::Fill);

    let mut col = column![header, body_wrap]
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(footer_el) = footer {
        let footer_container = container(footer_el)
            .padding([12_u16, 16_u16])
            .width(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                border: iced::Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });
        col = col.push(footer_container);
    }

    col.into()
}
