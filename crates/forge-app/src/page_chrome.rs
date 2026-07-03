use forge_widgets::ForgePalette;
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_SM, Spacing, spf};
use iced::widget::{container, row, text};
use iced::{Background, Border, Element, Length};

use crate::message::Message;

pub(crate) fn simple_page_header<'a>(
    crumbs: &[(String, bool)],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    page_header_with_actions(crumbs, None, palette)
}

pub(crate) fn page_header_with_actions<'a>(
    crumbs: &[(String, bool)],
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
        crumb_row = crumb_row.push(text(label.clone()).size(FONT_SM).color(color));
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
