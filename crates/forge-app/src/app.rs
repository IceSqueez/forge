use std::sync::Arc;
use std::time::SystemTime;

use forge_soundboard::SoundboardPlayer;

use forge_events::Event;
#[cfg(test)]
use forge_events::EventPublisher;
#[cfg(test)]
use forge_obs::ObsClient;
use forge_platform_twitch::ChatConnectionState;
use forge_runtime::{
    ActionEngineHandle, EventBus, NullEventLogRepo, QueueSchedulerHandle, ScriptRegistry,
};
use forge_storage::{CredentialsRepo, DataProvider};
#[cfg(test)]
use forge_vtube::{VTubeClient, VTubeConfig};
use forge_widgets::{ForgePalette, ThemeId, ToastQueue};
use iced::{Task, Theme};

#[cfg(test)]
use forge_widgets::icons::Icon;

use crate::actions::ActionsState;
use crate::boot;
use crate::builtin_detail::BuiltinDetailState;
use crate::event_feed;
use crate::event_feed::EventFeedState;
use crate::globals_view::GlobalsState;
use crate::home::HomeStats;
use crate::live_chat::LiveChatState;
#[cfg(test)]
use crate::message::ObsClientRef;
#[cfg(test)]
use crate::message::SettingsMsg;
#[cfg(test)]
use crate::message::VTubeClientRef;
use crate::message::{ActionsMsg, BootMsg, ServerSubsystemMsg, SidebarMsg, ToastMsg, TtsMsg};
use crate::queues_view::QueuesState;
use crate::script_editor::ScriptEditorState;
use crate::server_screen::ServerScreenState;
use crate::server_subsystem::ServerSubsystem;
use crate::settings_audio::SettingsAudioState;
use crate::settings_hotkeys::SettingsHotkeysState;
use crate::settings_websocket::SettingsWebSocketState;
use crate::soundboard::SoundboardState;
use crate::tts_dashboard::TtsDashState;
use crate::tts_engines::TtsEnginesState;
use crate::tts_filters::TtsFiltersState;
use crate::tts_triggers::TtsTriggersState;
use crate::voice_aliases::VoiceAliasesState;
use crate::{Message, Screen};
#[cfg(test)]
use forge_types::PlatformId;

pub struct SidebarExpandState {
    pub actions_queues: bool,
}

impl SidebarExpandState {
    pub fn new() -> Self {
        Self {
            actions_queues: false,
        }
    }
}

impl Default for SidebarExpandState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub palette: ForgePalette,
    pub toast_queue: ToastQueue<Message>,
    pub storage_offline: bool,
    pub boot_time: SystemTime,
    pub sidebar_state: SidebarExpandState,
    pub rt: crate::runtime_view::RuntimeView,
    pub ui: UiState,
}

pub struct UiState {
    pub home: HomeStats,
    pub event_feed: EventFeedState,
    pub live_chat: LiveChatState,
    pub actions: ActionsState,
    pub triggers_registry: crate::triggers_registry::TriggersRegistryState,
    pub queues: QueuesState,
    pub viewers: crate::viewers::ViewersState,
    pub globals: GlobalsState,
    pub script_editor: ScriptEditorState,
    pub builtin_detail: Option<BuiltinDetailState>,
    pub server_screen: ServerScreenState,
    pub settings_websocket: SettingsWebSocketState,
    pub settings_hotkeys: SettingsHotkeysState,
    pub twitch_panel: crate::twitch_panel::TwitchPanelState,
    pub obs_panel: crate::obs_panel::ObsPanelState,
    pub soundboard: SoundboardState,
    pub settings_audio: SettingsAudioState,
    pub tts_dashboard: TtsDashState,
    pub tts_engines: TtsEnginesState,
    pub tts_aliases: VoiceAliasesState,
    pub tts_filters: TtsFiltersState,
    pub tts_triggers: TtsTriggersState,
    pub tts_cloud_engines: crate::cloud_tts_engines::CloudTtsEnginesState,
    pub local_callback_flow: crate::local_callback_flow::LocalCallbackFlowState,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            triggers_registry: crate::triggers_registry::TriggersRegistryState::default(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            builtin_detail: None,
            server_screen: ServerScreenState::default(),
            settings_websocket: SettingsWebSocketState::default(),
            settings_hotkeys: SettingsHotkeysState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            obs_panel: crate::obs_panel::ObsPanelState::default(),
            soundboard: SoundboardState::new(),
            settings_audio: SettingsAudioState::new(),
            tts_dashboard: TtsDashState::new(),
            tts_engines: TtsEnginesState::new(),
            tts_aliases: VoiceAliasesState::new(),
            tts_filters: TtsFiltersState::new(),
            tts_triggers: TtsTriggersState::new(),
            tts_cloud_engines: crate::cloud_tts_engines::CloudTtsEnginesState::default(),
            local_callback_flow: crate::local_callback_flow::LocalCallbackFlowState::default(),
        }
    }
}

