#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use forge_app::{
    App, EventFeedState, Message, PlatformFilter, RuntimeView, Screen, ScriptEditorState,
    SidebarExpandState, app::update,
};
use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, NullEventLogRepo, ScriptRegistry, bus_subscription};
use forge_storage_sqlite::SqliteBackend;
use futures_util::StreamExt as _;

const TEST_KEY: [u8; 32] = [0xab; 32];

fn test_app() -> App {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let backend = Arc::new(
        rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
            .expect("in-memory SQLite always opens"),
    );
    let (theme, palette) = forge_widgets::catppuccin_mocha();
    let server_subsystem = Arc::new(forge_app::server_subsystem::ServerSubsystem::new(
        Arc::clone(&backend) as Arc<dyn forge_storage::CredentialsRepo>,
    ));
    App {
        screen: Screen::Home,
        theme,
        palette,
        toast_queue: forge_widgets::ToastQueue::new(),
        storage_offline: false,
        boot_time: std::time::SystemTime::now(),
        sidebar_state: SidebarExpandState::new(),
        rt: RuntimeView {
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            command_parser: None,
            obs_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
        },
        home: forge_app::app::HomeStats::new(),
        event_feed: EventFeedState::new(),
        live_chat: forge_app::LiveChatState::new(),
        actions: forge_app::ActionsState::new(),
        commands: forge_app::commands_view::CommandsState::new(),
        queues: forge_app::queues_view::QueuesState::new(),
        viewers: forge_app::viewers::ViewersState::default(),
        globals: forge_app::GlobalsState::new(),
        script_editor: ScriptEditorState::new(),
        integration_detail: None,
        server_screen: forge_app::ServerScreenState::default(),
        settings_websocket: forge_app::SettingsWebSocketState::default(),
        twitch_panel: forge_app::twitch_panel::TwitchPanelState::default(),
        obs_panel: forge_app::obs_panel::ObsPanelState::default(),
        soundboard: forge_app::soundboard::SoundboardState::new(),
        settings_audio: forge_app::settings_audio::SettingsAudioState::new(),
        tts_dashboard: forge_app::tts_dashboard::TtsDashState::new(),
        tts_engines: forge_app::tts_engines::TtsEnginesState::new(),
        tts_aliases: forge_app::voice_aliases::VoiceAliasesState::new(),
        tts_filters: forge_app::tts_filters::TtsFiltersState::new(),
        tts_triggers: forge_app::tts_triggers::TtsTriggersState::new(),
    }
}

fn make_twitch_chat_event(username: &str, message: &str) -> Event {
    Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({
            "chatter_user_name": username,
            "message": { "text": message },
            "badges": [],
            "color": "#cba6f7",
        }),
    )
}

#[test]
fn chat_message_event_appends_to_log() {
    let mut app = test_app();
    app.live_chat.chat_log.clear();
    let ev = make_twitch_chat_event("INTEGRATION_TEST_USERNAME", "INTEGRATION_TEST_MESSAGE_BODY");
    let _ = update(&mut app, Message::EventArrived(Arc::new(ev)));
    assert_eq!(app.live_chat.chat_log.len(), 1);
    let row = &app.live_chat.chat_log[0];
    assert_eq!(row.username, "INTEGRATION_TEST_USERNAME");
    assert_eq!(
        row.body,
        forge_widgets::ChatBody::Message("INTEGRATION_TEST_MESSAGE_BODY".to_owned()),
    );
}

#[test]
fn chat_log_trims_at_1000_entries() {
    let mut app = test_app();
    app.live_chat.chat_log.clear();
    let limit = forge_app::live_chat::CHAT_LOG_MAX;
    for i in 0..=limit {
        let ev = make_twitch_chat_event(
            &format!("INTEGRATION_TEST_USERNAME_{i}"),
            "INTEGRATION_TEST_MESSAGE_BODY",
        );
        let _ = update(&mut app, Message::EventArrived(Arc::new(ev)));
    }
    assert_eq!(app.live_chat.chat_log.len(), limit);
    let first = &app.live_chat.chat_log[0];
    assert_ne!(first.username, "INTEGRATION_TEST_USERNAME_0");
}

#[test]
fn non_chat_events_are_ignored() {
    let mut app = test_app();
    app.live_chat.chat_log.clear();
    let ev = Event::new(
        EventSource::Twitch,
        "platform.connected",
        serde_json::json!({ "platform": "twitch" }),
    );
    let _ = update(&mut app, Message::EventArrived(Arc::new(ev)));
    assert!(app.live_chat.chat_log.is_empty());
}

#[test]
fn filter_changed_updates_filter_state() {
    let mut app = test_app();
    let _ = update(&mut app, Message::ChatToggleHideBots);
    assert!(app.live_chat.chat_filter.hide_bots);
    let _ = update(&mut app, Message::ChatToggleHideBots);
    assert!(!app.live_chat.chat_filter.hide_bots);
}

#[test]
fn platform_filter_changed_updates_state() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::ChatPlatformFilter(PlatformFilter::Twitch),
    );
    assert_eq!(app.live_chat.chat_filter.platform, PlatformFilter::Twitch);
}

#[test]
fn chat_input_changed_updates_state() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::ChatInputChanged("INTEGRATION_TEST_INPUT".into()),
    );
    assert_eq!(app.live_chat.chat_input, "INTEGRATION_TEST_INPUT");
}

#[test]
fn chat_submit_clears_input_optimistically() {
    let mut app = test_app();
    app.live_chat.chat_input = "INTEGRATION_TEST_MESSAGE_BODY".to_owned();
    let _ = update(&mut app, Message::ChatSubmit);
    assert!(app.live_chat.chat_input.is_empty());
}

#[tokio::test]
async fn event_bus_subscription_bridge_delivers_events() {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut stream = bus_subscription(Arc::clone(&bus));

    let ev1 = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
    let ev2 = Event::new(EventSource::Twitch, "chat.message", serde_json::Value::Null);
    let ev3 = Event::new(EventSource::Core, "action.done", serde_json::Value::Null);

    let id1 = ev1.id;
    let id2 = ev2.id;
    let id3 = ev3.id;

    bus.publish(ev1);
    bus.publish(ev2);
    bus.publish(ev3);

    let received1 = stream.next().await.unwrap();
    let received2 = stream.next().await.unwrap();
    let received3 = stream.next().await.unwrap();

    assert_eq!(received1.id, id1);
    assert_eq!(received2.id, id2);
    assert_eq!(received3.id, id3);
}
