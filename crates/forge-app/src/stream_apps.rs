use forge_platform_core::{BuiltinId, ConnectionState};
use forge_widgets::{
    ForgePalette, Icon, tabler_icon,
    tokens::{FONT_MD, FONT_SM, Radius, Spacing, radius, spf},
};
use iced::{
    Alignment, Background, Border, Color, Element, Length,
    widget::{column, container, row, scrollable, text},
};

use crate::{App, Message};

pub fn view<'a>(state: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;

    let title = text(forge_widgets::tr!("stream_apps.title"))
        .size(FONT_MD)
        .color(p.text_primary);
    let subtitle = text(forge_widgets::tr!("stream_apps.subtitle"))
        .size(FONT_SM)
        .color(p.text_muted);
    let header = column![title, subtitle].spacing(spf(Spacing::Xxs));

    let obs_connected = matches!(
        state.rt.obs_client.as_ref().map(|c| c.connection_state()),
        Some(ConnectionState::Connected)
    );

    let obs_card = crate::platforms_view::overview_card(
        app_icon_tile(Icon::Broadcast, p.success, palette),
        "OBS Studio",
        forge_widgets::tr!("stream_apps.obs.desc"),
        &[],
        obs_connected,
        BuiltinId::new("obs"),
        palette,
    );
    let vtube_connected = matches!(
        state.rt.vtube_client.as_ref().map(|c| c.connection_state()),
        Some(ConnectionState::Connected)
    );

    let vtube_card = crate::platforms_view::overview_card(
        app_icon_tile(Icon::MoodSmile, p.warning, palette),
        "VTube Studio",
        forge_widgets::tr!("stream_apps.vtube.desc"),
        &[],
        vtube_connected,
        BuiltinId::new("vtube"),
        palette,
    );

    let grid = row![obs_card, vtube_card]
        .spacing(spf(Spacing::Sm))
        .width(Length::Fill);

    let body = column![header, grid].spacing(spf(Spacing::Md));
    let page_header = forge_widgets::breadcrumb(
        vec![forge_widgets::BreadcrumbCrumb::leaf(forge_widgets::tr!(
            "stream_apps.breadcrumb"
        ))],
        None,
        palette,
    );
    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(spf(Spacing::Lg));

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Rounded surface tile holding a stream-app glyph, used as the `leading`
/// visual for [`crate::platforms_view::overview_card`].
fn app_icon_tile<'a>(
    icon: Icon,
    icon_color: Color,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    let p = *palette;
    container(tabler_icon(icon, 22.0, icon_color))
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
        })
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

    #[test]
    #[allow(clippy::expect_used)]
    fn stream_apps_view_with_vtube_connected_does_not_panic() {
        use forge_vtube::{VTubeClient, VTubeConfig};
        use std::sync::Arc;

        let mut app = App::default();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let _enter = rt.enter();
        let client = Arc::new(VTubeClient::connect(
            VTubeConfig::default(),
            Arc::clone(&app.rt.bus) as Arc<dyn forge_events::EventPublisher>,
            Arc::clone(&app.rt.backend) as Arc<dyn forge_storage::CredentialsRepo>,
        ));
        app.rt.vtube_client = Some(client);
        let _ = view(&app, &app.palette.clone());
    }
}