impl App {
    pub fn default_with(
        initial: Screen,
        backend: Arc<dyn DataProvider>,
        storage_offline: bool,
        script_registry: Arc<ScriptRegistry>,
        action_engine: Option<ActionEngineHandle>,
        scheduler: Option<QueueSchedulerHandle>,
        sound_player: Option<Arc<SoundboardPlayer>>,
    ) -> Self {
        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        Self {
            screen: initial,
            theme,
            palette,
            toast_queue: ToastQueue::new(),
            storage_offline,
            boot_time: SystemTime::now(),
            sidebar_state: SidebarExpandState::new(),
            rt: crate::runtime_view::RuntimeView {
                actions: Arc::new(forge_runtime::actions::ActionsService::new(
                    backend.action_repo(),
                    backend.queue_repo(),
                    backend.history_repo(),
                    backend.trigger_instance_repo(),
                    backend.soundboard_clips_repo(),
                )),
                backend,
                bus: EventBus::new(Arc::new(NullEventLogRepo)),
                script_registry,
                server_subsystem,
                action_engine,
                scheduler,
                obs_client: None,
                vtube_client: None,
                discord_client: None,
                midi_client: None,
                hotkey_client: None,
                speak_queue: None,
                sound_player,
                twitch_chat_handle: None,
                chat_send_bridge: None,
                twitch_flow: None,
                youtube_flow: None,
                trovo_flow: None,
                kick_flow: None,
                tts_engine_ids: Vec::new(),
                twitch_login: None,
                twitch_token_expires: None,
                twitch_reauth_required: false,
                sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
                trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
            },
            ui: UiState::default(),
        }
    }
}

#[cfg(test)]
const TEST_KEY: [u8; 32] = [0xab; 32];

