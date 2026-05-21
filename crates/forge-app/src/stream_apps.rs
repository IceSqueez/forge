use forge_platform_core::{ConnectionState, IntegrationId};
use forge_widgets::{
    BOOTSTRAP_FONT, ForgePalette, ICON_BROADCAST, StatusVariant, section_header, status_pill,
    tokens::{FONT_BODY, Radius, radius},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{button, column, container, row, text},
};

use crate::{App, Message, Screen};

pub fn view<'a>(state: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let header = section_header("STREAM APPS", None, palette);
    let obs_card = obs_card(state, palette);

    let vtube_card = coming_card("VTube Studio", "Available in beta-3", palette);

    let cards = column![obs_card, vtube_card]
        .spacing(10)
        .width(Length::Fill);

    let body = column![header, cards].spacing(12).width(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn obs_card<'a>(state: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let obs_id = IntegrationId::new("obs");
    let on_press = Message::Navigate(Screen::IntegrationDetail(obs_id));

    let icon_color = palette.success;

    let icon_box = container(
        text(ICON_BROADCAST.to_string())
            .size(20.0)
            .font(BOOTSTRAP_FONT)
            .color(icon_color),
    )
    .width(44.0)
    .height(44.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Md).into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let conn_state = state.obs_client.as_ref().map(|c| c.connection_state());
    let (pill_label, pill_variant) = match conn_state {
        Some(ConnectionState::Connected) => ("Connected", StatusVariant::Positive),
        Some(ConnectionState::Connecting) | Some(ConnectionState::Reconnecting) => {
            ("Connecting", StatusVariant::Neutral)
        }
        _ => ("Not connected", StatusVariant::Neutral),
    };
    let pill = status_pill(pill_label, pill_variant, palette);

    let name_row = row![
        text("OBS Studio")
            .size(FONT_BODY)
            .color(palette.text_primary),
        iced::widget::Space::new().width(Length::Fill),
        pill,
    ]
    .align_y(Alignment::Center)
    .spacing(8);

    let endpoint_str = state
        .obs_client
        .as_ref()
        .map(|c| c.endpoint().to_owned())
        .unwrap_or_else(|| "\u{2014}".to_owned());

    let endpoint = text(endpoint_str)
        .size(FONT_BODY)
        .color(palette.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let info_col = column![name_row, endpoint].spacing(3).width(Length::Fill);

    let open_label = text("Open").size(FONT_BODY).color(palette.text_secondary);

    let open_btn = container(open_label)
        .padding([5, 12])
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..container::Style::default()
        });

    let content = row![icon_box, info_col, open_btn]
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Length::Fill);

    button(content)
        .on_press(on_press)
        .padding(14)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg = match status {
                iced::widget::button::Status::Hovered => Color {
                    a: 1.0,
                    ..palette.elevated
                },
                _ => palette.elevated,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: palette.border_regular,
                    width: 0.5,
                    radius: radius(Radius::Md).into(),
                },
                text_color: palette.text_primary,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn coming_card<'a>(
    name: &'a str,
    note: &'a str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header_row = row![
        text(name).size(FONT_BODY).color(palette.text_muted),
        iced::widget::Space::new().width(Length::Fill),
        status_pill("Coming soon", StatusVariant::Neutral, palette),
    ]
    .align_y(Alignment::Center)
    .spacing(8);

    let subtitle = text(note).size(FONT_BODY).color(palette.text_faint);

    forge_widgets::card([header_row.into(), subtitle.into()], palette)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::IntegrationId;

    use super::*;
    use crate::app::update;
    use crate::{App, Screen};

    #[test]
    fn stream_apps_view_renders_without_panic() {
        let app = App::default();
        let _ = view(&app, &app.palette.clone());
    }

    #[test]
    fn navigate_stream_apps_sets_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::StreamApps));
        assert_eq!(app.screen, Screen::StreamApps);
    }

    #[test]
    fn obs_card_on_press_navigates_to_integration_detail() {
        let on_press = Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("obs")));
        assert!(matches!(
            on_press,
            Message::Navigate(Screen::IntegrationDetail(ref id)) if id == &IntegrationId::new("obs")
        ));
    }
}
