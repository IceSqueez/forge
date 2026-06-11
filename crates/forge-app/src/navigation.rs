use forge_platform_core::BuiltinId;
use forge_widgets::icons::Icon;
use forge_widgets::{ForgePalette, NavItem, Sidebar};
use iced::{Element, Length, Task};

use crate::app::App;
use crate::message::{
    ActionsMsg, GlobalsMsg, HomeMsg, LiveChatMsg, QueuesMsg, SettingsAudioMsg, SoundboardMsg,
};
use crate::script_editor::ScriptEditorMsg;
use crate::settings_websocket::SettingsWebSocketMsg;
use crate::viewers::ViewersMsg;
use crate::{Message, Screen, SettingsSection, TtsSection};

pub(crate) fn coming_soon_view(
    screen_label: String,
    palette: &ForgePalette,
) -> Element<'static, Message> {
    iced::widget::container(forge_widgets::empty_state(
        forge_widgets::tr!("nav_coming_soon"),
        screen_label,
        None::<(&str, Message)>,
        palette,
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(crate) fn breadcrumb_icon_for(screen: &Screen) -> Icon {
    match screen {
        Screen::Home => Icon::Home,
        Screen::Actions | Screen::ActionEditor(_) | Screen::Queues | Screen::TriggersRegistry => {
            Icon::Bolt
        }
        Screen::Platforms => Icon::Broadcast,
        Screen::StreamApps | Screen::Builtin | Screen::BuiltinDetail(_) => Icon::LayoutGrid,
        Screen::LiveChat => Icon::MessageCircle,
        Screen::EventFeed => Icon::Activity,
        Screen::Globals => Icon::Variable,
        Screen::Settings(_) => Icon::Settings,
        Screen::Tts(_) => Icon::Volume,
        Screen::Soundboard => Icon::Music,
        Screen::ScriptEditor | Screen::ScriptingApiDocs => Icon::Terminal,
        Screen::Server | Screen::Logs => Icon::Settings,
    }
}

pub(crate) fn screen_label(screen: &Screen) -> String {
    match screen {
        Screen::Home => forge_widgets::tr!("nav_home"),
        Screen::Actions | Screen::ActionEditor(_) => forge_widgets::tr!("nav_actions"),
        Screen::Queues => forge_widgets::tr!("nav_queues"),
        Screen::TriggersRegistry => forge_widgets::tr!("nav_triggers"),
        Screen::Platforms => forge_widgets::tr!("nav_platforms"),
        Screen::StreamApps => forge_widgets::tr!("nav_stream_apps"),
        Screen::Builtin => forge_widgets::tr!("nav_builtin"),
        Screen::BuiltinDetail(_) => forge_widgets::tr!("nav_integration"),
        Screen::LiveChat => forge_widgets::tr!("nav_live_chat"),
        Screen::EventFeed => forge_widgets::tr!("nav_event_feed"),
        Screen::Globals => forge_widgets::tr!("nav_globals"),
        Screen::Settings(_) => forge_widgets::tr!("nav_settings"),
        Screen::Tts(_) => forge_widgets::tr!("nav_tts"),
        Screen::Soundboard => forge_widgets::tr!("nav_soundboard"),
        Screen::ScriptEditor => forge_widgets::tr!("nav_script_editor"),
        Screen::ScriptingApiDocs => forge_widgets::tr!("nav_api_reference"),
        Screen::Server => forge_widgets::tr!("nav_server"),
        Screen::Logs => forge_widgets::tr!("nav_logs"),
    }
}

pub(crate) fn builtin_active(screen: &Screen, id: &str) -> bool {
    matches!(screen, Screen::BuiltinDetail(s) if s.as_str() == id)
}

pub(crate) fn nav_items_for<'a>(app: &'a App, palette: &'a ForgePalette) -> Sidebar<Message> {
    let is_home = matches!(app.screen, Screen::Home);
    let is_actions = matches!(app.screen, Screen::Actions | Screen::ActionEditor(_));
    let is_queues = matches!(app.screen, Screen::Queues);
    let is_triggers_registry = matches!(app.screen, Screen::TriggersRegistry);
    let is_live_chat = matches!(app.screen, Screen::LiveChat);
    let is_event_feed = matches!(app.screen, Screen::EventFeed);
    let is_globals = matches!(app.screen, Screen::Globals);
    let is_soundboard = matches!(app.screen, Screen::Soundboard);
    let is_tts = matches!(app.screen, Screen::Tts(_));
    let is_server = matches!(app.screen, Screen::Server);
    let is_settings = matches!(app.screen, Screen::Settings(_));

    let twitch_target = Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("twitch")));
    let obs_target = Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("obs")));

    let items = vec![
        NavItem::Leaf {
            icon: Icon::Home,
            label: forge_widgets::tr!("nav_item_home"),
            active: is_home,
            on_press: Message::Navigate(Screen::Home),
        },
        NavItem::Section(forge_widgets::tr!("nav_section_audience")),
        NavItem::Leaf {
            icon: Icon::MessageCircle,
            label: forge_widgets::tr!("nav_item_chat"),
            active: is_live_chat,
            on_press: Message::Navigate(Screen::LiveChat),
        },
        NavItem::Section(forge_widgets::tr!("nav_section_automation")),
        NavItem::Leaf {
            icon: Icon::Bolt,
            label: forge_widgets::tr!("nav_item_actions"),
            active: is_actions,
            on_press: Message::Navigate(Screen::Actions),
        },
        NavItem::Leaf {
            icon: Icon::Bolt,
            label: forge_widgets::tr!("nav_item_triggers"),
            active: is_triggers_registry,
            on_press: Message::Navigate(Screen::TriggersRegistry),
        },
        NavItem::Leaf {
            icon: Icon::Notebook,
            label: forge_widgets::tr!("nav_item_queues"),
            active: is_queues,
            on_press: Message::Navigate(Screen::Queues),
        },
        NavItem::Leaf {
            icon: Icon::Activity,
            label: forge_widgets::tr!("nav_item_event_feed"),
            active: is_event_feed,
            on_press: Message::Navigate(Screen::EventFeed),
        },
        NavItem::Leaf {
            icon: Icon::Variable,
            label: forge_widgets::tr!("nav_item_globals"),
            active: is_globals,
            on_press: Message::Navigate(Screen::Globals),
        },
        NavItem::Section(forge_widgets::tr!("nav_section_connections")),
        NavItem::MiniLabel(forge_widgets::tr!("nav_item_platforms")),
        NavItem::FlatLink {
            dot_color: palette.brand,
            label: "Twitch".to_owned(),
            active: builtin_active(&app.screen, "twitch"),
            on_press: twitch_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.random,
            label: "YouTube".to_owned(),
            active: builtin_active(&app.screen, "youtube"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("youtube"))),
        },
        NavItem::FlatLink {
            dot_color: palette.info,
            label: "Kick".to_owned(),
            active: builtin_active(&app.screen, "kick"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("kick"))),
        },
        NavItem::MiniLabel(forge_widgets::tr!("nav_item_stream_apps")),
        NavItem::FlatLink {
            dot_color: palette.success,
            label: "OBS Studio".to_owned(),
            active: builtin_active(&app.screen, "obs"),
            on_press: obs_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.warning,
            label: "VTube Studio".to_owned(),
            active: builtin_active(&app.screen, "vtube"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("vtube"))),
        },
        NavItem::Leaf {
            icon: Icon::Music,
            label: forge_widgets::tr!("nav_item_soundboard"),
            active: is_soundboard,
            on_press: Message::Navigate(Screen::Soundboard),
        },
        NavItem::Leaf {
            icon: Icon::Volume,
            label: forge_widgets::tr!("nav_item_tts"),
            active: is_tts,
            on_press: Message::Navigate(Screen::Tts(TtsSection::Dashboard)),
        },
        NavItem::Leaf {
            icon: Icon::Server,
            label: forge_widgets::tr!("nav_item_ws_server"),
            active: is_server,
            on_press: Message::Navigate(Screen::Server),
        },
    ];

    let bottom_items = vec![
        NavItem::Divider,
        NavItem::Leaf {
            icon: Icon::Settings,
            label: forge_widgets::tr!("nav_item_settings"),
            active: is_settings,
            on_press: Message::Navigate(Screen::Settings(SettingsSection::Appearance)),
        },
    ];

    Sidebar {
        items,
        bottom_items,
    }
}

