#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use forge_app::{
    App, LiveChatMsg, Message, PlatformFilter, RuntimeView, Screen, SidebarExpandState, app::update,
};
use forge_events::EventSource;
use forge_runtime::{
    EventBus, NullEventLogRepo, ScriptRegistry, actions::ActionsService, bus_subscription,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ChatSource, EventId, ModerationMarks, PlatformId, UnifiedChatRow};
use futures_util::StreamExt as _;
use time::OffsetDateTime;

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
    let backend: Arc<dyn forge_storage::DataProvider> = backend;
    App {
        screen: Screen::Home,
        theme,
        palette,
        toast_queue: forge_widgets::ToastQueue::new(),
        storage_offline: false,
        boot_time: std::time::SystemTime::now(),
        sidebar_state: SidebarExpandState::new(),
        rt: RuntimeView {
            actions: Arc::new(ActionsService::new(
                backend.action_repo(),
                backend.queue_repo(),
                backend.history_repo(),
                backend.trigger_instance_repo(),
                backend.soundboard_clips_repo(),
            )),
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            script_registry: Arc::new(ScriptRegistry::new()),
            server_subsystem,
            action_engine: None,
            scheduler: None,
            obs_client: None,
            speak_queue: None,
            sound_player: None,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            twitch_flow: None,
            youtube_flow: None,
            trovo_flow: None,
            kick_flow: None,
            twitch_login: None,
            twitch_token_expires: None,
            twitch_reauth_required: false,
            sub_action_registry: std::sync::Arc::new(forge_registry::SubActionRegistry::new()),
            trigger_registry: std::sync::Arc::new(forge_registry::TriggerRegistry::new()),
        },
        ui: forge_app::UiState::default(),
    }
}

fn make_unified_row(id: &str, author: &str, source: ChatSource) -> UnifiedChatRow {
    UnifiedChatRow {
        id: id.to_owned(),
        event_id: EventId::new(),
        source,
        received_at: OffsetDateTime::now_utc(),
        author: author.to_owned(),
        author_color: None,
        body_segments: vec![forge_types::ChatSegment::Text {
            text: "test".to_owned(),
        }],
        badges: vec![],
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

#[test]
fn row_received_appends_to_log() {
    let mut app = test_app();
    app.ui.live_chat.rows.clear();
    let row = make_unified_row(
        "integration-1",
        "INTEGRATION_TEST_USERNAME",
        ChatSource::Twitch,
    );
    let _ = update(&mut app, Message::LiveChat(LiveChatMsg::RowReceived(row)));
    assert_eq!(app.ui.live_chat.rows.len(), 1);
    assert_eq!(app.ui.live_chat.rows[0].author, "INTEGRATION_TEST_USERNAME");
}

#[test]
fn chat_log_trims_at_cap() {
    let mut app = test_app();
    app.ui.live_chat.rows.clear();
    let limit = forge_app::live_chat::CHAT_LOG_MAX;
    for i in 0..=limit {
        let row = make_unified_row(&format!("id-{i}"), &format!("user-{i}"), ChatSource::Twitch);
        let _ = update(&mut app, Message::LiveChat(LiveChatMsg::RowReceived(row)));
    }
    assert_eq!(app.ui.live_chat.rows.len(), limit);
    assert_ne!(app.ui.live_chat.rows[0].author, "user-0");
}

#[test]
fn event_arrived_does_not_append_to_rows() {
    use forge_events::Event;
    let mut app = test_app();
    app.ui.live_chat.rows.clear();
    let ev = Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({ "username": "x", "message": "y" }),
    );
    let _ = update(&mut app, Message::EventArrived(Arc::new(ev)));
    assert!(app.ui.live_chat.rows.is_empty());
}

#[test]
fn hide_bots_toggled_updates_state() {
    let mut app = test_app();
    let _ = update(&mut app, Message::LiveChat(LiveChatMsg::HideBotsToggled));
    assert!(app.ui.live_chat.hide_bots);
    let _ = update(&mut app, Message::LiveChat(LiveChatMsg::HideBotsToggled));
    assert!(!app.ui.live_chat.hide_bots);
}

#[test]
fn platform_filter_changed_updates_state() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::LiveChat(LiveChatMsg::PlatformFilterChanged(PlatformFilter::Single(
            PlatformId::Twitch,
        ))),
    );
    assert_eq!(
        app.ui.live_chat.platform_filter,
        PlatformFilter::Single(PlatformId::Twitch)
    );
}

#[test]
fn chat_input_changed_updates_state() {
    let mut app = test_app();
    let _ = update(
        &mut app,
        Message::LiveChat(LiveChatMsg::InputChanged("INTEGRATION_TEST_INPUT".into())),
    );
    assert_eq!(app.ui.live_chat.input_buffer, "INTEGRATION_TEST_INPUT");
}

#[test]
fn chat_send_clears_input_buffer() {
    let mut app = test_app();
    app.ui.live_chat.input_buffer = "INTEGRATION_TEST_MESSAGE_BODY".to_owned();
    let _ = update(&mut app, Message::LiveChat(LiveChatMsg::SendPressed));
    assert!(app.ui.live_chat.input_buffer.is_empty());
}

#[tokio::test]
async fn event_bus_subscription_bridge_delivers_events() {
    use forge_events::Event;
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