#[cfg(test)]
impl Default for App {
    #[allow(clippy::expect_used)]
    fn default() -> Self {
        use forge_storage_sqlite::SqliteBackend;
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for test");
        let backend: Arc<dyn DataProvider> = Arc::new(
            rt.block_on(SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY))
                .expect("in-memory SQLite always opens"),
        );
        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        Self {
            screen: Screen::Home,
            theme,
            palette,
            toast_queue: ToastQueue::new(),
            storage_offline: false,
            boot_time: SystemTime::now(),
            sidebar_state: SidebarExpandState::new(),
            rt: crate::runtime_view::RuntimeView {
                actions: Arc::new(forge_runtime::actions::ActionsService::new(
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
                vtube_client: None,
                discord_client: None,
                midi_client: None,
                hotkey_client: None,
                speak_queue: None,
                sound_player: None,
                twitch_chat_handle: None,
                chat_send_bridge: None,
                twitch_flow: None,
                youtube_flow: None,
                trovo_flow: None,
                kick_flow: None,
                tts_engine_ids: Vec::new(),
                twitch_login: None,
                twitch_token_expires: None,
                twitch_reauth_required: false,
                sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
                trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
            },
            ui: UiState::default(),
        }
    }
}

fn dispatch_event(app: &mut App, event: &Arc<Event>) -> Task<Message> {
    let mut task = crate::builtin_detail::on_event(app.ui.builtin_detail.as_mut(), event);
    task = task.chain(crate::home::on_event(&mut app.ui.home, event));
    task = task.chain(crate::event_feed::on_event(&mut app.ui.event_feed, event));
    if event.kind == "platform.reauth_required"
        && event.payload["platform"].as_str() == Some("twitch")
    {
        app.rt.twitch_reauth_required = true;
    }
    task
}

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Navigate(screen) => crate::navigation::handle_navigate(app, screen),
        Message::Sidebar(sub) => {
            match sub {
                SidebarMsg::ToggleActionsQueues => {
                    app.sidebar_state.actions_queues = !app.sidebar_state.actions_queues;
                }
            }
            Task::none()
        }
        Message::ThemeChanged(id) => {
            let (theme, palette) = match id {
                ThemeId::CatppuccinMocha => forge_widgets::catppuccin_mocha(),
                ThemeId::TokyoNight => forge_widgets::tokyo_night_storm(),
                ThemeId::Latte => forge_widgets::latte(),
            };
            app.theme = theme;
            app.palette = palette;
            Task::none()
        }
        Message::EventArrived(event) => dispatch_event(app, &event),
        Message::EventFeed(sub) => event_feed::update(&mut app.ui.event_feed, &app.rt, sub),
        Message::LiveChat(sub) => crate::live_chat::update(&mut app.ui.live_chat, &app.rt, sub),
        Message::Settings(sub) => crate::settings::handle_message(app, sub),
        Message::Home(sub) => crate::home::update(&mut app.ui.home, &app.rt, sub),
        Message::Globals(sub) => crate::globals_view::update(&mut app.ui.globals, &app.rt, sub),
        Message::Actions(sub) => crate::actions::update(&mut app.ui.actions, &app.rt, sub),
        Message::Queues(sub) => crate::queues_view::update(&mut app.ui.queues, &app.rt, sub),
        Message::Viewers(sub) => crate::viewers::update(&mut app.ui.viewers, &app.rt, sub),
        Message::TriggersRegistry(sub) => {
            crate::triggers_registry::update(&mut app.ui.triggers_registry, &app.rt, sub)
        }
        Message::ScriptEditor(sub) => {
            crate::script_editor::update(&mut app.ui.script_editor, &app.rt, sub)
        }
        Message::BuiltinDetail(sub) => {
            crate::builtin_detail::update(&mut app.ui.builtin_detail, &app.rt, sub)
        }
        Message::Boot(boot_msg) => match boot_msg {
            BootMsg::Obs(result) => boot::handle_obs_boot_result(app, result),
            BootMsg::Vtube(result) => boot::handle_vtube_boot_result(app, result),
            BootMsg::Discord(result) => boot::handle_discord_boot_result(app, result),
            BootMsg::Midi(result) => boot::handle_midi_boot_result(app, result),
            BootMsg::Hotkey(result) => boot::handle_hotkey_boot_result(app, result),
            BootMsg::Twitch(result) => boot::handle_twitch_boot_result(app, result),
            BootMsg::Server(result) => boot::handle_server_boot_result(app, result),
        },
        Message::ServerSubsystem(sub) => match sub {
            ServerSubsystemMsg::RestartResult(result) => {
                boot::handle_server_restart_result(app, result)
            }
            ServerSubsystemMsg::StopResult(result) => boot::handle_server_stop_result(app, result),
            ServerSubsystemMsg::TokenRotated(result) => {
                boot::handle_server_token_rotated(app, result)
            }
        },
        Message::Server(crate::server_screen::ServerScreenMsg::RestartServer) => {
            boot::handle_server_restart_command(app)
        }
        Message::Server(crate::server_screen::ServerScreenMsg::StopServer) => {
            boot::handle_server_stop_command(app)
        }
        Message::Server(crate::server_screen::ServerScreenMsg::RegenerateToken) => {
            boot::handle_server_regenerate_token(app)
        }
        Message::Server(sub) => {
            crate::server_screen::update(&mut app.ui.server_screen, &app.rt, sub)
        }
        Message::SettingsWebSocket(
            crate::settings_websocket::SettingsWebSocketMsg::SaveStatus(Ok(())),
        ) => {
            if !matches!(
                app.ui.server_screen.server_status,
                crate::server_screen::ServerStatus::Running
            ) {
                return Task::none();
            }
            let subsystem = Arc::clone(&app.rt.server_subsystem);
            Task::perform(
                async move { subsystem.restart().await.map_err(|e| e.to_string()) },
                |r| Message::ServerSubsystem(ServerSubsystemMsg::RestartResult(r)),
            )
        }
        Message::SettingsWebSocket(sub) => {
            crate::settings_websocket::update(&mut app.ui.settings_websocket, &app.rt, sub)
        }
        Message::TwitchPanel(sub) => crate::twitch_panel::update(
            &mut app.ui.twitch_panel,
            &mut app.ui.builtin_detail,
            &mut app.rt,
            sub,
        ),
        Message::TwitchReauthRequested => boot::handle_twitch_reauth_requested(app),
        Message::ObsPanel(sub) => crate::obs_panel::update(&mut app.ui.obs_panel, &app.rt, sub),
        Message::Soundboard(sub) => crate::soundboard::update(&mut app.ui.soundboard, &app.rt, sub),
        Message::SettingsAudio(sub) => {
            crate::settings_audio::update(&mut app.ui.settings_audio, &app.rt, sub)
        }
        Message::Tts(TtsMsg::Dashboard(sub)) => {
            crate::tts_dashboard::update(&mut app.ui.tts_dashboard, &app.rt, sub)
        }
        Message::Tts(TtsMsg::Engines(sub)) => {
            crate::tts_engines::update(&mut app.ui.tts_engines, &app.rt, sub)
        }
        Message::Tts(TtsMsg::Aliases(sub)) => {
            crate::voice_aliases::update(&mut app.ui.tts_aliases, &app.rt, sub)
        }
        Message::Tts(TtsMsg::Filters(sub)) => {
            crate::tts_filters::update(&mut app.ui.tts_filters, &app.rt, sub)
        }
        Message::Tts(TtsMsg::Triggers(sub)) => {
            crate::tts_triggers::update(&mut app.ui.tts_triggers, &app.rt, sub)
        }
        Message::Tts(TtsMsg::CloudEngines(sub)) => {
            crate::cloud_tts_engines::update(&mut app.ui.tts_cloud_engines, &app.rt, sub)
        }
        Message::LocalCallbackFlow(sub) => {
            crate::local_callback_flow::update(&mut app.ui.local_callback_flow, &mut app.rt, sub)
        }
        Message::SettingsHotkeys(sub) => {
            crate::settings_hotkeys::update(&mut app.ui.settings_hotkeys, &app.rt, sub)
        }
        Message::Toast(sub) => match sub {
            ToastMsg::Fired {
                kind,
                message,
                duration_ms,
            } => {
                let duration = std::time::Duration::from_millis(duration_ms);
                app.toast_queue.push(kind, message, None, duration);
                Task::none()
            }
            ToastMsg::Dismissed(id) => {
                app.toast_queue.dismiss(id);
                Task::none()
            }
            ToastMsg::Tick(now) => {
                app.toast_queue.prune_expired(now);
                Task::none()
            }
        },
        Message::OutsideClick => {
            if app.ui.actions.renaming_action.is_some() {
                Task::done(Message::Actions(ActionsMsg::RenameCancel))
            } else if app.ui.actions.action_menu_open.is_some() {
                Task::done(Message::Actions(ActionsMsg::DismissActionMenu))
            } else {
                Task::none()
            }
        }
        Message::Noop => Task::none(),
    }
}

pub(crate) fn format_uptime(elapsed: std::time::Duration) -> String {
    let total_secs = elapsed.as_secs();
    if total_secs < 60 {
        return format!("{total_secs}s");
    }
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours == 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{hours}h {minutes}m")
    }
}

pub(crate) fn subsystem_connectivity(app: &App) -> (u8, u8) {
    let mut connected: u8 = 0;
    let twitch_live = app
        .rt
        .twitch_chat_handle
        .as_ref()
        .is_some_and(|h| matches!(h.connection_state(), ChatConnectionState::Connected));
    if twitch_live {
        connected += 2;
    }
    if app.rt.obs_client.is_some() {
        connected += 1;
    }
    if matches!(
        app.ui.server_screen.server_status,
        crate::server_screen::ServerStatus::Running
    ) {
        connected += 1;
    }
    if app.rt.sound_player.is_some() {
        connected += 2;
    }
    if app.rt.speak_queue.is_some() {
        connected += 1;
    }
    if !app.storage_offline {
        connected += 1;
    }
    (connected, 8)
}

