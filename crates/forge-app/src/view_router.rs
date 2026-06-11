use forge_widgets::tokens::{Spacing, spf};
use forge_widgets::{
    BreadcrumbCrumb, app_footer, breadcrumb, page_shell, sidebar, title_bar, toast_viewport,
};
use iced::{Element, Length};

use crate::Screen;
use crate::action_editor_view::action_editor_view;
use crate::app::{App, format_uptime, subsystem_connectivity};
use crate::builtin_detail::view as builtin_detail_view;
use crate::event_feed::event_feed_view;
use crate::globals_view::globals_view;
use crate::live_chat_view::live_chat_view;
use crate::message::{Message, ToastMsg};
use crate::navigation;
use crate::queues_view::queues_view;
use crate::script_editor::script_editor_view;
use crate::server_screen::server_screen_view;
use crate::soundboard::soundboard_view;
use crate::stream_apps::view as stream_apps_view;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = &app.palette;

    let elapsed = app.boot_time.elapsed().unwrap_or_default();
    let version = env!("CARGO_PKG_VERSION");

    let chrome_title = title_bar(palette);
    let (conn_n, conn_total) = subsystem_connectivity(app);
    let uptime_str = format_uptime(elapsed);
    let chrome_footer = app_footer(conn_n, conn_total, &uptime_str, version, palette);

    let crumb_bar = breadcrumb(
        vec![BreadcrumbCrumb {
            icon: Some(navigation::breadcrumb_icon_for(&app.screen)),
            label: navigation::screen_label(&app.screen),
            on_press: None::<Message>,
        }],
        palette,
    );

    let sidebar = sidebar(palette, navigation::nav_items_for(app, palette));

    let screen_content: Element<'_, Message> = match &app.screen {
        Screen::Home => crate::home::home_view(app, palette),
        Screen::LiveChat => live_chat_view(&app.ui.live_chat, &app.ui.viewers, palette),
        Screen::Globals => globals_view(app, palette),
        Screen::Actions => crate::actions_view::actions_view(app, palette),
        Screen::ActionEditor(id) => action_editor_view(app, *id, palette),
        Screen::Queues => queues_view(&app.ui.queues, palette),
        Screen::TriggersRegistry => {
            crate::triggers_registry::view(&app.ui.triggers_registry, &app.rt, palette)
        }
        Screen::Settings(section) => crate::settings::settings_view(
            crate::settings::SettingsViewParams {
                section,
                ws: &app.ui.settings_websocket,
                server: &app.ui.server_screen,
                audio: &app.ui.settings_audio,
                hotkeys: &app.ui.settings_hotkeys,
                scripting: &app.ui.settings_scripting,
                shortcuts: &app.ui.settings_shortcuts,
                rt: &app.rt,
                current_language: app.language,
                current_density: app.density,
                fonts: &app.fonts,
            },
            palette,
        ),
        Screen::ScriptEditor => script_editor_view(app, palette),
        Screen::ScriptingApiDocs => {
            crate::scripting_api_docs::scripting_api_docs_view(&app.ui.script_editor, palette)
        }
        Screen::Platforms => crate::platforms_view::platforms_overview_view(app, palette),
        Screen::StreamApps => stream_apps_view(app, palette),
        Screen::EventFeed => event_feed_view(&app.ui.event_feed, palette),
        Screen::Server => server_screen_view(&app.ui.server_screen, palette),
        Screen::BuiltinDetail(id) => {
            if id.as_str() == "twitch" && app.rt.twitch_chat_handle.is_none() {
                crate::twitch_panel::twitch_disconnected_view(&app.ui.twitch_panel, palette)
            } else if id.as_str() == "obs" && app.rt.obs_client.is_none() {
                crate::obs_panel::obs_disconnected_view(&app.ui.obs_panel, palette)
            } else if let Some((color, info)) = crate::platform_generic::registry(id, palette) {
                let oauth_for_this = info
                    .connect_platform
                    .filter(|p| {
                        *p == app.ui.local_callback_flow.platform
                            && app.ui.local_callback_flow.phase
                                != crate::local_callback_flow::LocalCallbackFlowPhase::Idle
                    })
                    .is_some();
                if oauth_for_this {
                    crate::local_callback_flow::view(&app.ui.local_callback_flow, palette)
                } else {
                    crate::platform_generic::platform_generic_view(color, info, palette)
                }
            } else if let Some(state) = app.ui.builtin_detail.as_ref() {
                let inner = builtin_detail_view(state, palette);
                if id.as_str() == "twitch" && app.rt.twitch_reauth_required {
                    iced::widget::container(
                        iced::widget::column![
                            crate::twitch_panel::twitch_reauth_banner(palette),
                            inner,
                        ]
                        .spacing(spf(Spacing::Sm)),
                    )
                    .padding(iced::Padding::from([12_u16, 14_u16]))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                } else {
                    inner
                }
            } else {
                iced::widget::container(forge_widgets::empty_state(
                    "Not connected",
                    "Open this integration in Platforms or Stream Apps to connect.",
                    None::<(&str, Message)>,
                    palette,
                ))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        }
        Screen::Soundboard => soundboard_view(&app.ui.soundboard, palette),
        Screen::Tts(section) => crate::tts_view::tts_section_view(app, section, palette),
        other => navigation::coming_soon_view(format!("{other:?}"), palette),
    };

    let screen_uses_own_header = matches!(
        &app.screen,
        Screen::Actions
            | Screen::ActionEditor(_)
            | Screen::LiveChat
            | Screen::Home
            | Screen::Globals
            | Screen::Queues
            | Screen::TriggersRegistry
            | Screen::EventFeed
            | Screen::Platforms
            | Screen::StreamApps
            | Screen::BuiltinDetail(_)
            | Screen::Settings(_)
            | Screen::Server
            | Screen::ScriptEditor
            | Screen::Soundboard
            | Screen::Tts(_)
    );
    let content: Element<'_, Message> = if screen_uses_own_header {
        iced::widget::column![screen_content]
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    } else {
        iced::widget::column![crumb_bar, screen_content]
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    };

    let main_view = page_shell(chrome_title, None, sidebar, content, Some(chrome_footer));
    let toast_layer = toast_viewport(
        &app.toast_queue,
        |id| Message::Toast(ToastMsg::Dismissed(id)),
        palette,
    );
    iced::widget::stack![main_view, toast_layer].into()
}
