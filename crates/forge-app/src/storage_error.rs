use forge_widgets::tokens::{FONT_LG, FONT_SM, Spacing, sp, spf};
use forge_widgets::{FontRole, ForgePalette, Icon, Radius, card, font, radius, tabler_icon, tr};
use iced::widget::{column, container, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::message::Message;

pub fn storage_error_view<'a>(reason: &str, palette: &ForgePalette) -> Element<'a, Message> {
    let icon = tabler_icon(Icon::AlertTriangle, 36.0, palette.warning);

    let title = text(tr!("storage_error_title"))
        .size(FONT_LG)
        .color(palette.text_primary);

    let overlay_bg = palette.surface_overlay;
    let detail = container(scrollable(
        text(reason.to_owned())
            .size(FONT_SM)
            .font(font(FontRole::Monospace))
            .color(palette.text_secondary),
    ))
    .padding(sp(Spacing::Sm))
    .width(Length::Fill)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(iced::Background::Color(overlay_bg)),
        border: iced::Border {
            radius: radius(Radius::Sm).into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    });

    let safe = text(tr!("storage_error_data_safe"))
        .size(FONT_SM)
        .color(palette.text_muted);

    let report = text(tr!("storage_error_report"))
        .size(FONT_SM)
        .color(palette.text_muted);

    let body = column![icon, title, detail, safe, report]
        .spacing(spf(Spacing::Md))
        .align_x(Alignment::Center)
        .max_width(560.0);

    container(card([body.into()], palette))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(sp(Spacing::Lg))
        .into()
}
