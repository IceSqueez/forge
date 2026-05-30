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
        "Coming soon",
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
        Screen::Platforms | Screen::DeviceCodeFlow(_) => Icon::Broadcast,
        Screen::StreamApps | Screen::Builtin | Screen::BuiltinDetail(_) => Icon::LayoutGrid,
        Screen::LiveChat => Icon::MessageCircle,
        Screen::EventFeed => Icon::Activity,
        Screen::Globals => Icon::Variable,
        Screen::Settings(_) => Icon::Settings,
        Screen::Tts(_) => Icon::Volume,
        Screen::Soundboard => Icon::Music,
        Screen::ScriptEditor => Icon::Terminal,
        Screen::Server | Screen::Logs => Icon::Settings,
    }
}

pub(crate) fn screen_label(screen: &Screen) -> &'static str {
    match screen {
        Screen::Home => "Home",
        Screen::Actions => "Actions",
        Screen::ActionEditor(_) => "Actions",
        Screen::Queues => "Queues",
        Screen::TriggersRegistry => "Triggers",
        Screen::Platforms => "Platforms",
        Screen::DeviceCodeFlow(_) => "Connect",
        Screen::StreamApps => "Stream apps",
        Screen::Builtin => "Builtin",
        Screen::BuiltinDetail(_) => "Integration",
        Screen::LiveChat => "Live chat",
        Screen::EventFeed => "Event feed",
        Screen::Globals => "Globals",
        Screen::Settings(_) => "Settings",
        Screen::Tts(_) => "TTS",
        Screen::Soundboard => "Soundboard",
        Screen::ScriptEditor => "Script editor",
        Screen::Server => "Server",
        Screen::Logs => "Logs",
    }
}

pub(crate) fn builtin_active(screen: &Screen, id: &str) -> bool {
    matches!(screen, Screen::BuiltinDetail(s) if s.as_str() == id)
}

pub(crate) fn nav_items_for<'a>(app: &'a App, palette: &'a ForgePalette) -> Sidebar<'a, Message> {
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
            label: "Home",
            active: is_home,
            on_press: Message::Navigate(Screen::Home),
        },
        NavItem::Section("AUDIENCE"),
        NavItem::Leaf {
            icon: Icon::MessageCircle,
            label: "Chat",
            active: is_live_chat,
            on_press: Message::Navigate(Screen::LiveChat),
        },
        NavItem::Section("AUTOMATION"),
        NavItem::Leaf {
            icon: Icon::Bolt,
            label: "Actions",
            active: is_actions,
            on_press: Message::Navigate(Screen::Actions),
        },
        NavItem::Leaf {
            icon: Icon::Bolt,
            label: "Triggers",
            active: is_triggers_registry,
            on_press: Message::Navigate(Screen::TriggersRegistry),
        },
        NavItem::Leaf {
            icon: Icon::Notebook,
            label: "Queues",
            active: is_queues,
            on_press: Message::Navigate(Screen::Queues),
        },
        NavItem::Leaf {
            icon: Icon::Activity,
            label: "Event feed",
            active: is_event_feed,
            on_press: Message::Navigate(Screen::EventFeed),
        },
        NavItem::Leaf {
            icon: Icon::Variable,
            label: "Globals",
            active: is_globals,
            on_press: Message::Navigate(Screen::Globals),
        },
        NavItem::Section("CONNECTIONS"),
        NavItem::MiniLabel("Platforms"),
        NavItem::FlatLink {
            dot_color: palette.brand,
            label: "Twitch",
            active: builtin_active(&app.screen, "twitch"),
            on_press: twitch_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.random,
            label: "YouTube",
            active: builtin_active(&app.screen, "youtube"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("youtube"))),
        },
        NavItem::FlatLink {
            dot_color: palette.info,
            label: "Kick",
            active: builtin_active(&app.screen, "kick"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("kick"))),
        },
        NavItem::FlatLink {
            dot_color: palette.success,
            label: "Trovo",
            active: builtin_active(&app.screen, "trovo"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("trovo"))),
        },
        NavItem::MiniLabel("Stream apps"),
        NavItem::FlatLink {
            dot_color: palette.success,
            label: "OBS Studio",
            active: builtin_active(&app.screen, "obs"),
            on_press: obs_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.warning,
            label: "VTube Studio",
            active: builtin_active(&app.screen, "vtube"),
            on_press: Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("vtube"))),
        },
        NavItem::Leaf {
            icon: Icon::Music,
            label: "Soundboard",
            active: is_soundboard,
            on_press: Message::Navigate(Screen::Soundboard),
        },
        NavItem::Leaf {
            icon: Icon::Volume,
            label: "Text-to-Speech",
            active: is_tts,
            on_press: Message::Navigate(Screen::Tts(TtsSection::Dashboard)),
        },
        NavItem::Leaf {
            icon: Icon::Server,
            label: "WebSocket server",
            active: is_server,
            on_press: Message::Navigate(Screen::Server),
        },
    ];

    let bottom_items = vec![
        NavItem::Divider,
        NavItem::Leaf {
            icon: Icon::Settings,
            label: "Settings",
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
    let editor_id = if let Screen::ActionEditor(id) = &screen {
        Some(*id)
    } else {
        None
    };
    let dcf_platform = if let Screen::DeviceCodeFlow(p) = &screen {
        Some(*p)
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
    } else if let Some(p) = dcf_platform {
        app.ui.device_code_flow = crate::device_code_flow::DeviceCodeFlowState {
            platform: p,
            ..crate::device_code_flow::DeviceCodeFlowState::default()
        };
        Task::none()
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