pub(crate) fn handle_navigate(app: &mut App, screen: Screen) -> Task<Message> {
    let is_actions = matches!(screen, Screen::Actions);
    let is_queues = matches!(screen, Screen::Queues);
    let is_triggers_registry = matches!(screen, Screen::TriggersRegistry);
    let is_live_chat = matches!(screen, Screen::LiveChat);
    let is_hub = matches!(screen, Screen::Home);
    let is_globals = matches!(screen, Screen::Globals);
    let is_script_editor = matches!(screen, Screen::ScriptEditor);
    let is_soundboard = matches!(screen, Screen::Soundboard);
    let is_settings_audio = matches!(screen, Screen::Settings(SettingsSection::Audio));
    let is_settings_ws = matches!(screen, Screen::Settings(SettingsSection::WebSocket));
    let is_settings_hotkeys = matches!(screen, Screen::Settings(SettingsSection::Hotkeys));
    let is_settings_scripting = matches!(screen, Screen::Settings(SettingsSection::Scripting));
    let editor_id = if let Screen::ActionEditor(id) = &screen {
        Some(*id)
    } else {
        None
    };
    app.screen = screen;
    if is_actions {
        Task::done(Message::Actions(ActionsMsg::LoadRequested))
    } else if is_triggers_registry {
        Task::done(Message::TriggersRegistry(
            crate::triggers_registry::TriggersRegistryMsg::LoadRequested,
        ))
    } else if is_queues {
        Task::done(Message::Queues(QueuesMsg::LoadRequested))
    } else if is_live_chat {
        Task::batch([
            Task::done(Message::Viewers(ViewersMsg::LoadRequested)),
            Task::done(Message::LiveChat(LiveChatMsg::LoadDrawerWidth)),
        ])
    } else if is_hub {
        Task::done(Message::Home(HomeMsg::LoadStats))
    } else if is_globals {
        Task::done(Message::Globals(GlobalsMsg::LoadRequested))
    } else if is_script_editor {
        Task::done(Message::ScriptEditor(ScriptEditorMsg::LoadRequested))
    } else if is_soundboard {
        Task::done(Message::Soundboard(SoundboardMsg::LoadRequested))
    } else if is_settings_audio {
        Task::done(Message::SettingsAudio(SettingsAudioMsg::LoadRequested))
    } else if is_settings_ws {
        Task::done(Message::SettingsWebSocket(
            SettingsWebSocketMsg::LoadRequested,
        ))
    } else if is_settings_hotkeys {
        Task::done(Message::SettingsHotkeys(
            crate::settings_hotkeys::SettingsHotkeysMsg::Enter,
        ))
    } else if is_settings_scripting {
        Task::done(Message::Settings(crate::message::SettingsMsg::Scripting(
            crate::settings_scripting::ScriptingSettingsMsg::LoadRequested,
        )))
    } else if let Some(id) = editor_id {
        let needs_load = app
            .ui
            .actions
            .detail
            .as_ref()
            .map(|d| d.action.id != id)
            .unwrap_or(true);
        if needs_load {
            Task::batch([
                Task::done(Message::Actions(ActionsMsg::LoadRequested)),
                Task::done(Message::Actions(ActionsMsg::ActionSelected(id))),
            ])
        } else {
            Task::none()
        }
    } else {
        Task::none()
    }
}
