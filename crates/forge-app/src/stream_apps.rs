use forge_platform_core::{BuiltinId, ConnectionState};
use forge_widgets::{
    ForgePalette, Icon, tabler_icon,
    tokens::{FONT_MD, FONT_SM, FONT_XS, Radius, Spacing, radius, sp, spf},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow,
    widget::{button, column, container, row, scrollable, text},
};

use crate::{App, Message, Screen};

pub fn view<'a>(state: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;

    let title = text("Stream apps").size(FONT_MD).color(p.text_primary);
    let subtitle =
        text("Local apps Forge talks to over WebSocket. Connect to control them from actions.")
            .size(FONT_SM)
            .color(p.text_muted);
    let header = column![title, subtitle].spacing(spf(Spacing::Xxs));

    let obs_connected = matches!(
        state.rt.obs_client.as_ref().map(|c| c.connection_state()),
        Some(ConnectionState::Connected)
    );

    let obs_card = app_overview_card(
        Icon::Broadcast,
        p.success,
        "OBS Studio",
        "Scenes, sources, recording control, replay buffers — full obs-websocket API",
        obs_connected,
        BuiltinId::new("obs"),
        palette,
    );
    let vtube_card = app_overview_card(
        Icon::MoodSmile,
        p.warning,
        "VTube Studio",
        "Vtuber avatar control: hotkeys, expressions, item triggers",
        false,
        BuiltinId::new("vtube"),
        palette,
    );

    let grid = row![obs_card, vtube_card]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);

    let body = column![header, grid].spacing(spf(Spacing::Md));
    let page_header = crate::app::simple_page_header(&[("Stream Apps", true)], palette);
    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(spf(Spacing::Lg));

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn app_overview_card<'a>(
    icon: Icon,
    icon_color: Color,
    name: &'a str,
    desc: &'a str,
    connected: bool,
    target: BuiltinId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;

    let icon_box = container(tabler_icon(icon, 22.0, icon_color))
        .width(44.0)
        .height(44.0)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.surface_overlay)),
            border: Border {
                radius: radius(Radius::Md).into(),
                color: Color::TRANSPARENT,
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
    .padding([sp(Spacing::Xxs), sp(Spacing::Xs)])
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

    let info_col = column![title_row, desc_text].spacing(spf(Spacing::Xs));

    let inner = row![
        icon_box,
        container(info_col).width(Length::Fill),
        tabler_icon(Icon::ChevronRight, 16.0, p.text_faint),
    ]
    .spacing(spf(Spacing::Sm))
    .align_y(Alignment::Start);

    button(inner)
        .padding([sp(Spacing::Md), sp(Spacing::Md)])
        .width(Length::Fill)
        .on_press(Message::Navigate(Screen::BuiltinDetail(target)))
        .style(
            move |_: &iced::Theme, status: iced::widget::button::Status| {
                let hovered = matches!(
                    status,
                    iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                );
                button::Style {
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
                    shadow: Shadow::default(),
                    snap: false,
                }
            },
        )
        .into()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::BuiltinId;

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
    fn obs_card_on_press_navigates_to_builtin_detail() {
        let on_press = Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("obs")));
        assert!(matches!(
            on_press,
            Message::Navigate(Screen::BuiltinDetail(ref id)) if id == &BuiltinId::new("obs")
        ));
    }
}