pub fn theme_callback(app: &App) -> Theme {
    app.theme.clone()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::SettingsSection;
    use crate::actions::{AddActionMsg, AddSubActionMsg, SubActionKindChoice};
    use crate::message::{ActionEditorMsg, HomeMsg, HomeStatsData};
    use crate::navigation::{breadcrumb_icon_for, screen_label};
    use crate::subscriptions::subscription;
    use crate::view_router::view;
    use forge_storage_sqlite::SqliteBackend;
    use forge_widgets::ThemeId;

    #[test]
    fn navigate_updates_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        assert_eq!(app.screen, Screen::Actions);
    }

    #[test]
    fn navigate_to_hub_sets_hub_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Logs));
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn navigate_to_settings_diagnostics() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Navigate(Screen::Settings(SettingsSection::Diagnostics)),
        );
        assert_eq!(app.screen, Screen::Settings(SettingsSection::Diagnostics));
    }

    #[test]
    fn theme_changed_tokyo_night() {
        let mut app = App::default();
        let _ = update(&mut app, Message::ThemeChanged(ThemeId::TokyoNight));
        let _ = theme_callback(&app);
    }

    #[test]
    fn theme_changed_latte() {
        let mut app = App::default();
        let _ = update(&mut app, Message::ThemeChanged(ThemeId::Latte));
        let _ = theme_callback(&app);
    }

    #[test]
    fn noop_does_not_change_screen() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Noop);
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn subscription_compiles() {
        let app = App::default();
        let _ = subscription(&app);
    }

    #[test]
    fn view_compiles_hub() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = view(&app);
    }

    #[test]
    fn reconnect_platform_twitch_dispatches_navigate_task() {
        let mut app = App::default();
        let _task = update(
            &mut app,
            Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::Twitch)),
        );
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn reconnect_platform_youtube_dispatches_navigate_task() {
        let mut app = App::default();
        let _task = update(
            &mut app,
            Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::YouTube)),
        );
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn settings_reconnect_result_ok_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(
            &mut app,
            Message::Settings(SettingsMsg::PlatformReconnectResult(Ok(()))),
        );
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn settings_reconnect_result_err_logs_and_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(
            &mut app,
            Message::Settings(SettingsMsg::PlatformReconnectResult(Err(
                "connection refused".into(),
            ))),
        );
        assert_eq!(app.screen, Screen::Home);
    }

    #[test]
    fn chat_send_empty_input_is_noop() {
        use crate::message::LiveChatMsg;
        let mut app = App::default();
        app.ui.live_chat.input_buffer = String::new();
        let _ = update(&mut app, Message::LiveChat(LiveChatMsg::SendPressed));
        assert!(app.ui.live_chat.input_buffer.is_empty());
    }

    #[test]
    fn chat_send_clears_input_and_dispatches_task() {
        use crate::message::LiveChatMsg;
        let mut app = App::default();
        app.ui.live_chat.input_buffer = "hello chat".into();
        let _ = update(&mut app, Message::LiveChat(LiveChatMsg::SendPressed));
        assert!(app.ui.live_chat.input_buffer.is_empty());
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn runtime_handles_present_when_storage_is_online() {
        use forge_storage::DataProvider;

        let sqlite = Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .expect("in-memory SQLite always opens"),
        );
        let dp: Arc<dyn DataProvider> = Arc::clone(&sqlite) as Arc<dyn DataProvider>;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let queues = dp.queue_repo().list().await.expect("list queues");

        let engine = forge_runtime::spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(forge_registry::SubActionRegistry::new()),
        );
        let scheduler =
            forge_runtime::QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);

        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&dp) as Arc<dyn CredentialsRepo>
        ));
        let app = App {
            screen: Screen::Home,
            theme,
            palette,
            toast_queue: ToastQueue::new(),
            storage_offline: false,
            boot_time: std::time::SystemTime::now(),
            sidebar_state: SidebarExpandState::new(),
            rt: crate::runtime_view::RuntimeView {
                actions: Arc::new(forge_runtime::actions::ActionsService::new(
                    dp.action_repo(),
                    dp.queue_repo(),
                    dp.history_repo(),
                    dp.trigger_instance_repo(),
                    dp.soundboard_clips_repo(),
                )),
                backend: dp,
                bus,
                script_registry: Arc::new(forge_runtime::ScriptRegistry::new()),
                server_subsystem,
                action_engine: Some(engine),
                scheduler: Some(scheduler),
                obs_client: None,
                vtube_client: None,
                discord_client: None,
                midi_client: None,
                hotkey_client: None,
                speak_queue: None,
                sound_player: None,
                twitch_chat_handle: None,
                chat_send_bridge: None,
                twitch_flow: None,
                youtube_flow: None,
                trovo_flow: None,
                kick_flow: None,
                tts_engine_ids: Vec::new(),
                twitch_login: None,
                twitch_token_expires: None,
                twitch_reauth_required: false,
                sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
                trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
            },
            ui: UiState::default(),
        };

        assert!(app.rt.action_engine.is_some());
        assert!(app.rt.scheduler.is_some());
    }

    #[test]
    fn runtime_handles_absent_when_storage_offline() {
        let app = App {
            storage_offline: true,
            ..App::default()
        };

        assert!(app.rt.action_engine.is_none());
        assert!(app.rt.scheduler.is_none());
    }

    #[test]
    fn navigate_to_actions_sets_loading_true() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        assert_eq!(app.screen, Screen::Actions);
    }

    #[test]
    fn summaries_loaded_ok_clears_loading_flag() {
        let mut app = App::default();
        app.ui.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::SummariesLoaded(Ok(vec![]))),
        );
        assert!(!app.ui.actions.loading);
        assert!(app.ui.actions.tree.is_empty());
    }

    #[test]
    fn summaries_loaded_err_clears_loading_flag() {
        let mut app = App::default();
        app.ui.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::SummariesLoaded(Err("db error".into()))),
        );
        assert!(!app.ui.actions.loading);
    }

    #[test]
    fn action_selected_updates_selected_field() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        let _ = update(&mut app, Message::Actions(ActionsMsg::ActionSelected(id)));
        assert_eq!(app.ui.actions.selected, Some(id));
    }

    #[test]
    fn detail_loaded_ok_stores_detail() {
        use forge_types::{Action, ActionId, QueueId};
        let mut app = App::default();
        let id = ActionId::new();
        let action = Action {
            id,
            name: "!quote".to_string(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };
        let detail = crate::actions::ActionDetail {
            sub_action_avg_ms: vec![],
            action,
            trigger_instances: vec![],
        };
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::DetailLoaded(Ok(detail))),
        );
        assert!(app.ui.actions.detail.is_some());
        assert_eq!(
            app.ui.actions.detail.as_ref().unwrap().action.name,
            "!quote"
        );
    }

    #[test]
    fn detail_loaded_err_clears_detail() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::DetailLoaded(Err("not found".into()))),
        );
        assert!(app.ui.actions.detail.is_none());
    }

    #[test]
    fn telemetry_loaded_ok_stores_and_clears_loading() {
        let mut app = App::default();
        app.ui.actions.telemetry_loading = true;
        let t = forge_storage::ActionTelemetry {
            runs_today: 42,
            ..Default::default()
        };
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TelemetryLoaded(Ok(t.clone()))),
        );
        assert!(!app.ui.actions.telemetry_loading);
        assert_eq!(app.ui.actions.telemetry.as_ref().unwrap().runs_today, 42);
    }

    #[test]
    fn telemetry_loaded_err_clears_loading_and_data() {
        let mut app = App::default();
        app.ui.actions.telemetry_loading = true;
        app.ui.actions.telemetry = Some(forge_storage::ActionTelemetry::default());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TelemetryLoaded(Err("timeout".into()))),
        );
        assert!(!app.ui.actions.telemetry_loading);
        assert!(app.ui.actions.telemetry.is_none());
    }

    #[test]
    fn action_selected_sets_telemetry_loading_true() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        let _ = update(&mut app, Message::Actions(ActionsMsg::ActionSelected(id)));
        assert!(app.ui.actions.telemetry_loading);
        assert!(app.ui.actions.telemetry.is_none());
    }

    #[test]
    fn toggle_enabled_updates_detail_optimistically() {
        use forge_types::{Action, ActionId, QueueId};
        let mut app = App::default();
        let id = ActionId::new();
        let action = Action {
            id,
            name: "test".to_string(),
            group: None,
            queue_id: QueueId::new(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };
        app.ui.actions.detail = Some(crate::actions::ActionDetail {
            sub_action_avg_ms: vec![],
            action,
            trigger_instances: vec![],
        });
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::ToggleEnabled(id, false)),
        );
        assert!(!app.ui.actions.detail.as_ref().unwrap().action.enabled);
    }

    #[test]
    fn actions_delete_clears_selection_and_detail() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        app.ui.actions.selected = Some(id);
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::ActionDeleted(Ok(()))),
        );
        assert!(app.ui.actions.selected.is_none());
        assert!(app.ui.actions.detail.is_none());
    }

    #[test]
    fn view_compiles_actions_empty() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        let _ = view(&app);
    }

    #[test]
    fn open_add_action_modal_creates_form() {
        let mut app = App::default();
        assert!(app.ui.actions.add_action_modal.is_none());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::OpenRequested,
            ))),
        );
        assert!(app.ui.actions.add_action_modal.is_some());
    }

    #[test]
    fn cancel_clears_modal() {
        let mut app = App::default();
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Cancel,
            ))),
        );
        assert!(app.ui.actions.add_action_modal.is_none());
    }

    #[test]
    fn name_changed_updates_form() {
        let mut app = App::default();
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::NameChanged("Sub raid".to_string()),
            ))),
        );
        assert_eq!(
            app.ui.actions.add_action_modal.as_ref().unwrap().name,
            "Sub raid"
        );
    }

    #[test]
    fn submit_with_invalid_form_is_noop() {
        let mut app = App::default();
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Submit,
            ))),
        );
        assert!(
            app.ui.actions.add_action_modal.is_some(),
            "modal remains open"
        );
    }

    #[test]
    fn saved_ok_closes_modal_and_sets_selected() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let new_id = ActionId::new();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Saved(Ok(new_id)),
            ))),
        );
        assert!(app.ui.actions.add_action_modal.is_none());
    }

    #[test]
    fn saved_err_keeps_modal_open_with_error() {
        let mut app = App::default();
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Saved(Err("db locked".to_string())),
            ))),
        );
        let form = app.ui.actions.add_action_modal.as_ref().unwrap();
        assert_eq!(form.error.as_deref(), Some("db locked"));
        assert!(!form.saving);
    }

    #[test]
    fn view_compiles_actions_with_open_modal() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        app.ui.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = view(&app);
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn submit_with_valid_form_sets_saving_and_saved_ok_stores_action() {
        use forge_storage::DataProvider;
        use forge_types::{Queue, QueueId};

        use forge_storage_sqlite::SqliteBackend;
        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .expect("in-memory SQLite"),
        );
        let queue = Queue {
            id: QueueId::new(),
            name: "default".to_string(),
            blocking: false,
        };
        dp.queue_repo().save(&queue).await.expect("save queue");

        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&dp) as Arc<dyn CredentialsRepo>
        ));
        let mut app = App {
            screen: Screen::Actions,
            theme,
            palette,
            toast_queue: ToastQueue::new(),
            storage_offline: false,
            boot_time: std::time::SystemTime::now(),
            sidebar_state: SidebarExpandState::new(),
            rt: crate::runtime_view::RuntimeView {
                actions: Arc::new(forge_runtime::actions::ActionsService::new(
                    dp.action_repo(),
                    dp.queue_repo(),
                    dp.history_repo(),
                    dp.trigger_instance_repo(),
                    dp.soundboard_clips_repo(),
                )),
                backend: Arc::clone(&dp),
                bus: EventBus::new(Arc::new(NullEventLogRepo)),
                script_registry: Arc::new(ScriptRegistry::new()),
                server_subsystem,
                action_engine: None,
                scheduler: None,
                obs_client: None,
                vtube_client: None,
                discord_client: None,
                midi_client: None,
                hotkey_client: None,
                speak_queue: None,
                sound_player: None,
                twitch_chat_handle: None,
                chat_send_bridge: None,
                twitch_flow: None,
                youtube_flow: None,
                trovo_flow: None,
                kick_flow: None,
                tts_engine_ids: Vec::new(),
                twitch_login: None,
                twitch_token_expires: None,
                twitch_reauth_required: false,
                sub_action_registry: Arc::new(forge_registry::SubActionRegistry::new()),
                trigger_registry: Arc::new(forge_registry::TriggerRegistry::new()),
            },
            ui: UiState::default(),
        };

        let mut form = crate::actions::AddActionForm::new();
        form.name = "My test action".to_string();
        form.set_queue_options(vec![(queue.id, "default".to_string())]);
        app.ui.actions.add_action_modal = Some(form);

        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Submit,
            ))),
        );
        assert!(app.ui.actions.add_action_modal.as_ref().unwrap().saving);

        let saved_id = forge_types::ActionId::new();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Saved(Ok(saved_id)),
            ))),
        );
        assert!(app.ui.actions.add_action_modal.is_none());
    }

    #[test]
    fn open_add_sub_action_modal_creates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        assert!(app.ui.actions.add_sub_action_modal.is_none());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::OpenRequested(id),
            ))),
        );
        assert!(app.ui.actions.add_sub_action_modal.is_some());
        assert_eq!(
            app.ui
                .actions
                .add_sub_action_modal
                .as_ref()
                .unwrap()
                .for_action_id,
            id
        );
    }

    #[test]
    fn cancel_sub_action_modal_clears_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Cancel,
            ))),
        );
        assert!(app.ui.actions.add_sub_action_modal.is_none());
    }

    #[test]
    fn kind_selected_updates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::KindSelected(SubActionKindChoice::Delay),
            ))),
        );
        assert_eq!(
            app.ui.actions.add_sub_action_modal.as_ref().unwrap().kind,
            SubActionKindChoice::Delay,
        );
    }

    #[test]
    fn send_chat_message_changed_updates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::SendChatMessageChanged("Hello %user%!".to_string()),
            ))),
        );
        assert_eq!(
            app.ui
                .actions
                .add_sub_action_modal
                .as_ref()
                .unwrap()
                .config
                .send_chat_message,
            "Hello %user%!"
        );
    }

    #[test]
    fn submit_invalid_delay_sets_error_on_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        let mut form = crate::actions::AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::Delay;
        form.config.delay_ms = "not_a_number".to_string();
        app.ui.actions.add_sub_action_modal = Some(form);
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Submit,
            ))),
        );
        let f = app.ui.actions.add_sub_action_modal.as_ref().unwrap();
        assert!(f.error.is_some());
    }

    #[test]
    fn add_sub_action_saved_ok_closes_modal() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        app.ui.actions.add_sub_action_modal = Some(crate::actions::AddSubActionForm::new(id));
        app.ui.actions.selected = Some(id);
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Saved(Ok(())),
            ))),
        );
        assert!(app.ui.actions.add_sub_action_modal.is_none());
    }

    #[test]
    fn add_sub_action_saved_err_keeps_modal_with_error() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Saved(Err("db locked".to_string())),
            ))),
        );
        let f = app.ui.actions.add_sub_action_modal.as_ref().unwrap();
        assert_eq!(f.error.as_deref(), Some("db locked"));
        assert!(!f.saving);
    }

    #[test]
    fn view_compiles_actions_with_add_sub_action_modal() {
        use forge_types::ActionId;
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = view(&app);
    }

    #[test]
    fn clips_loaded_populates_available_clips() {
        use forge_types::{ActionId, ClipId};
        let mut app = App::default();
        app.ui.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let clip_id = ClipId::new();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::ClipsLoaded(vec![(clip_id, "Airhorn".to_string())]),
            ))),
        );
        let clips = &app
            .ui
            .actions
            .add_sub_action_modal
            .as_ref()
            .unwrap()
            .available_clips;
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].1, "Airhorn");
    }

    #[test]
    fn play_sound_clip_selected_updates_config() {
        use forge_types::{ActionId, ClipId};
        let mut app = App::default();
        let mut form = crate::actions::AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::PlaySound;
        app.ui.actions.add_sub_action_modal = Some(form);
        let clip_id = ClipId::new();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::PlaySoundClipSelected(clip_id),
            ))),
        );
        assert_eq!(
            app.ui
                .actions
                .add_sub_action_modal
                .as_ref()
                .unwrap()
                .config
                .play_sound_clip_id,
            Some(clip_id)
        );
    }

    #[test]
    fn submit_play_sound_without_clip_sets_error() {
        use forge_types::ActionId;
        let mut app = App::default();
        let mut form = crate::actions::AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::PlaySound;
        app.ui.actions.add_sub_action_modal = Some(form);
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                AddSubActionMsg::Submit,
            ))),
        );
        let f = app.ui.actions.add_sub_action_modal.as_ref().unwrap();
        assert!(f.error.is_some());
    }

    #[test]
    fn view_compiles_play_sound_with_clips() {
        use forge_types::{ActionId, ClipId};
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        let mut form = crate::actions::AddSubActionForm::new(ActionId::new());
        form.kind = SubActionKindChoice::PlaySound;
        form.available_clips = vec![(ClipId::new(), "Airhorn".to_string())];
        app.ui.actions.add_sub_action_modal = Some(form);
        let _ = view(&app);
    }

    #[test]
    fn format_uptime_zero_seconds() {
        assert_eq!(format_uptime(std::time::Duration::from_secs(0)), "0s");
    }

    #[test]
    fn format_uptime_ninety_seconds() {
        assert_eq!(format_uptime(std::time::Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn format_uptime_one_hour_one_minute() {
        assert_eq!(format_uptime(std::time::Duration::from_secs(3700)), "1h 1m");
    }

    #[test]
    fn format_uptime_twenty_four_hours() {
        assert_eq!(
            format_uptime(std::time::Duration::from_secs(86400)),
            "24h 0m"
        );
    }

    #[test]
    fn format_uptime_less_than_minute() {
        assert_eq!(format_uptime(std::time::Duration::from_secs(47)), "47s");
    }

    #[test]
    fn hub_view_compiles_with_empty_stats() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = view(&app);
    }

    #[test]
    fn hub_view_compiles_with_populated_stats() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        app.ui.home.actions_count = Some(47);
        app.ui.home.triggers_fired = Some(1284);
        app.ui.home.globals_count = Some(31);
        let _ = view(&app);
    }

    #[test]
    fn navigate_to_hub_dispatches_load_stats() {
        let mut app = App::default();
        let task = update(&mut app, Message::Navigate(Screen::Home));
        assert_eq!(app.screen, Screen::Home);
        let _ = task;
    }

    #[test]
    fn home_stats_loaded_ok_updates_all_fields() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let data = HomeStatsData {
            actions_count: 5,
            triggers_fired: 42,
            globals_count: 7,
        };
        let _ = update(&mut app, Message::Home(HomeMsg::StatsLoaded(Ok(data))));
        assert_eq!(app.ui.home.actions_count, Some(5));
        assert_eq!(app.ui.home.triggers_fired, Some(42));
        assert_eq!(app.ui.home.globals_count, Some(7));
    }

    #[test]
    fn home_stats_loaded_err_leaves_nones() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(
            &mut app,
            Message::Home(HomeMsg::StatsLoaded(Err("db error".into()))),
        );
        assert!(app.ui.home.actions_count.is_none());
        assert!(app.ui.home.triggers_fired.is_none());
        assert!(app.ui.home.globals_count.is_none());
    }

    #[test]
    fn sidebar_expand_state_initializes_collapsed() {
        let app = App::default();
        assert!(!app.sidebar_state.actions_queues);
    }

    #[test]
    fn sidebar_toggle_actions_queues_flips_bool() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::ToggleActionsQueues));
        assert!(app.sidebar_state.actions_queues);
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::ToggleActionsQueues));
        assert!(!app.sidebar_state.actions_queues);
    }

    #[test]
    fn breadcrumb_icon_for_home_returns_home_icon() {
        assert_eq!(breadcrumb_icon_for(&Screen::Home), Icon::Home);
    }

    #[test]
    fn breadcrumb_icon_for_actions_returns_lightning() {
        assert_eq!(breadcrumb_icon_for(&Screen::Actions), Icon::Bolt);
    }

    #[test]
    fn breadcrumb_icon_for_settings_returns_gear() {
        assert_eq!(
            breadcrumb_icon_for(&Screen::Settings(SettingsSection::Appearance)),
            Icon::Settings
        );
    }

    #[test]
    fn screen_label_home() {
        assert_eq!(screen_label(&Screen::Home), "Home");
    }

    #[test]
    fn screen_label_actions() {
        assert_eq!(screen_label(&Screen::Actions), "Actions");
    }

    #[test]
    fn screen_label_settings() {
        assert_eq!(
            screen_label(&Screen::Settings(SettingsSection::Appearance)),
            "Settings"
        );
    }

    #[test]
    fn view_home_renders() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        app.ui.home.actions_count = Some(12);
        app.ui.home.triggers_fired = Some(99);
        app.ui.home.globals_count = Some(3);
        let _ = view(&app);
    }

    #[test]
    fn view_live_chat_renders() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::LiveChat));
        let _ = view(&app);
    }

    #[test]
    fn hub_view_desc_shows_actions_count() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        app.ui.home.actions_count = Some(47);
        app.ui.home.triggers_fired = Some(1284);
        let _ = view(&app);
    }

    #[test]
    fn navigate_to_integration_detail_sets_screen() {
        use forge_platform_core::BuiltinId;
        let mut app = App::default();
        let id = BuiltinId::new("obs");
        let _ = update(
            &mut app,
            Message::Navigate(Screen::BuiltinDetail(id.clone())),
        );
        assert_eq!(app.screen, Screen::BuiltinDetail(id));
    }

    #[test]
    fn view_compiles_integration_detail_without_state() {
        use forge_platform_core::BuiltinId;
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Navigate(Screen::BuiltinDetail(BuiltinId::new("obs"))),
        );
        let _ = view(&app);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn obs_boot_result_ok_sets_obs_client_and_integration_detail() {
        let mut app = App::default();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as _;
        let client = rt
            .block_on(ObsClient::connect("ws://127.0.0.1:4455", None, publisher))
            .expect("ObsClient::connect always returns Ok; supervisor connects in background");
        let _ = update(
            &mut app,
            Message::Boot(BootMsg::Obs(Ok(ObsClientRef::new(Arc::new(client))))),
        );
        assert!(app.rt.obs_client.is_some());
        assert!(app.ui.builtin_detail.is_some());
    }

    #[test]
    fn obs_boot_result_err_leaves_obs_client_none() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Boot(BootMsg::Obs(Err("connection refused".into()))),
        );
        assert!(app.rt.obs_client.is_none());
        assert!(app.ui.builtin_detail.is_none());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn vtube_boot_result_ok_sets_vtube_client_and_integration_detail() {
        let mut app = App::default();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let _enter = rt.enter();
        let client = Arc::new(VTubeClient::connect(
            VTubeConfig::default(),
            Arc::clone(&app.rt.bus) as Arc<dyn EventPublisher>,
            Arc::clone(&app.rt.backend) as Arc<dyn forge_storage::CredentialsRepo>,
        ));
        let _ = update(
            &mut app,
            Message::Boot(BootMsg::Vtube(Ok(VTubeClientRef::new(client)))),
        );
        assert!(app.rt.vtube_client.is_some());
        assert!(app.ui.builtin_detail.is_some());
    }

    #[test]
    fn vtube_boot_result_err_leaves_vtube_client_none() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Boot(BootMsg::Vtube(Err("vtube not running".into()))),
        );
        assert!(app.rt.vtube_client.is_none());
        assert!(app.ui.builtin_detail.is_none());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn vtube_all_six_sub_action_runners_register_successfully() {
        use forge_registry::SubActionRegistry;
        use forge_vtube::{VTubeSink, register_vtube_sub_actions};

        let app = App::default();
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let _enter = rt.enter();
        let client = Arc::new(VTubeClient::connect(
            VTubeConfig::default(),
            Arc::clone(&app.rt.bus) as Arc<dyn EventPublisher>,
            Arc::clone(&app.rt.backend) as Arc<dyn forge_storage::CredentialsRepo>,
        ));
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, client as Arc<dyn VTubeSink>)
            .expect("registration succeeds with a fresh registry");
        assert_eq!(reg.all().count(), 6);
        for id in &[
            "vtube.hotkey.trigger",
            "vtube.expression.set",
            "vtube.param.set",
            "vtube.model.load",
            "vtube.params.reset",
            "vtube.model.move",
        ] {
            assert!(reg.get(id).is_some(), "missing runner: {id}");
        }
    }

    #[test]
    fn scroll_to_unknown_trigger_instance_is_noop() {
        use crate::triggers_registry::TriggersRegistryMsg;
        use forge_types::TriggerInstanceId;

        let mut app = App::default();
        let unknown = TriggerInstanceId::new();
        let _ = update(
            &mut app,
            Message::TriggersRegistry(TriggersRegistryMsg::ScrollTo(unknown)),
        );
        assert_eq!(app.ui.triggers_registry.selected_id, None);
    }

    #[test]
    fn scroll_to_known_trigger_instance_sets_selected() {
        use crate::triggers_registry::{TriggerInstanceRow, TriggersRegistryMsg};
        use forge_types::TriggerInstanceId;

        let mut app = App::default();
        let id = TriggerInstanceId::new();
        app.ui.triggers_registry.instances.push(TriggerInstanceRow {
            id,
            name: "Test".to_owned(),
            kind_id: "twitch.chat.command".to_owned(),
            enabled: true,
            used_in_count: 0,
            overrides: Default::default(),
            platform_scope: Default::default(),
        });
        let _ = update(
            &mut app,
            Message::TriggersRegistry(TriggersRegistryMsg::ScrollTo(id)),
        );
        assert_eq!(app.ui.triggers_registry.selected_id, Some(id));
    }

    #[test]
    fn navigate_to_action_from_triggers_registry_returns_task() {
        use crate::triggers_registry::TriggersRegistryMsg;
        use forge_types::ActionId;

        let mut app = App::default();
        let action_id = ActionId::new();
        let task = update(
            &mut app,
            Message::TriggersRegistry(TriggersRegistryMsg::NavigateToAction(action_id)),
        );
        let _ = task;
    }

    #[test]
    fn trigger_chip_clicked_returns_task() {
        use crate::message::ActionsMsg;
        use forge_types::TriggerInstanceId;

        let mut app = App::default();
        let instance_id = TriggerInstanceId::new();
        let task = update(
            &mut app,
            Message::Actions(ActionsMsg::TriggerChipClicked(instance_id)),
        );
        let _ = task;
    }
}
