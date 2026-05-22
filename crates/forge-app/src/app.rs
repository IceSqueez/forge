use std::sync::Arc;
use std::time::SystemTime;

use forge_soundboard::SoundboardPlayer;

use forge_events::{Event, EventPublisher, EventSource};
use forge_obs::ObsClient;
use forge_platform_core::{
    IntegrationContent, IntegrationHealth, IntegrationId, IntegrationStatus, QuickActions,
    SectionIcon,
};
use forge_platform_twitch::{ChatConnectionState, TwitchIntegrationBundle};
use forge_runtime::{
    ActionEngineHandle, CommandParserHandle, EventBus, NullEventLogRepo, QueueSchedulerHandle,
    ScriptRegistry,
};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId};
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_LG, FONT_MD, FONT_SM, FONT_XS};
use forge_widgets::{
    BreadcrumbCrumb, FontRole, ForgePalette, NavItem, Radius, Sidebar, ThemeId, ToastQueue,
    app_footer, breadcrumb, font, page_shell, radius, sidebar, title_bar, toast_viewport,
};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::action_editor::action_editor_view;
use crate::actions::{
    ActionsFilter, ActionsState, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    AddTriggerForm, AddTriggerMsg, RemoveSubActionMsg, SubActionKindChoice, TriggerCategory,
    duplicate_sub_action, kind_label, kind_summary, load_action_detail, load_actions_tree,
    load_clip_options, load_telemetry, move_sub_action, remove_sub_action, save_sub_action,
};
use crate::event_feed::{EventFeedState, event_feed_view, handle_event_feed_msg};
use crate::globals_view::{
    GlobalsState, globals_view, handle_globals_msg, handle_variant_editor_msg,
};
use crate::integration_detail::{
    IntegrationDetailState, handle_integration_detail_msg, health_subscription,
    view as integration_detail_view,
};
use crate::live_chat::{CHAT_LOG_MAX, LiveChatState, chat_row_from_event, live_chat_view};
use crate::message::{
    ActionsMsg, GlobalsMsg, HomeMsg, HomeStatsData, MoveSubActionMsg, ObsClientRef, PlatformId,
    QueuesMsg, SettingsMsg, SidebarMsg, ToastMsg,
};
use crate::queues_view::{QueuesState, load_queues, queues_view};
use crate::script_editor::{
    ScriptEditorMsg, ScriptEditorState, handle_script_editor_msg, script_editor_view,
};
use crate::server_screen::{
    ServerScreenMsg, ServerScreenState, handle_server_screen_msg, server_screen_view,
};
use crate::server_subsystem::ServerSubsystem;
use crate::settings_audio::{SettingsAudioState, handle_settings_audio_msg, settings_audio_view};
use crate::settings_websocket::{
    SettingsWebSocketState, handle_settings_websocket_msg, settings_websocket_view,
};
use crate::soundboard::{SoundboardState, handle_soundboard_msg, soundboard_view};
use crate::stream_apps::view as stream_apps_view;
use crate::test_trigger::synthesize_test_event;
use crate::tts_dashboard::{TtsDashState, handle_tts_dash_msg, tts_dashboard_view};
use crate::tts_engines::{TtsEnginesState, handle_tts_engines_msg, tts_engines_view};
use crate::tts_filters::{TtsFiltersState, handle_tts_filters_msg, tts_filters_view};
use crate::tts_triggers::{TtsTriggersState, handle_tts_triggers_msg, tts_triggers_view};
use crate::voice_aliases::{VoiceAliasesState, handle_voice_aliases_msg, voice_aliases_view};
use crate::{Message, Screen, SettingsSection, TtsSection};

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

#[derive(Default)]
pub struct HomeStats {
    pub actions_count: Option<usize>,
    pub commands_count: Option<usize>,
    pub triggers_fired: Option<u64>,
    pub globals_count: Option<usize>,
}

impl HomeStats {
    pub fn new() -> Self {
        Self::default()
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
    pub home: HomeStats,
    pub event_feed: EventFeedState,
    pub live_chat: LiveChatState,
    pub actions: ActionsState,
    pub commands: crate::commands_view::CommandsState,
    pub queues: QueuesState,
    pub viewers: crate::viewers::ViewersState,
    pub globals: GlobalsState,
    pub script_editor: ScriptEditorState,
    pub integration_detail: Option<IntegrationDetailState>,
    pub server_screen: ServerScreenState,
    pub settings_websocket: SettingsWebSocketState,
    pub twitch_panel: crate::twitch_panel::TwitchPanelState,
    pub obs_panel: crate::obs_panel::ObsPanelState,
    pub soundboard: SoundboardState,
    pub settings_audio: SettingsAudioState,
    pub tts_dashboard: TtsDashState,
    pub tts_engines: TtsEnginesState,
    pub tts_aliases: VoiceAliasesState,
    pub tts_filters: TtsFiltersState,
    pub tts_triggers: TtsTriggersState,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn default_with(
        initial: Screen,
        backend: Arc<SqliteBackend>,
        storage_offline: bool,
        script_registry: Arc<ScriptRegistry>,
        action_engine: Option<ActionEngineHandle>,
        scheduler: Option<QueueSchedulerHandle>,
        command_parser: Option<CommandParserHandle>,
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
                backend,
                bus: EventBus::new(Arc::new(NullEventLogRepo)),
                script_registry,
                server_subsystem,
                action_engine,
                scheduler,
                command_parser,
                obs_client: None,
                speak_queue: None,
                sound_player,
                twitch_chat_handle: None,
                chat_send_bridge: None,
                twitch_flow: None,
                twitch_login: None,
                twitch_token_expires: None,
                twitch_reauth_required: false,
            },
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            commands: crate::commands_view::CommandsState::new(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            integration_detail: None,
            server_screen: ServerScreenState::default(),
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            obs_panel: crate::obs_panel::ObsPanelState::default(),
            soundboard: SoundboardState::new(),
            settings_audio: SettingsAudioState::new(),
            tts_dashboard: TtsDashState::new(),
            tts_engines: TtsEnginesState::new(),
            tts_aliases: VoiceAliasesState::new(),
            tts_filters: TtsFiltersState::new(),
            tts_triggers: TtsTriggersState::new(),
        }
    }
}

#[cfg(test)]
const TEST_KEY: [u8; 32] = [0xab; 32];

#[cfg(test)]
impl Default for App {
    #[allow(clippy::expect_used)]
    fn default() -> Self {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for test");
        let backend = Arc::new(
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
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            commands: crate::commands_view::CommandsState::new(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            integration_detail: None,
            server_screen: ServerScreenState::default(),
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            obs_panel: crate::obs_panel::ObsPanelState::default(),
            soundboard: SoundboardState::new(),
            settings_audio: SettingsAudioState::new(),
            tts_dashboard: TtsDashState::new(),
            tts_engines: TtsEnginesState::new(),
            tts_aliases: VoiceAliasesState::new(),
            tts_filters: TtsFiltersState::new(),
            tts_triggers: TtsTriggersState::new(),
        }
    }
}

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Navigate(screen) => {
            let is_actions = matches!(screen, Screen::Actions);
            let is_queues = matches!(screen, Screen::Queues);
            let is_commands = matches!(screen, Screen::Commands);
            let is_live_chat = matches!(screen, Screen::LiveChat);
            let is_hub = matches!(screen, Screen::Home);
            let is_globals = matches!(screen, Screen::Globals);
            let is_script_editor = matches!(screen, Screen::ScriptEditor);
            let is_soundboard = matches!(screen, Screen::Soundboard);
            let is_settings_audio = matches!(
                screen,
                Screen::Settings(crate::screen::SettingsSection::Audio)
            );
            let is_settings_ws = matches!(
                screen,
                Screen::Settings(crate::screen::SettingsSection::WebSocket)
            );
            let editor_id = if let Screen::ActionEditor(id) = &screen {
                Some(*id)
            } else {
                None
            };
            app.screen = screen;
            if is_actions {
                Task::done(Message::Actions(ActionsMsg::LoadRequested))
            } else if is_queues {
                Task::done(Message::Queues(QueuesMsg::LoadRequested))
            } else if is_commands {
                Task::done(Message::Commands(
                    crate::commands_view::CommandsMsg::LoadRequested,
                ))
            } else if is_live_chat {
                Task::done(Message::Viewers(crate::viewers::ViewersMsg::LoadRequested))
            } else if is_hub {
                Task::done(Message::Home(HomeMsg::LoadStats))
            } else if is_globals {
                Task::done(Message::Globals(GlobalsMsg::LoadRequested))
            } else if is_script_editor {
                Task::done(Message::ScriptEditor(ScriptEditorMsg::LoadRequested))
            } else if is_soundboard {
                Task::done(Message::Soundboard(
                    crate::message::SoundboardMsg::LoadRequested,
                ))
            } else if is_settings_audio {
                Task::done(Message::SettingsAudio(
                    crate::message::SettingsAudioMsg::LoadRequested,
                ))
            } else if is_settings_ws {
                Task::done(Message::SettingsWebSocket(
                    crate::settings_websocket::SettingsWebSocketMsg::LoadRequested,
                ))
            } else if let Some(id) = editor_id {
                let needs_load = app
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
        Message::EventArrived(event) => {
            let mut auto_scroll_task: Option<Task<Message>> = None;
            if let Some(row) = chat_row_from_event(&event) {
                app.live_chat.chat_log.push_back(row);
                if app.live_chat.chat_log.len() > CHAT_LOG_MAX {
                    app.live_chat.chat_log.pop_front();
                }
                if app.live_chat.auto_scroll {
                    auto_scroll_task = Some(iced::widget::operation::snap_to_end(
                        crate::live_chat::chat_scroll_id(),
                    ));
                } else {
                    app.live_chat.unread_count = app.live_chat.unread_count.saturating_add(1);
                }
            }
            if event.kind == "quick_action.done"
                && let Some(state) = app.integration_detail.as_mut()
            {
                let label = event.payload["label"].as_str().unwrap_or("Quick Action");
                let outcome = event.payload["outcome"].as_str().unwrap_or("done");
                state.quick_action_toast = Some(if outcome == "success" {
                    format!("{label} — done")
                } else {
                    format!("{label} — {outcome}")
                });
            }
            if event.kind == "platform.reauth_required"
                && event.payload["platform"].as_str() == Some("twitch")
            {
                app.rt.twitch_reauth_required = true;
            }
            if event.kind == "action.done" {
                app.home.triggers_fired = Some(app.home.triggers_fired.unwrap_or(0) + 1);
            }
            if !app.event_feed.paused {
                app.event_feed.push_event(Arc::unwrap_or_clone(event));
            }
            auto_scroll_task.unwrap_or_else(Task::none)
        }
        Message::EventFeed(sub) => {
            handle_event_feed_msg(&mut app.event_feed, sub, Arc::clone(&app.rt.bus))
        }
        Message::ChatInputChanged(s) => {
            app.live_chat.chat_input = s;
            Task::none()
        }
        Message::ChatSubmit => {
            let msg = std::mem::take(&mut app.live_chat.chat_input);
            let msg = msg.trim().to_owned();
            if msg.is_empty() {
                return Task::none();
            }
            let backend = Arc::clone(&app.rt.backend);
            let bus = Arc::clone(&app.rt.bus);
            Task::perform(
                async move {
                    let json_str = backend
                        .load(&CredentialId::new("twitch:broadcaster"))
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "no Twitch credentials stored".to_owned())?;
                    let bundle: serde_json::Value =
                        serde_json::from_str(&json_str).map_err(|e| e.to_string())?;
                    let token = bundle["access_token"]
                        .as_str()
                        .ok_or_else(|| "missing access_token".to_owned())?
                        .to_owned();
                    let client_id = forge_platform_twitch::client_id()
                        .ok_or_else(|| "FORGE_TWITCH_CLIENT_ID not configured".to_owned())?;
                    let user_id = bundle["user_id"]
                        .as_str()
                        .ok_or_else(|| {
                            "missing user_id — re-authorize in Settings → Platforms".to_owned()
                        })?
                        .to_owned();
                    let oauth = forge_types::OAuthToken::new(token);
                    let limiter = NoopRateLimiter;
                    forge_platform_twitch::send_chat(
                        &limiter, &oauth, &client_id, &user_id, &user_id, &msg, &bus,
                    )
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
                },
                Message::ChatSent,
            )
        }
        Message::ChatSent(Ok(())) => Task::none(),
        Message::ChatSent(Err(e)) => {
            tracing::warn!(error = %e, "chat send failed");
            Task::none()
        }
        Message::ChatPlatformFilter(platform) => {
            app.live_chat.chat_filter.platform = platform;
            Task::none()
        }
        Message::ChatToggleEventsOnly => {
            app.live_chat.chat_filter.events_only = !app.live_chat.chat_filter.events_only;
            Task::none()
        }
        Message::ChatToggleHideBots => {
            app.live_chat.chat_filter.hide_bots = !app.live_chat.chat_filter.hide_bots;
            Task::none()
        }
        Message::ChatToggleDrawer => {
            app.live_chat.drawer_open = !app.live_chat.drawer_open;
            Task::none()
        }
        Message::ChatScrolled(viewport) => {
            let rel = viewport.relative_offset();
            let at_bottom = rel.y >= 0.98;
            app.live_chat.auto_scroll = at_bottom;
            if at_bottom {
                app.live_chat.unread_count = 0;
            }
            Task::none()
        }
        Message::ChatScrollToBottom => {
            app.live_chat.auto_scroll = true;
            app.live_chat.unread_count = 0;
            iced::widget::operation::snap_to_end(crate::live_chat::chat_scroll_id())
        }
        Message::ChatToggleEmoji => {
            app.live_chat.emoji_picker_open = !app.live_chat.emoji_picker_open;
            Task::none()
        }
        Message::ChatDrawerSearchChanged(s) => {
            app.live_chat.drawer_search = s;
            Task::none()
        }
        Message::ChatDrawerSelectViewer(name) => {
            app.live_chat.selected_viewer = Some(name);
            app.live_chat.drawer_open = true;
            Task::none()
        }
        Message::ChatDrawerMenuToggle => {
            app.live_chat.drawer_menu_open = !app.live_chat.drawer_menu_open;
            Task::none()
        }
        Message::ChatDrawerMenuDismiss => {
            app.live_chat.drawer_menu_open = false;
            Task::none()
        }
        Message::Settings(sub) => match sub {
            SettingsMsg::ReconnectPlatform(PlatformId::Twitch) => {
                if let Some(handle) = app.rt.twitch_chat_handle.take() {
                    handle.shutdown();
                }
                let backend = Arc::clone(&app.rt.backend);
                let bus = Arc::clone(&app.rt.bus);
                Task::perform(
                    async move { reconnect_twitch(backend, bus).await },
                    |result| Message::Settings(SettingsMsg::PlatformReconnectResult(result)),
                )
            }
            SettingsMsg::ReconnectPlatform(_) => Task::none(),
            SettingsMsg::PlatformReconnectResult(Ok(())) => Task::none(),
            SettingsMsg::PlatformReconnectResult(Err(e)) => {
                tracing::warn!(error = %e, "platform reconnect failed");
                Task::none()
            }
            SettingsMsg::DbVacuumRequested => {
                let dp = Arc::clone(&app.rt.backend) as Arc<dyn DataProvider>;
                Task::perform(
                    async move {
                        let tmp_target = std::env::temp_dir().join("forge_vacuum.db");
                        dp.export(&tmp_target)
                            .await
                            .map(|()| tmp_target.metadata().map(|m| m.len()).unwrap_or(0))
                            .map_err(|e| e.to_string())
                    },
                    |r| Message::Settings(SettingsMsg::DbVacuumDone(r)),
                )
            }
            SettingsMsg::DbVacuumDone(result) => {
                match result {
                    Ok(bytes) => tracing::info!(bytes, "DB vacuum exported snapshot"),
                    Err(e) => tracing::warn!(error = %e, "DB vacuum failed"),
                }
                Task::none()
            }
            SettingsMsg::DbBackupRequested => {
                let dp = Arc::clone(&app.rt.backend) as Arc<dyn DataProvider>;
                Task::perform(
                    async move {
                        let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
                        let path = forge_platform_core::paths::data_dir()
                            .join(format!("forge-backup-{stamp}.db"));
                        dp.export(&path)
                            .await
                            .map(|()| path.display().to_string())
                            .map_err(|e| e.to_string())
                    },
                    |r| Message::Settings(SettingsMsg::DbBackupDone(r)),
                )
            }
            SettingsMsg::DbBackupDone(result) => {
                match result {
                    Ok(path) => tracing::info!(path = %path, "DB backup created"),
                    Err(e) => tracing::warn!(error = %e, "DB backup failed"),
                }
                Task::none()
            }
            SettingsMsg::OpenLogDirectoryRequested => {
                let log_dir = forge_platform_core::paths::data_dir().join("logs");
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            open::that(&log_dir).map_err(|e| e.to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                    },
                    |r| Message::Settings(SettingsMsg::OpenLogDirectoryResult(r)),
                )
            }
            SettingsMsg::OpenLogDirectoryResult(result) => {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "failed to open log directory");
                }
                Task::none()
            }
        },
        Message::Home(sub) => handle_home_msg(app, sub),
        Message::Globals(sub) => handle_globals_msg(app, sub),
        Message::VariantEditor(sub) => handle_variant_editor_msg(app, sub),
        Message::Actions(sub) => handle_actions_msg(app, sub),
        Message::Queues(sub) => handle_queues_msg(app, sub),
        Message::Viewers(sub) => crate::viewers::handle_msg(&mut app.viewers, sub, &app.rt.backend),
        Message::Commands(sub) => {
            crate::commands_view::handle_msg(&mut app.commands, sub, &app.rt.backend)
        }
        Message::AddAction(sub) => handle_add_action_msg(app, sub),
        Message::AddTrigger(sub) => handle_add_trigger_msg(app, sub),
        Message::AddSubAction(sub) => handle_add_sub_action_msg(app, sub),
        Message::RemoveSubAction(sub) => handle_remove_sub_action_msg(app, sub),
        Message::MoveSubAction(sub) => handle_move_sub_action_msg(app, sub),
        Message::ScriptEditor(sub) => handle_script_editor_msg(app, sub),
        Message::IntegrationDetail(sub) => handle_integration_detail_msg(app, sub),
        Message::TwitchBootResult(result) => match result {
            Ok(Some(bundle)) => {
                let login = if bundle.login.is_empty() {
                    None
                } else {
                    Some(bundle.login.clone())
                };
                let tracker = forge_platform_twitch::SubscriptionTracker::default();
                let chat = forge_platform_twitch::TwitchChat::new(
                    forge_types::OAuthToken::new(bundle.access_token),
                    bundle.client_id,
                    bundle.user_id.clone(),
                    bundle.user_id,
                    Arc::clone(&app.rt.bus),
                    Arc::clone(&tracker),
                );
                let handle = chat.start();
                let state_rx = handle.state_receiver();
                let (twitch_bundle, _health_tx) =
                    TwitchIntegrationBundle::new(login.clone(), state_rx, tracker);
                let id = IntegrationId::new("twitch");
                let icon = SectionIcon::new("brand-twitch");
                let status: Arc<dyn IntegrationStatus> = twitch_bundle.clone();
                let health: Arc<dyn IntegrationHealth> = twitch_bundle.clone();
                let content: Arc<dyn IntegrationContent> = twitch_bundle.clone();
                let quick_actions: Arc<dyn QuickActions> = twitch_bundle.clone();
                app.integration_detail = Some(IntegrationDetailState::new(
                    id,
                    icon,
                    status,
                    health,
                    content,
                    quick_actions,
                ));
                app.rt.twitch_chat_handle = Some(handle);
                app.rt.twitch_token_expires = bundle.expires_at;
                if let Some(l) = login {
                    app.rt.twitch_login = Some(l);
                }
                tracing::info!("twitch chat session restarted from stored credentials");
                Task::none()
            }
            Ok(None) => Task::none(),
            Err(e) => {
                tracing::warn!(error = %e, "twitch boot reconnect failed");
                Task::none()
            }
        },
        Message::ObsBootResult(result) => match result {
            Ok(handle) => {
                let client = handle.into_arc();
                let id = IntegrationId::new("obs");
                let icon = SectionIcon::new("broadcast");
                let status: Arc<dyn IntegrationStatus> = client.clone();
                let health: Arc<dyn IntegrationHealth> = client.clone();
                let content: Arc<dyn IntegrationContent> = client.clone();
                let quick_actions: Arc<dyn QuickActions> = client.clone();
                app.integration_detail = Some(IntegrationDetailState::new(
                    id,
                    icon,
                    status,
                    health,
                    content,
                    quick_actions,
                ));
                app.rt.obs_client = Some(client);
                Task::none()
            }
            Err(e) => {
                tracing::warn!(error = %e, "OBS boot connection failed");
                Task::none()
            }
        },
        Message::ServerBootResult(result) => {
            match result {
                Ok(snapshot) => {
                    app.server_screen.bind_address = snapshot.bind_address;
                    app.server_screen.bearer_token = snapshot.bearer_token;
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Running;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server boot failed");
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerRestartResult(result) => {
            match result {
                Ok(()) => {
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Running;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server restart failed");
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerStopResult(result) => {
            match result {
                Ok(()) => {
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Stopped;
                    app.server_screen.connected_clients.clear();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server stop failed");
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerTokenRotated(result) => {
            match result {
                Ok(token) => {
                    app.server_screen.bearer_token = token;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "token regeneration failed");
                    app.server_screen.server_status = crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::Server(crate::server_screen::ServerScreenMsg::RestartServer) => {
            let subsystem = Arc::clone(&app.rt.server_subsystem);
            Task::perform(
                async move { subsystem.restart().await.map_err(|e| e.to_string()) },
                Message::ServerRestartResult,
            )
        }
        Message::Server(crate::server_screen::ServerScreenMsg::StopServer) => {
            let subsystem = Arc::clone(&app.rt.server_subsystem);
            Task::perform(
                async move { subsystem.stop().await.map_err(|e| e.to_string()) },
                Message::ServerStopResult,
            )
        }
        Message::Server(crate::server_screen::ServerScreenMsg::RegenerateToken) => {
            let subsystem = Arc::clone(&app.rt.server_subsystem);
            Task::perform(
                async move {
                    subsystem
                        .regenerate_token()
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::ServerTokenRotated,
            )
        }
        Message::Server(sub) => handle_server_screen_msg(&mut app.server_screen, sub),
        Message::SettingsWebSocket(
            crate::settings_websocket::SettingsWebSocketMsg::SaveStatus(Ok(())),
        ) => {
            if !matches!(
                app.server_screen.server_status,
                crate::server_screen::ServerStatus::Running
            ) {
                return Task::none();
            }
            let subsystem = Arc::clone(&app.rt.server_subsystem);
            Task::perform(
                async move { subsystem.restart().await.map_err(|e| e.to_string()) },
                Message::ServerRestartResult,
            )
        }
        Message::SettingsWebSocket(sub) => {
            handle_settings_websocket_msg(&mut app.settings_websocket, sub, &app.rt.backend)
        }
        Message::TwitchPanel(sub) => handle_twitch_panel_msg(app, sub),
        Message::TwitchReauthRequested => {
            if let Some(handle) = app.rt.twitch_chat_handle.take() {
                handle.shutdown();
            }
            app.integration_detail = None;
            app.rt.twitch_login = None;
            app.rt.twitch_reauth_required = false;
            let backend = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let id = CredentialId::new("twitch:broadcaster");
                    let _ = backend.delete(&id).await;
                },
                |()| Message::Noop,
            )
        }
        Message::ObsPanel(sub) => handle_obs_panel_msg(app, sub),
        Message::Soundboard(sub) => {
            let backend = Arc::clone(&app.rt.backend);
            let player = app.rt.sound_player.clone();
            handle_soundboard_msg(&mut app.soundboard, backend, player, sub)
        }
        Message::SettingsAudio(sub) => {
            let backend = Arc::clone(&app.rt.backend);
            handle_settings_audio_msg(&mut app.settings_audio, backend, sub)
        }
        Message::Tts(sub) => handle_tts_msg(app, sub),
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
            if app.actions.renaming_action.is_some() {
                Task::done(Message::Actions(ActionsMsg::RenameCancel))
            } else if app.actions.action_menu_open.is_some() {
                Task::done(Message::Actions(ActionsMsg::DismissActionMenu))
            } else {
                Task::none()
            }
        }
        Message::Noop => Task::none(),
    }
}

fn handle_twitch_panel_msg(
    app: &mut App,
    msg: crate::twitch_panel::TwitchPanelMsg,
) -> Task<Message> {
    use crate::twitch_panel::{TwitchPanelMsg, TwitchPanelState};
    match msg {
        TwitchPanelMsg::StartConnect => {
            let Some(cid) = forge_platform_twitch::client_id() else {
                app.twitch_panel = TwitchPanelState::MissingClientId;
                return Task::none();
            };
            app.twitch_panel = TwitchPanelState::Requesting;
            let flow = Arc::new(tokio::sync::Mutex::new(
                forge_platform_twitch::TwitchAuthFlow::new(cid),
            ));
            app.rt.twitch_flow = Some(Arc::clone(&flow));
            Task::perform(crate::twitch_panel::request_code(flow), |r| {
                Message::TwitchPanel(TwitchPanelMsg::DeviceCodeReceived(r))
            })
        }
        TwitchPanelMsg::Cancel => {
            app.twitch_panel = TwitchPanelState::Disconnected;
            Task::none()
        }
        TwitchPanelMsg::CopyCode => {
            if let TwitchPanelState::AwaitingAuthorization { user_code, .. } = &app.twitch_panel {
                iced::clipboard::write::<Message>(user_code.clone())
            } else {
                Task::none()
            }
        }
        TwitchPanelMsg::OpenVerificationUrl => {
            if let TwitchPanelState::AwaitingAuthorization {
                verification_uri, ..
            } = &app.twitch_panel
            {
                let uri = verification_uri.clone();
                Task::perform(
                    async move {
                        if let Err(e) = open::that(&uri) {
                            tracing::warn!(error = %e, url = %uri, "open browser failed");
                        }
                    },
                    |()| Message::Noop,
                )
            } else {
                Task::none()
            }
        }
        TwitchPanelMsg::DeviceCodeReceived(Ok(data)) => {
            app.twitch_panel = TwitchPanelState::AwaitingAuthorization {
                user_code: data.user_code,
                verification_uri: data.verification_uri,
                expires_at: data.expires_at,
            };
            let Some(flow) = app.rt.twitch_flow.clone() else {
                app.twitch_panel = TwitchPanelState::Error("no active flow handle".into());
                return Task::none();
            };
            let creds: Arc<dyn CredentialsRepo> =
                Arc::clone(&app.rt.backend) as Arc<dyn CredentialsRepo>;
            Task::perform(crate::twitch_panel::wait_for_auth(flow, creds), |r| {
                Message::TwitchPanel(TwitchPanelMsg::AuthCompleted(r))
            })
        }
        TwitchPanelMsg::DeviceCodeReceived(Err(e)) => {
            tracing::warn!(error = %e, "twitch device code request failed");
            app.twitch_panel = TwitchPanelState::Error(e);
            Task::none()
        }
        TwitchPanelMsg::AuthCompleted(Ok(outcome)) => {
            tracing::info!(
                login = %outcome.user_info.login,
                id = %outcome.user_info.id,
                "twitch authorization complete",
            );
            let login = Some(outcome.user_info.login.clone());
            app.rt.twitch_login = login.clone();
            let tracker = forge_platform_twitch::SubscriptionTracker::default();
            let chat = forge_platform_twitch::TwitchChat::new(
                outcome.token,
                outcome.client_id,
                outcome.user_info.id.clone(),
                outcome.user_info.id,
                Arc::clone(&app.rt.bus),
                Arc::clone(&tracker),
            );
            let handle = chat.start();
            let state_rx = handle.state_receiver();
            let (twitch_bundle, _health_tx) =
                TwitchIntegrationBundle::new(login, state_rx, tracker);
            let id = IntegrationId::new("twitch");
            let icon = SectionIcon::new("brand-twitch");
            let status: Arc<dyn IntegrationStatus> = twitch_bundle.clone();
            let health: Arc<dyn IntegrationHealth> = twitch_bundle.clone();
            let content: Arc<dyn IntegrationContent> = twitch_bundle.clone();
            let quick_actions: Arc<dyn QuickActions> = twitch_bundle.clone();
            app.integration_detail = Some(IntegrationDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
            ));
            app.rt.twitch_chat_handle = Some(handle);
            app.twitch_panel = TwitchPanelState::Disconnected;
            Task::none()
        }
        TwitchPanelMsg::AuthCompleted(Err(e)) => {
            tracing::warn!(error = %e, "twitch authorization failed");
            app.twitch_panel = TwitchPanelState::Error(e);
            Task::none()
        }
    }
}

fn handle_obs_panel_msg(app: &mut App, msg: crate::obs_panel::ObsPanelMsg) -> Task<Message> {
    use crate::obs_panel::{ObsPanelMsg, TestStatus};
    match msg {
        ObsPanelMsg::HostChanged(v) => {
            app.obs_panel.form.host = v;
            app.obs_panel.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::PortChanged(v) => {
            app.obs_panel.form.port_text = v;
            app.obs_panel.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::PasswordChanged(v) => {
            app.obs_panel.form.password = v;
            app.obs_panel.test_status = TestStatus::Idle;
            Task::none()
        }
        ObsPanelMsg::TogglePasswordReveal => {
            app.obs_panel.form.password_revealed = !app.obs_panel.form.password_revealed;
            Task::none()
        }
        ObsPanelMsg::ToggleAutoReconnect => {
            app.obs_panel.form.auto_reconnect = !app.obs_panel.form.auto_reconnect;
            Task::none()
        }
        ObsPanelMsg::ToggleConnectOnLaunch => {
            app.obs_panel.form.connect_on_launch = !app.obs_panel.form.connect_on_launch;
            Task::none()
        }
        ObsPanelMsg::TestRequested => {
            let port = match app.obs_panel.form.port_text.parse::<u16>() {
                Ok(p) => p,
                Err(_) => {
                    app.obs_panel.test_status =
                        TestStatus::Failure("port must be a number 1-65535".into());
                    return Task::none();
                }
            };
            let host = app.obs_panel.form.host.clone();
            let pw = if app.obs_panel.form.password.is_empty() {
                None
            } else {
                Some(app.obs_panel.form.password.clone())
            };
            app.obs_panel.test_status = TestStatus::Running;
            Task::perform(crate::obs_panel::run_test_connect(host, port, pw), |r| {
                Message::ObsPanel(ObsPanelMsg::TestResult(r))
            })
        }
        ObsPanelMsg::TestResult(Ok(info)) => {
            app.obs_panel.test_status = TestStatus::Success(info);
            Task::none()
        }
        ObsPanelMsg::TestResult(Err(e)) => {
            app.obs_panel.test_status = TestStatus::Failure(e);
            Task::none()
        }
        ObsPanelMsg::ConnectRequested => {
            let port = match app.obs_panel.form.port_text.parse::<u16>() {
                Ok(p) => p,
                Err(_) => {
                    app.obs_panel.test_status =
                        TestStatus::Failure("port must be a number 1-65535".into());
                    return Task::none();
                }
            };
            let host = app.obs_panel.form.host.clone();
            let password = app.obs_panel.form.password.clone();
            let backend = Arc::clone(&app.rt.backend);
            let bus = Arc::clone(&app.rt.bus);
            app.obs_panel.connecting = true;
            app.obs_panel.connect_error = None;
            Task::perform(
                crate::obs_panel::connect_obs_from_form(backend, bus, host, port, password),
                |r| match r {
                    Ok(client_ref) => Message::ObsBootResult(Ok(client_ref)),
                    Err(e) => Message::ObsPanel(ObsPanelMsg::ConnectError(e)),
                },
            )
        }
        ObsPanelMsg::ConnectError(e) => {
            app.obs_panel.connecting = false;
            app.obs_panel.connect_error = Some(e.clone());
            app.obs_panel.test_status = TestStatus::Failure(e);
            Task::none()
        }
    }
}

fn handle_home_msg(app: &mut App, sub: HomeMsg) -> Task<Message> {
    match sub {
        HomeMsg::LoadStats => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move { load_home_stats(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Home(HomeMsg::StatsLoaded(r)),
            )
        }
        HomeMsg::StatsLoaded(Ok(data)) => {
            app.home.actions_count = Some(data.actions_count);
            app.home.commands_count = Some(data.commands_count);
            app.home.triggers_fired = Some(data.triggers_fired);
            app.home.globals_count = Some(data.globals_count);
            Task::none()
        }
        HomeMsg::StatsLoaded(Err(e)) => {
            tracing::warn!(error = %e, "home stats load failed");
            Task::none()
        }
    }
}

fn handle_queues_msg(app: &mut App, sub: QueuesMsg) -> Task<Message> {
    match sub {
        QueuesMsg::LoadRequested => {
            app.queues.loading = true;
            let dp = Arc::clone(&app.rt.backend);
            let scheduler = app.rt.scheduler.clone();
            Task::perform(async move { load_queues(dp, scheduler).await }, |r| {
                Message::Queues(QueuesMsg::QueuesLoaded(r))
            })
        }
        QueuesMsg::QueuesLoaded(Ok(qs)) => {
            app.queues.queues = qs;
            app.queues.loading = false;
            Task::none()
        }
        QueuesMsg::QueuesLoaded(Err(e)) => {
            app.queues.loading = false;
            tracing::warn!(error = %e, "queues load failed");
            Task::none()
        }
        QueuesMsg::PauseQueue(id) => {
            if let Some(q) = app.queues.queues.iter_mut().find(|q| q.id == id) {
                q.paused = true;
            }
            let Some(scheduler) = app.rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { scheduler.pause(id).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::PauseResult(r)),
            )
        }
        QueuesMsg::ResumeQueue(id) => {
            if let Some(q) = app.queues.queues.iter_mut().find(|q| q.id == id) {
                q.paused = false;
            }
            let Some(scheduler) = app.rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { scheduler.resume(id).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::ResumeResult(r)),
            )
        }
        QueuesMsg::DrainQueue(id) => {
            // Drain is currently implemented as "pause new dispatches" plus a
            // bus event so observers know the intent. True drain semantics
            // (let in-flight finish, then auto-pause) require scheduler state
            // machine support that is not yet in place.
            for q in &mut app.queues.queues {
                if q.id == id {
                    q.paused = true;
                }
            }
            let Some(scheduler) = app.rt.scheduler.clone() else {
                return Task::none();
            };
            let bus = Arc::clone(&app.rt.bus);
            Task::perform(
                async move {
                    bus.publish(forge_events::Event::new(
                        forge_events::EventSource::Core,
                        "queue.drain_requested",
                        serde_json::json!({ "queue_id": id.to_string() }),
                    ));
                    scheduler.pause(id).await.map_err(|e| e.to_string())
                },
                |r| Message::Queues(QueuesMsg::PauseResult(r)),
            )
        }
        QueuesMsg::PauseAll => {
            for q in &mut app.queues.queues {
                q.paused = true;
            }
            let ids: Vec<_> = app.queues.queues.iter().map(|q| q.id).collect();
            let Some(scheduler) = app.rt.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    for id in ids {
                        if let Err(e) = scheduler.pause(id).await {
                            tracing::warn!(queue_id = %id, error = %e, "pause queue failed");
                        }
                    }
                },
                |()| Message::Noop,
            )
        }
        QueuesMsg::NewQueue => {
            tracing::info!("new queue modal: TODO");
            Task::none()
        }
        QueuesMsg::PauseResult(Ok(())) => Task::none(),
        QueuesMsg::PauseResult(Err(e)) => {
            tracing::warn!(error = %e, "pause queue failed");
            Task::none()
        }
        QueuesMsg::ResumeResult(Ok(())) => Task::none(),
        QueuesMsg::ResumeResult(Err(e)) => {
            tracing::warn!(error = %e, "resume queue failed");
            Task::none()
        }
    }
}

async fn load_home_stats(dp: Arc<SqliteBackend>) -> Result<HomeStatsData, String> {
    use forge_storage::GlobalsRepo;

    let actions = dp
        .action_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let commands = dp
        .command_repo()
        .list()
        .await
        .map_err(|e| e.to_string())?
        .len();
    let globals = dp.list().await.map_err(|e| e.to_string())?.len();
    let since = time::OffsetDateTime::now_utc() - time::Duration::hours(24);
    let stats = dp
        .history_repo()
        .stats_summary(since)
        .await
        .map_err(|e| e.to_string())?;
    let triggers_fired: u64 = stats.values().map(|s| u64::from(s.runs_24h)).sum();
    Ok(HomeStatsData {
        actions_count: actions,
        commands_count: commands,
        triggers_fired,
        globals_count: globals,
    })
}

fn handle_actions_msg(app: &mut App, sub: ActionsMsg) -> Task<Message> {
    match sub {
        ActionsMsg::LoadRequested => {
            app.actions.loading = true;
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move { load_actions_tree(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::TreeLoaded(r)),
            )
        }
        ActionsMsg::TreeLoaded(Ok(tree)) => {
            app.actions.tree = tree;
            app.actions.loading = false;
            Task::none()
        }
        ActionsMsg::TreeLoaded(Err(e)) => {
            app.actions.loading = false;
            tracing::warn!(error = %e, "actions tree load failed");
            Task::none()
        }
        ActionsMsg::ActionSelected(id) => {
            let already_loaded = app.actions.selected == Some(id)
                && app
                    .actions
                    .detail
                    .as_ref()
                    .map(|d| d.action.id == id)
                    .unwrap_or(false);
            if already_loaded {
                return Task::none();
            }
            app.actions.selected = Some(id);
            app.actions.detail = None;
            app.actions.telemetry = None;
            app.actions.telemetry_loading = true;
            let dp1 = Arc::clone(&app.rt.backend);
            let dp2 = Arc::clone(&app.rt.backend);
            let detail_task = Task::perform(
                async move { load_action_detail(dp1, id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::DetailLoaded(r)),
            );
            let telemetry_task = Task::perform(async move { load_telemetry(dp2, id).await }, |r| {
                Message::Actions(ActionsMsg::TelemetryLoaded(r))
            });
            Task::batch([detail_task, telemetry_task])
        }
        ActionsMsg::DetailLoaded(Ok(detail)) => {
            app.actions.detail = Some(detail);
            Task::none()
        }
        ActionsMsg::DetailLoaded(Err(e)) => {
            app.actions.detail = None;
            tracing::warn!(error = %e, "action detail load failed");
            Task::none()
        }
        ActionsMsg::ToggleEnabled(id, enabled) => {
            if let Some(detail) = app.actions.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.enabled = enabled;
            }
            for group in &mut app.actions.tree {
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.enabled = enabled;
                    }
                }
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let Some(mut action) =
                        dp.action_repo().get(id).await.map_err(|e| e.to_string())?
                    else {
                        return Err("action not found".to_string());
                    };
                    action.enabled = enabled;
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::EnabledToggled(r)),
            )
        }
        ActionsMsg::EnabledToggled(Ok(())) => Task::none(),
        ActionsMsg::EnabledToggled(Err(e)) => {
            tracing::warn!(error = %e, "toggle enabled persist failed");
            Task::none()
        }
        ActionsMsg::TestTrigger(id) => {
            let bus = Arc::clone(&app.rt.bus);
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let detail = load_action_detail(Arc::clone(&dp), id)
                        .await
                        .map_err(|e| e.to_string())?;
                    let event = match detail.triggers.first() {
                        Some(trigger) => synthesize_test_event(trigger, &detail.commands),
                        None => Event::new(
                            EventSource::Core,
                            "test.trigger",
                            serde_json::json!({ "action_id": id.to_string() }),
                        ),
                    };
                    let event_id = event.id;
                    bus.publish(event);
                    bus.replay_and_publish(event_id)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    if let Err(e) = r {
                        tracing::warn!(error = %e, "test trigger failed");
                    }
                    Message::Noop
                },
            )
        }
        ActionsMsg::DeleteAction(id) => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move { dp.action_repo().delete(id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::ActionDeleted(r.map(|_| ()))),
            )
        }
        ActionsMsg::ActionDeleted(Ok(())) => {
            app.actions.selected = None;
            app.actions.detail = None;
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDeleted(Err(e)) => {
            tracing::warn!(error = %e, "delete action failed");
            Task::none()
        }
        ActionsMsg::DuplicateAction(id) => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let original = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "source action not found".to_string())?;
                    let mut copy = original.clone();
                    copy.id = forge_types::ActionId::new();
                    copy.name = format!("{} (copy)", original.name);
                    dp.action_repo()
                        .save(&copy)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok(copy.id)
                },
                |r| Message::Actions(ActionsMsg::ActionDuplicated(r)),
            )
        }
        ActionsMsg::ActionDuplicated(Ok(new_id)) => {
            tracing::info!(action_id = %new_id, "action duplicated");
            Task::done(Message::Actions(ActionsMsg::LoadRequested))
        }
        ActionsMsg::ActionDuplicated(Err(e)) => {
            tracing::warn!(error = %e, "duplicate action failed");
            Task::none()
        }
        ActionsMsg::DeleteTrigger(trigger_id, action_id) => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    dp.trigger_repo()
                        .delete(trigger_id)
                        .await
                        .map(|_| action_id)
                        .map_err(|e| e.to_string())
                },
                |r| Message::Actions(ActionsMsg::TriggerDeleted(r)),
            )
        }
        ActionsMsg::TriggerDeleted(Ok(action_id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(action_id)))
        }
        ActionsMsg::TriggerDeleted(Err(e)) => {
            tracing::warn!(error = %e, "delete trigger failed");
            Task::none()
        }
        ActionsMsg::OpenAddActionModal => {
            Task::done(Message::AddAction(AddActionMsg::OpenRequested))
        }
        ActionsMsg::OpenAddTriggerModal(action_id) => {
            Task::done(Message::AddTrigger(AddTriggerMsg::OpenRequested(action_id)))
        }
        ActionsMsg::SearchChanged(q) => {
            app.actions.search = q;
            Task::none()
        }
        ActionsMsg::FilterChanged(f) => {
            app.actions.filter = f;
            Task::none()
        }
        ActionsMsg::ToggleGroupCollapsed(cat) => {
            if app.actions.collapsed_groups.contains(&cat) {
                app.actions.collapsed_groups.remove(&cat);
            } else {
                app.actions.collapsed_groups.insert(cat);
            }
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Ok(t)) => {
            app.actions.telemetry = Some(t);
            app.actions.telemetry_loading = false;
            Task::none()
        }
        ActionsMsg::TelemetryLoaded(Err(e)) => {
            app.actions.telemetry = None;
            app.actions.telemetry_loading = false;
            tracing::warn!(error = %e, "action telemetry load failed");
            Task::none()
        }
        ActionsMsg::ToggleStepMenu(i) => {
            app.actions.step_menu_open = if app.actions.step_menu_open == Some(i) {
                None
            } else {
                Some(i)
            };
            Task::none()
        }
        ActionsMsg::DismissStepMenu => {
            app.actions.step_menu_open = None;
            Task::none()
        }
        ActionsMsg::ToggleActionMenu(id) => {
            app.actions.action_menu_open = if app.actions.action_menu_open == Some(id) {
                None
            } else {
                Some(id)
            };
            Task::none()
        }
        ActionsMsg::DismissActionMenu => {
            app.actions.action_menu_open = None;
            Task::none()
        }
        ActionsMsg::RenameStarted(id) => {
            let current_name = app
                .actions
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .find(|a| a.id == id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            app.actions.renaming_action = Some((id, current_name));
            app.actions.action_menu_open = None;
            iced::widget::operation::focus(action_rename_input_id())
        }
        ActionsMsg::RenameBufferChanged(buf) => {
            if let Some((_, name)) = app.actions.renaming_action.as_mut() {
                *name = buf;
            }
            Task::none()
        }
        ActionsMsg::RenameCancel => {
            app.actions.renaming_action = None;
            Task::none()
        }
        ActionsMsg::RenameSubmit => {
            let Some((id, name)) = app.actions.renaming_action.clone() else {
                return Task::none();
            };
            let trimmed = name.trim().to_owned();
            if trimmed.is_empty() {
                app.actions.renaming_action = None;
                return Task::none();
            }
            let already_taken = app
                .actions
                .tree
                .iter()
                .flat_map(|g| g.actions.iter())
                .any(|a| a.id != id && a.name.eq_ignore_ascii_case(&trimmed));
            if already_taken {
                let toast_msg = format!("Name \u{201c}{trimmed}\u{201d} is already taken");
                return Task::done(Message::Toast(ToastMsg::Fired {
                    kind: forge_widgets::ToastKind::Error,
                    message: toast_msg,
                    duration_ms: 3000,
                }));
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    use forge_storage::DataProvider;
                    let mut action = dp
                        .action_repo()
                        .get(id)
                        .await
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| "action not found".to_owned())?;
                    action.name = trimmed.clone();
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>((id, trimmed))
                },
                |r| Message::Actions(ActionsMsg::RenameSaved(r)),
            )
        }
        ActionsMsg::RenameSaved(Ok((id, new_name))) => {
            app.actions.renaming_action = None;
            for group in &mut app.actions.tree {
                let touched = group.actions.iter().any(|s| s.id == id);
                for summary in &mut group.actions {
                    if summary.id == id {
                        summary.name = new_name.clone();
                    }
                }
                if touched {
                    group.actions.sort_by_key(|a| a.name.to_lowercase());
                }
            }
            if let Some(detail) = app.actions.detail.as_mut()
                && detail.action.id == id
            {
                detail.action.name = new_name;
            }
            Task::none()
        }
        ActionsMsg::RenameSaved(Err(e)) => {
            app.actions.renaming_action = None;
            tracing::warn!(error = %e, "action rename failed");
            Task::none()
        }
    }
}

fn handle_add_action_msg(app: &mut App, sub: AddActionMsg) -> Task<Message> {
    match sub {
        AddActionMsg::OpenRequested => {
            app.actions.add_action_modal = Some(AddActionForm::new());
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    dp.queue_repo()
                        .list()
                        .await
                        .map(|qs| qs.into_iter().map(|q| (q.id, q.name)).collect())
                        .map_err(|e| e.to_string())
                },
                |r| Message::AddAction(AddActionMsg::QueueOptionsLoaded(r)),
            )
        }
        AddActionMsg::QueueOptionsLoaded(Ok(opts)) => {
            if let Some(form) = app.actions.add_action_modal.as_mut() {
                form.set_queue_options(opts);
            }
            Task::none()
        }
        AddActionMsg::QueueOptionsLoaded(Err(e)) => {
            if let Some(form) = app.actions.add_action_modal.as_mut() {
                form.error = Some(e);
            }
            Task::none()
        }
        AddActionMsg::NameChanged(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.name = v;
            }
            Task::none()
        }
        AddActionMsg::GroupChanged(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.group = v;
            }
            Task::none()
        }
        AddActionMsg::QueueSelected(name) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.select_queue_by_name(name);
            }
            Task::none()
        }
        AddActionMsg::DescriptionChanged(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.description = v;
            }
            Task::none()
        }
        AddActionMsg::EnabledToggled(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.enabled = v;
            }
            Task::none()
        }
        AddActionMsg::ConcurrentToggled(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.concurrent = v;
            }
            Task::none()
        }
        AddActionMsg::BypassPauseToggled(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.bypass_pause = v;
            }
            Task::none()
        }
        AddActionMsg::RandomPickToggled(v) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.random_pick = v;
            }
            Task::none()
        }
        AddActionMsg::Cancel => {
            app.actions.add_action_modal = None;
            Task::none()
        }
        AddActionMsg::Submit => {
            let Some(form) = app.actions.add_action_modal.as_ref() else {
                return Task::none();
            };
            if !form.is_valid() {
                return Task::none();
            }
            let Some(queue_id) = form.queue_id else {
                return Task::none();
            };
            let action = Action {
                id: ActionId::new(),
                name: form.name.trim().to_string(),
                group: if form.group.trim().is_empty() {
                    None
                } else {
                    Some(form.group.trim().to_string())
                },
                queue_id,
                enabled: form.enabled,
                concurrent: form.concurrent,
                bypass_pause: form.bypass_pause,
                execution_mode: if form.random_pick {
                    forge_types::ExecutionMode::RandomPick
                } else {
                    forge_types::ExecutionMode::Sequential
                },
                description: if form.description.trim().is_empty() {
                    None
                } else {
                    Some(form.description.trim().to_string())
                },
                sub_actions: vec![],
            };
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    dp.action_repo()
                        .save(&action)
                        .await
                        .map(|_| action.id)
                        .map_err(|e| e.to_string())
                },
                |r| Message::AddAction(AddActionMsg::Saved(r)),
            )
        }
        AddActionMsg::Saved(Ok(id)) => {
            app.actions.add_action_modal = None;
            let load = Task::done(Message::Actions(ActionsMsg::LoadRequested));
            let select = Task::done(Message::Actions(ActionsMsg::ActionSelected(id)));
            load.chain(select)
        }
        AddActionMsg::Saved(Err(e)) => {
            if let Some(f) = app.actions.add_action_modal.as_mut() {
                f.saving = false;
                f.error = Some(e);
            }
            Task::none()
        }
    }
}

fn handle_add_trigger_msg(app: &mut App, sub: AddTriggerMsg) -> Task<Message> {
    match sub {
        AddTriggerMsg::OpenRequested(action_id) => {
            app.actions.add_trigger_modal = Some(AddTriggerForm::new(action_id));
            Task::none()
        }
        AddTriggerMsg::SearchChanged(v) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.search = v;
            }
            Task::none()
        }
        AddTriggerMsg::CategorySelected(cat) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.category = cat;
            }
            Task::none()
        }
        AddTriggerMsg::KindSelected(kind) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.selected_kind = Some(kind);
                f.error = None;
            }
            Task::none()
        }
        AddTriggerMsg::CommandNameChanged(v) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.config.command_name = v;
            }
            Task::none()
        }
        AddTriggerMsg::CooldownChanged(v) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.config.cooldown_secs = v;
            }
            Task::none()
        }
        AddTriggerMsg::PermissionSelected(perm) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.config.permission = perm;
            }
            Task::none()
        }
        AddTriggerMsg::MinBitsChanged(v) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.config.min_bits = v;
            }
            Task::none()
        }
        AddTriggerMsg::Cancel => {
            app.actions.add_trigger_modal = None;
            Task::none()
        }
        AddTriggerMsg::Submit => {
            let Some(form) = app.actions.add_trigger_modal.as_ref() else {
                return Task::none();
            };
            if !form.is_valid() {
                return Task::none();
            }
            let Some(kind) = form.selected_kind.clone() else {
                return Task::none();
            };
            let action_id = form.for_action_id;
            let config = build_trigger_config(&kind, &form.config);
            let trigger = forge_types::Trigger {
                id: forge_types::TriggerId::new(),
                action_id,
                kind: kind.clone(),
                config,
            };
            let cmd = if matches!(kind, forge_types::TriggerKind::TwitchChatCommand) {
                let raw = form.config.command_name.trim();
                let normalized = format!("!{}", raw.trim_start_matches('!').to_lowercase());
                Some(forge_types::Command {
                    id: forge_types::CommandId::new(),
                    action_id,
                    name: normalized,
                    cooldown_secs: form.config.parsed_cooldown(),
                    permission: form.config.permission.clone(),
                })
            } else {
                None
            };
            let trigger_id = trigger.id;
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    dp.trigger_repo()
                        .save(&trigger)
                        .await
                        .map_err(|e| e.to_string())?;
                    if let Some(c) = cmd {
                        dp.command_repo()
                            .save(&c)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(trigger_id)
                },
                |r| Message::AddTrigger(AddTriggerMsg::Saved(r)),
            )
        }
        AddTriggerMsg::Saved(Ok(_)) => {
            let action_id = app
                .actions
                .add_trigger_modal
                .as_ref()
                .map(|f| f.for_action_id);
            app.actions.add_trigger_modal = None;
            if let Some(id) = action_id {
                Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
            } else {
                Task::none()
            }
        }
        AddTriggerMsg::Saved(Err(e)) => {
            if let Some(f) = app.actions.add_trigger_modal.as_mut() {
                f.saving = false;
                f.error = Some(e);
            }
            Task::none()
        }
    }
}

fn handle_add_sub_action_msg(app: &mut App, sub: AddSubActionMsg) -> Task<Message> {
    match sub {
        AddSubActionMsg::OpenRequested(action_id) => {
            app.actions.add_sub_action_modal = Some(AddSubActionForm::new(action_id));
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(load_clip_options(dp), |clips| {
                Message::AddSubAction(AddSubActionMsg::ClipsLoaded(clips))
            })
        }
        AddSubActionMsg::EditRequested(action_id, index) => {
            let mut form = AddSubActionForm::new(action_id);
            form.editing_index = Some(index);
            if let Some(detail) = app.actions.detail.as_ref()
                && detail.action.id == action_id
                && let Some(spec) = detail.action.sub_actions.get(index)
            {
                form.populate_from_spec(spec);
            }
            app.actions.add_sub_action_modal = Some(form);
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(load_clip_options(dp), |clips| {
                Message::AddSubAction(AddSubActionMsg::ClipsLoaded(clips))
            })
        }
        AddSubActionMsg::KindSelected(kind) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.kind = kind;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SendChatMessageChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.send_chat_message = v;
            }
            Task::none()
        }
        AddSubActionMsg::SendChatTargetChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.send_chat_target = v;
            }
            Task::none()
        }
        AddSubActionMsg::SetGlobalNameChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.set_global_name = v;
            }
            Task::none()
        }
        AddSubActionMsg::SetGlobalValueChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.set_global_value = v;
            }
            Task::none()
        }
        AddSubActionMsg::DelayMsChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.delay_ms = v;
            }
            Task::none()
        }
        AddSubActionMsg::LogLevelSelected(level) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.log_level = level;
            }
            Task::none()
        }
        AddSubActionMsg::LogMessageChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.log_message = v;
            }
            Task::none()
        }
        AddSubActionMsg::PlaySoundClipSelected(clip_id) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.play_sound_clip_id = Some(clip_id);
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SpeakTextChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.speak_text = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::SpeakVoiceOverrideChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.speak_voice_override = v;
            }
            Task::none()
        }
        AddSubActionMsg::ReadFilePathChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.read_file_path = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::ReadFileTargetVarChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.read_file_target_var = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntMinChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.random_int_min = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntMaxChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.random_int_max = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::RandomIntTargetVarChanged(v) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.config.random_int_target_var = v;
                f.error = None;
            }
            Task::none()
        }
        AddSubActionMsg::ClipsLoaded(clips) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.available_clips = clips;
            }
            Task::none()
        }
        AddSubActionMsg::Cancel => {
            app.actions.add_sub_action_modal = None;
            Task::none()
        }
        AddSubActionMsg::Submit => {
            let Some(form) = app.actions.add_sub_action_modal.as_ref() else {
                return Task::none();
            };
            if !form.is_valid() {
                let error_msg = match form.kind {
                    SubActionKindChoice::SendChat => "Message is required.",
                    SubActionKindChoice::SetGlobal => "Variable name is required.",
                    SubActionKindChoice::Delay => "Milliseconds must be a non-negative integer.",
                    SubActionKindChoice::Log => "Log message is required.",
                    SubActionKindChoice::PlaySound => "Select a clip to play.",
                    SubActionKindChoice::Speak => "Speak text is required.",
                    SubActionKindChoice::ReadFile => "Path and target variable are required.",
                    SubActionKindChoice::RandomInt => {
                        "min, max (min ≤ max), and target variable are required."
                    }
                };
                if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                    f.error = Some(error_msg.to_string());
                }
                return Task::none();
            }
            let spec = match form.kind {
                SubActionKindChoice::SendChat => forge_types::SubActionSpec::SendChat {
                    message: form.config.send_chat_message.clone(),
                    target: form.config.send_chat_target.clone(),
                },
                SubActionKindChoice::SetGlobal => forge_types::SubActionSpec::SetGlobal {
                    name: form.config.set_global_name.clone(),
                    value: form.config.set_global_value.clone(),
                },
                SubActionKindChoice::Delay => {
                    let ms = form.config.delay_ms.trim().parse::<u64>().unwrap_or(0);
                    forge_types::SubActionSpec::Delay { ms }
                }
                SubActionKindChoice::Log => forge_types::SubActionSpec::Log {
                    level: form.config.log_level.clone(),
                    message: form.config.log_message.clone(),
                },
                SubActionKindChoice::PlaySound => forge_types::SubActionSpec::PlaySound {
                    clip_id: form.config.play_sound_clip_id.unwrap_or_default(),
                    output_device_override: None,
                },
                SubActionKindChoice::Speak => forge_types::SubActionSpec::Speak {
                    text: form.config.speak_text.clone(),
                    voice_id_override: if form.config.speak_voice_override.trim().is_empty() {
                        None
                    } else {
                        Some(form.config.speak_voice_override.trim().to_owned())
                    },
                },
                SubActionKindChoice::ReadFile => forge_types::SubActionSpec::ReadFile {
                    path: form.config.read_file_path.trim().to_owned(),
                    target_var: form.config.read_file_target_var.trim().to_owned(),
                },
                SubActionKindChoice::RandomInt => {
                    let min = form
                        .config
                        .random_int_min
                        .trim()
                        .parse::<i64>()
                        .unwrap_or(0);
                    let max = form
                        .config
                        .random_int_max
                        .trim()
                        .parse::<i64>()
                        .unwrap_or(0);
                    forge_types::SubActionSpec::RandomInt {
                        min,
                        max,
                        target_var: form.config.random_int_target_var.trim().to_owned(),
                    }
                }
            };
            let action_id = form.for_action_id;
            let editing_index = form.editing_index;
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    save_sub_action(dp, action_id, spec, editing_index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::AddSubAction(AddSubActionMsg::Saved(r)),
            )
        }
        AddSubActionMsg::Saved(Ok(())) => {
            let selected = app.actions.selected;
            app.actions.add_sub_action_modal = None;
            match selected {
                Some(id) => Task::done(Message::Actions(ActionsMsg::ActionSelected(id))),
                None => Task::none(),
            }
        }
        AddSubActionMsg::Saved(Err(e)) => {
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.saving = false;
                f.error = Some(e);
            }
            Task::none()
        }
        AddSubActionMsg::DuplicateRequested(action_id, index) => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    duplicate_sub_action(dp, action_id, index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::AddSubAction(AddSubActionMsg::Duplicated(r)),
            )
        }
        AddSubActionMsg::Duplicated(Ok(id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
        }
        AddSubActionMsg::Duplicated(Err(e)) => {
            tracing::warn!(error = %e, "duplicate sub-action failed");
            Task::none()
        }
    }
}

fn handle_remove_sub_action_msg(app: &mut App, sub: RemoveSubActionMsg) -> Task<Message> {
    match sub {
        RemoveSubActionMsg::Requested(action_id, index) => {
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    remove_sub_action(dp, action_id, index)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::RemoveSubAction(RemoveSubActionMsg::Removed(r)),
            )
        }
        RemoveSubActionMsg::Removed(Ok(())) => match app.actions.selected {
            Some(id) => Task::done(Message::Actions(ActionsMsg::ActionSelected(id))),
            None => Task::none(),
        },
        RemoveSubActionMsg::Removed(Err(e)) => {
            tracing::warn!(error = %e, "remove sub-action persist failed");
            Task::none()
        }
    }
}

fn handle_move_sub_action_msg(app: &mut App, sub: MoveSubActionMsg) -> Task<Message> {
    let total = app
        .actions
        .detail
        .as_ref()
        .map(|d| d.action.sub_actions.len())
        .unwrap_or(0);

    match sub {
        MoveSubActionMsg::Up(action_id, i) => {
            if i == 0 {
                return Task::none();
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    move_sub_action(dp, action_id, i, i - 1)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::MoveSubAction(MoveSubActionMsg::Moved(r)),
            )
        }
        MoveSubActionMsg::Down(action_id, i) => {
            if total == 0 || i + 1 >= total {
                return Task::none();
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    move_sub_action(dp, action_id, i, i + 1)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::MoveSubAction(MoveSubActionMsg::Moved(r)),
            )
        }
        MoveSubActionMsg::ToTop(action_id, i) => {
            if i == 0 {
                return Task::none();
            }
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    move_sub_action(dp, action_id, i, 0)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::MoveSubAction(MoveSubActionMsg::Moved(r)),
            )
        }
        MoveSubActionMsg::ToBottom(action_id, i) => {
            if total == 0 || i + 1 >= total {
                return Task::none();
            }
            let last = total - 1;
            let dp = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    move_sub_action(dp, action_id, i, last)
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| Message::MoveSubAction(MoveSubActionMsg::Moved(r)),
            )
        }
        MoveSubActionMsg::Moved(Ok(id)) => {
            Task::done(Message::Actions(ActionsMsg::ActionSelected(id)))
        }
        MoveSubActionMsg::Moved(Err(e)) => {
            tracing::warn!(error = %e, "move sub-action failed");
            Task::none()
        }
    }
}

fn build_trigger_config(
    kind: &forge_types::TriggerKind,
    form: &crate::actions::TriggerConfigForm,
) -> forge_types::TriggerConfig {
    let mut m = std::collections::BTreeMap::new();
    match kind {
        forge_types::TriggerKind::TwitchChatCommand => {
            m.insert(
                "cooldown_secs".to_string(),
                forge_types::Variant::Int(form.parsed_cooldown() as i64),
            );
        }
        forge_types::TriggerKind::TwitchCheer => {
            m.insert(
                "min_bits".to_string(),
                forge_types::Variant::Int(form.parsed_min_bits() as i64),
            );
        }
        _ => {}
    }
    m
}

async fn reconnect_twitch(backend: Arc<SqliteBackend>, bus: Arc<EventBus>) -> Result<(), String> {
    use forge_platform_twitch::{TwitchChat, client_id};

    let cid = client_id().ok_or_else(|| "FORGE_TWITCH_CLIENT_ID not set".to_owned())?;
    let bundle_json = backend
        .load(&CredentialId::new("twitch:broadcaster"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no Twitch credential stored".to_owned())?;

    let bundle: serde_json::Value =
        serde_json::from_str(&bundle_json).map_err(|e| e.to_string())?;
    let access = bundle["access_token"]
        .as_str()
        .ok_or_else(|| "missing access_token in credential bundle".to_owned())?
        .to_owned();
    let user_id = bundle["user_id"]
        .as_str()
        .ok_or_else(|| "missing user_id — re-authorize in Settings → Platforms".to_owned())?
        .to_owned();

    let token = forge_types::OAuthToken::new(access);
    let tracker = forge_platform_twitch::SubscriptionTracker::default();
    TwitchChat::new(token, cid, user_id.clone(), user_id, bus, tracker).start();
    Ok(())
}

pub async fn load_twitch_credential(
    backend: Arc<SqliteBackend>,
) -> Result<Option<crate::message::TwitchBootBundle>, String> {
    let Some(client_id) = forge_platform_twitch::client_id() else {
        return Ok(None);
    };
    let Some(json) = backend
        .load(&CredentialId::new("twitch:broadcaster"))
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let bundle: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let access_token = bundle["access_token"]
        .as_str()
        .ok_or_else(|| "missing access_token in twitch credential bundle".to_owned())?
        .to_owned();
    let user_id = bundle["user_id"]
        .as_str()
        .ok_or_else(|| "missing user_id in twitch credential bundle".to_owned())?
        .to_owned();
    let login = bundle["login"].as_str().unwrap_or_default().to_owned();
    let expires_at = bundle["expires_at_unix"].as_i64().and_then(|secs| {
        if secs <= 0 {
            None
        } else {
            Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64))
        }
    });
    Ok(Some(crate::message::TwitchBootBundle {
        access_token,
        client_id,
        user_id,
        login,
        expires_at,
    }))
}

pub async fn load_obs_and_connect(
    backend: Arc<SqliteBackend>,
    bus: Arc<EventBus>,
) -> Result<ObsClientRef, String> {
    let Some(json) = backend
        .load(&CredentialId::new("obs:default"))
        .await
        .map_err(|e| e.to_string())?
    else {
        return Err("obs:default credentials not stored".to_owned());
    };

    let bundle: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let url = bundle["url"]
        .as_str()
        .ok_or_else(|| "missing url in OBS credential bundle".to_owned())?
        .to_owned();
    let password = bundle["password"].as_str().unwrap_or("").to_owned();

    let publisher: Arc<dyn EventPublisher> = bus;
    let pw: Option<&str> = if password.is_empty() {
        None
    } else {
        Some(&password)
    };

    let client = ObsClient::connect(&url, pw, publisher)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ObsClientRef::new(Arc::new(client)))
}

fn handle_tts_msg(app: &mut App, msg: crate::message::TtsMsg) -> Task<Message> {
    use crate::message::TtsMsg;
    match msg {
        TtsMsg::Dashboard(sub) => handle_tts_dash_msg(&mut app.tts_dashboard, sub),
        TtsMsg::Engines(sub) => handle_tts_engines_msg(&mut app.tts_engines, sub),
        TtsMsg::Aliases(sub) => handle_voice_aliases_msg(&mut app.tts_aliases, sub),
        TtsMsg::Filters(sub) => handle_tts_filters_msg(&mut app.tts_filters, sub),
        TtsMsg::Triggers(sub) => handle_tts_triggers_msg(&mut app.tts_triggers, sub),
    }
}

struct NoopRateLimiter;

#[async_trait::async_trait]
impl forge_platform_core::RateLimiter for NoopRateLimiter {
    async fn acquire(
        &self,
        _weight: u32,
    ) -> Result<forge_platform_core::RateLimitOutcome, forge_platform_core::PlatformError> {
        Ok(forge_platform_core::RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        u32::MAX
    }

    async fn observe_remote_throttle(&self, _retry_after: std::time::Duration) {}
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

fn event_source_label(source: EventSource) -> &'static str {
    match source {
        EventSource::Twitch => "twitch",
        EventSource::YouTube => "youtube",
        EventSource::Kick => "kick",
        EventSource::Trovo => "trovo",
        EventSource::Core => "core",
        EventSource::Rhai => "rhai",
        EventSource::Http => "http",
        EventSource::Obs => "obs",
        EventSource::VTube => "vtube",
        EventSource::Discord => "discord",
        EventSource::Midi => "midi",
        EventSource::Hotkey => "hotkey",
        EventSource::Timer => "timer",
        EventSource::Server => "server",
        EventSource::Audio => "audio",
    }
}

fn home_inline_button<'a>(
    icon: Icon,
    label: &'a str,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, row, text};
    use iced::{Alignment, Background, Border, Shadow};

    let icon_color = palette.text_secondary;
    let text_color = palette.text_secondary;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);

    let content = row![
        tabler_icon(icon, 12.0, icon_color),
        text(label).size(FONT_SM).color(text_color),
    ]
    .spacing(5.0)
    .align_y(Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding([6.0, 12.0])
        .style(move |_theme: &Theme, status| {
            let bg = if matches!(status, iced::widget::button::Status::Hovered) {
                Some(Background::Color(iced::Color {
                    a: 0.06,
                    ..border_color
                }))
            } else {
                Some(Background::Color(iced::Color::TRANSPARENT))
            };
            button::Style {
                background: bg,
                text_color,
                border: Border {
                    color: border_color,
                    width: 0.5,
                    radius: r.into(),
                },
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .into()
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
        app.server_screen.server_status,
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

fn home_hero<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let brand = palette.brand;
    let shell = palette.shell;

    let brand_box = container(text("F").size(26.0).color(shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(54.0)
    .height(54.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| iced::widget::container::Style {
        background: Some(Background::Color(brand)),
        border: Border {
            radius: 12.0.into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let title_col = column![
        text("Forge").size(22.0).color(palette.text_primary),
        text("Open-source stream automation, forged for streamers")
            .size(FONT_SM)
            .color(palette.text_muted),
    ]
    .spacing(2.0);

    let import_btn = home_inline_button(Icon::Download, "Import", Message::Noop, palette);
    let new_action_btn = home_inline_button(
        Icon::Plus,
        "New action",
        Message::Navigate(Screen::Actions),
        palette,
    );

    let buttons_row = row![import_btn, new_action_btn].spacing(8.0);

    let inner = row![
        brand_box,
        container(title_col).width(Length::Fill),
        buttons_row,
    ]
    .spacing(18.0)
    .align_y(Alignment::Center);

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;

    container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 22.0,
            right: 24.0,
            bottom: 22.0,
            left: 24.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Lg).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_jump_cards<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::{BigJumpCardProps, big_jump_card};
    use iced::widget::{container, row};

    let actions_count = app.home.actions_count.unwrap_or(0);
    let triggers_fired = app.home.triggers_fired.unwrap_or(0);
    let chat_count = app.live_chat.chat_log.len();
    let twitch_ok = app.rt.twitch_chat_handle.is_some();
    let obs_ok = app.rt.obs_client.is_some();
    let total_integrations: u8 = 6;
    let connected_integrations: u8 = u8::from(twitch_ok) + u8::from(obs_ok);
    let connections_warn = connected_integrations < total_integrations;

    let card_chat = big_jump_card(
        BigJumpCardProps {
            icon: Icon::MessageCircle,
            icon_color: palette.brand,
            section_label: "AUDIENCE",
            title: "Chat",
            stat: chat_count.to_string(),
            stat_label: "viewers tracked".to_string(),
            hint: "Talk to your audience and see who's watching",
            on_press: Message::Navigate(Screen::LiveChat),
            warn: false,
        },
        palette,
    );

    let card_actions = big_jump_card(
        BigJumpCardProps {
            icon: Icon::Bolt,
            icon_color: palette.warning,
            section_label: "AUTOMATION",
            title: "Actions",
            stat: actions_count.to_string(),
            stat_label: format!("actions · {triggers_fired} fired today"),
            hint: "Set up triggers, commands and timers",
            on_press: Message::Navigate(Screen::Actions),
            warn: false,
        },
        palette,
    );

    let card_connections = big_jump_card(
        BigJumpCardProps {
            icon: Icon::Plug,
            icon_color: palette.success,
            section_label: "CONNECTIONS",
            title: "Connections",
            stat: format!("{connected_integrations}/{total_integrations}"),
            stat_label: "connected".to_string(),
            hint: "Manage platforms, apps and modules",
            on_press: Message::Navigate(Screen::Platforms),
            warn: connections_warn,
        },
        palette,
    );

    row![
        container(card_chat).width(Length::FillPortion(1)),
        container(card_actions).width(Length::FillPortion(1)),
        container(card_connections).width(Length::FillPortion(1)),
    ]
    .spacing(10.0)
    .into()
}

fn home_stream_health<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let success = palette.success;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;

    let header_icon = tabler_icon(Icon::ChartLine, 14.0, success);
    let live_dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(success)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let live_badge = row![
        live_dot,
        text("LIVE")
            .size(FONT_XS)
            .color(success)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(4.0)
    .align_y(Alignment::Center);

    let header_left = row![
        header_icon,
        text("Stream health").size(FONT_SM).color(text_primary),
        live_badge,
    ]
    .spacing(7.0)
    .align_y(Alignment::Center);

    let header_right = text("last 60s · auto-refresh")
        .size(FONT_XS)
        .color(text_faint)
        .font(font(FontRole::Monospace));

    let header = row![
        header_left,
        iced::widget::Space::new().width(Length::Fill),
        header_right,
    ]
    .align_y(Alignment::Center);

    let sparkline_col = column![
        text("THROUGHPUT · ev/s")
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
        forge_widgets::throughput_sparkline(&[], "ev/s", palette),
    ]
    .spacing(4.0)
    .width(Length::FillPortion(14));

    let health_stat =
        |label: &'a str, value: String, unit: Option<&'a str>| -> Element<'a, Message> {
            let val_el: Element<'a, Message> = if let Some(u) = unit {
                row![
                    text(value)
                        .size(FONT_MD)
                        .color(text_primary)
                        .font(font(FontRole::Monospace)),
                    text(u).size(FONT_XS).color(text_muted),
                ]
                .spacing(4.0)
                .align_y(Alignment::Center)
                .into()
            } else {
                text(value)
                    .size(FONT_MD)
                    .color(text_primary)
                    .font(font(FontRole::Monospace))
                    .into()
            };
            column![
                text(label)
                    .size(FONT_XS)
                    .color(text_faint)
                    .font(font(FontRole::Monospace)),
                val_el,
            ]
            .spacing(3.0)
            .width(Length::FillPortion(10))
            .into()
        };

    let (fps_val, cpu_val, dropped_val) = if let Some(client) = &app.rt.obs_client {
        let snap = client.health_snapshot();
        let fps = format!("{:.1}", snap.fps);
        let cpu = format!("{:.1}", snap.cpu_percent);
        let dropped = if snap.total_frames > 0 {
            format!(
                "{} ({:.2}%)",
                snap.dropped_frames,
                (snap.dropped_frames as f64 / snap.total_frames as f64) * 100.0
            )
        } else {
            snap.dropped_frames.to_string()
        };
        (fps, cpu, dropped)
    } else {
        (
            "\u{2014}".to_owned(),
            "\u{2014}".to_owned(),
            "\u{2014}".to_owned(),
        )
    };

    let stats_row = row![
        sparkline_col,
        health_stat("BITRATE · OBS", "\u{2014}".to_owned(), Some("kbps")),
        health_stat("DROPPED · OBS", dropped_val, None),
        health_stat("FPS", fps_val, None),
        health_stat("CPU", cpu_val, Some("%")),
    ]
    .spacing(12.0)
    .align_y(Alignment::End);

    let card_content = column![header, stats_row].spacing(8.0);

    container(card_content)
        .width(Length::Fill)
        .padding(14.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_connection_cell<'a>(
    label: &'a str,
    dot_color: iced::Color,
    ok: bool,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Shadow};

    let text_primary = palette.text_primary;
    let text_faint = palette.text_faint;
    let success = palette.success;
    let elevated = palette.elevated;
    let shell = palette.shell;
    let border_regular = palette.border_regular;

    let platform_dot = container(iced::widget::Space::new())
        .width(10.0)
        .height(10.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let status_color = if ok { success } else { text_faint };
    let status_str = if ok { "connected" } else { "offline" };

    let label_col = column![
        text(label).size(FONT_XS).color(text_primary),
        text(status_str)
            .size(FONT_XS)
            .color(status_color)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(2.0)
    .width(Length::Fill);

    let status_dot = container(iced::widget::Space::new())
        .width(8.0)
        .height(8.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(status_color)),
            border: Border {
                radius: 4.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let content = row![platform_dot, label_col, status_dot]
        .spacing(10.0)
        .align_y(Alignment::Center);

    button(content)
        .on_press(on_press)
        .padding(iced::Padding {
            top: 12.0,
            right: 14.0,
            bottom: 12.0,
            left: 14.0,
        })
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, iced::widget::button::Status::Hovered) {
                    shell
                } else {
                    elevated
                },
            )),
            border: Border {
                color: border_regular,
                width: 0.0,
                radius: 0.0.into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .into()
}

fn home_connections_strip<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let twitch_ok = app.rt.twitch_chat_handle.is_some();
    let obs_ok = app.rt.obs_client.is_some();
    let connected: u8 = u8::from(twitch_ok) + u8::from(obs_ok);
    let disconnected: u8 = 6u8.saturating_sub(connected);

    let header_icon = tabler_icon(Icon::PlugConnected, 14.0, palette.success);
    let header_title = text("Integrations")
        .size(FONT_SM)
        .color(palette.text_primary);
    let header_sub = text(format!("{connected} active · {disconnected} disconnected"))
        .size(FONT_XS)
        .color(palette.text_faint);

    let header = row![
        header_icon,
        header_title,
        header_sub,
        iced::widget::Space::new().width(Length::Fill),
    ]
    .spacing(7.0)
    .align_y(Alignment::Center);

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let surface_overlay = palette.surface_overlay;

    let header_bar = container(header)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 10.0,
            right: 14.0,
            bottom: 10.0,
            left: 14.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: iced::border::Radius {
                    top_left: radius(Radius::Md),
                    top_right: radius(Radius::Md),
                    bottom_left: 0.0,
                    bottom_right: 0.0,
                },
            },
            ..iced::widget::container::Style::default()
        });

    let cells = row![
        container(home_connection_cell(
            "Twitch",
            palette.brand,
            twitch_ok,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("twitch")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
        container(home_connection_cell(
            "YouTube",
            palette.random,
            false,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("youtube")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
        container(home_connection_cell(
            "Kick",
            palette.info,
            false,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("kick")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
        container(home_connection_cell(
            "Trovo",
            palette.success,
            false,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("trovo")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
        container(home_connection_cell(
            "OBS",
            palette.success,
            obs_ok,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("obs")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
        container(home_connection_cell(
            "VTube",
            palette.warning,
            false,
            Message::Navigate(Screen::IntegrationDetail(
                forge_platform_core::IntegrationId::new("vtube")
            )),
            palette,
        ))
        .width(Length::FillPortion(1)),
    ]
    .spacing(1.0);

    let cells_container = container(cells)
        .width(Length::Fill)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(surface_overlay)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: iced::border::Radius {
                    top_left: 0.0,
                    top_right: 0.0,
                    bottom_left: radius(Radius::Md),
                    bottom_right: radius(Radius::Md),
                },
            },
            ..iced::widget::container::Style::default()
        });

    column![header_bar, cells_container]
        .spacing(0.0)
        .width(Length::Fill)
        .into()
}

fn home_system_event_row<'a>(
    event: &'a forge_events::Event,
    has_bottom_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{color_for_source, source_label};
    use iced::widget::{button, container, row as irow, text};
    use iced::{Alignment, Background, Border, Shadow};

    let dot_color = color_for_source(event.source, palette);
    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let shell = palette.shell;
    let text_primary = palette.text_primary;

    let dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(dot_color)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let ts_str = format!(
        "{:02}:{:02}:{:02}",
        event.timestamp.hour(),
        event.timestamp.minute(),
        event.timestamp.second()
    );

    let ts_col = container(
        text(ts_str)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    )
    .width(60.0);

    let source_str = source_label(event.source);
    let summary_str = crate::event_feed::format_summary(event);
    let full = format!("{}: {}", source_str, summary_str);

    let description: Element<'a, Message> = text(full)
        .size(FONT_XS)
        .color(text_primary)
        .width(Length::Fill)
        .into();

    let inner = irow![dot, ts_col, description]
        .spacing(10.0)
        .align_y(Alignment::Center);

    let border_width = if has_bottom_border { 0.5 } else { 0.0 };

    let styled_row = container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 7.0,
            right: 4.0,
            bottom: 7.0,
            left: 4.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    button(styled_row)
        .on_press(Message::Navigate(Screen::EventFeed))
        .style(move |_theme: &Theme, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, iced::widget::button::Status::Hovered) {
                    shell
                } else {
                    elevated
                },
            )),
            border: Border {
                color: iced::Color::TRANSPARENT,
                width: 0.0,
                radius: 4.0.into(),
            },
            text_color: text_primary,
            shadow: Shadow::default(),
            snap: false,
        })
        .padding(0.0)
        .width(Length::Fill)
        .into()
}

fn home_recent_events_card<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let success = palette.success;
    let text_faint = palette.text_faint;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;

    let live_dot = container(iced::widget::Space::new())
        .width(6.0)
        .height(6.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(success)),
            border: Border {
                radius: 3.0.into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let live_label = row![
        live_dot,
        text("LIVE")
            .size(FONT_XS)
            .color(text_faint)
            .font(font(FontRole::Monospace)),
    ]
    .spacing(5.0)
    .align_y(Alignment::Center);

    let header = row![
        text("Recent events").size(FONT_SM).color(text_primary),
        iced::widget::Space::new().width(Length::Fill),
        live_label,
    ]
    .align_y(Alignment::Center);

    let recent: Vec<&forge_events::Event> = app.event_feed.events.iter().rev().take(5).collect();

    let body: Element<'a, Message> = if recent.is_empty() {
        text("No events yet").size(FONT_XS).color(text_muted).into()
    } else {
        let count = recent.len();
        let mut col = column![].spacing(0.0);
        for (i, row_data) in recent.into_iter().enumerate() {
            col = col.push(home_system_event_row(row_data, i + 1 < count, palette));
        }
        col.into()
    };

    container(column![header, body].spacing(10.0))
        .width(Length::FillPortion(14))
        .padding(14.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_glance_row<'a>(
    label: &'a str,
    value: String,
    color: iced::Color,
    last: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Alignment, Border};

    let border_regular = palette.border_regular;
    let text_muted = palette.text_muted;

    let inner = row![
        text(label)
            .size(FONT_XS)
            .color(text_muted)
            .width(Length::Fill),
        text(value)
            .size(FONT_SM)
            .color(color)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: 5.0,
        right: 0.0,
        bottom: 5.0,
        left: 0.0,
    });

    let border_width = if last { 0.0 } else { 0.5 };

    container(inner)
        .width(Length::Fill)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_glance_card<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, text};
    use iced::{Background, Border};

    let elevated = palette.elevated;
    let border_regular = palette.border_regular;
    let text_primary = palette.text_primary;

    let actions_val = app
        .home
        .actions_count
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());
    let commands_val = app
        .home
        .commands_count
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());
    let fired_val = app
        .home
        .triggers_fired
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());
    let globals_val = app
        .home
        .globals_count
        .map_or_else(|| "\u{2014}".to_string(), |n| n.to_string());

    let header = text("At a glance").size(FONT_SM).color(text_primary);

    let content = column![
        header,
        home_glance_row("Actions", actions_val, palette.brand, false, palette),
        home_glance_row("Commands", commands_val, palette.info, false, palette),
        home_glance_row(
            "Fired this session",
            fired_val,
            palette.success,
            false,
            palette
        ),
        home_glance_row("Globals", globals_val, palette.warning, true, palette),
    ]
    .spacing(0.0);

    container(content)
        .width(Length::FillPortion(10))
        .padding(14.0)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(elevated)),
            border: Border {
                color: border_regular,
                width: 0.5,
                radius: radius(Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn home_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container};

    let page_header = simple_page_header(&[("Home", true)], palette);

    let hero = home_hero(palette);
    let jump_cards = home_jump_cards(app, palette);
    let connections = home_connections_strip(app, palette);
    let bottom = iced::widget::row![
        home_recent_events_card(app, palette),
        home_glance_card(app, palette),
    ]
    .spacing(12.0);

    let mut content = column![hero, jump_cards,].spacing(16.0).width(Length::Fill);

    if app.rt.obs_client.is_some() {
        content = content.push(home_stream_health(app, palette));
    }

    content = content.push(connections).push(bottom);

    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 22.0,
            right: 28.0,
            bottom: 22.0,
            left: 28.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.base)),
            ..iced::widget::container::Style::default()
        });

    column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(crate) fn simple_page_header<'a>(
    crumbs: &[(&'a str, bool)],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    page_header_with_actions(crumbs, None, palette)
}

pub(crate) fn page_header_with_actions<'a>(
    crumbs: &[(&'a str, bool)],
    right: Option<Element<'a, Message>>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Background, Border};

    let p = *palette;
    let mut crumb_row = row![tabler_icon(Icon::Home, 13.0, p.text_faint)]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    for (label, is_last) in crumbs {
        crumb_row = crumb_row.push(tabler_icon(Icon::ChevronRight, 11.0, p.text_faint));
        let color = if *is_last {
            p.text_primary
        } else {
            p.text_muted
        };
        crumb_row = crumb_row.push(text(label.to_string()).size(FONT_SM).color(color));
    }

    let inner: Element<'a, Message> = if let Some(right_el) = right {
        row![
            crumb_row,
            iced::widget::Space::new().width(Length::Fill),
            right_el,
        ]
        .align_y(iced::alignment::Vertical::Center)
        .into()
    } else {
        crumb_row.into()
    };

    container(inner)
        .width(Length::Fill)
        .padding([10_u16, 16_u16])
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(p.shell)),
            border: Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

pub(crate) fn header_divider<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let p = *palette;
    iced::widget::container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn settings_section_button<'a>(
    label: &'a str,
    section: SettingsSection,
    active: &SettingsSection,
    palette: &ForgePalette,
) -> Element<'a, Message> {
    if &section == active {
        forge_widgets::primary_button(label, Message::Navigate(Screen::Settings(section)), palette)
    } else {
        forge_widgets::ghost_button(label, Message::Navigate(Screen::Settings(section)), palette)
    }
}

fn settings_diagnostics_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let version = env!("CARGO_PKG_VERSION");
    let log_dir = forge_platform_core::paths::data_dir().join("logs");

    let metrics = iced::widget::row![
        forge_widgets::metric_card("Build", version, None::<&str>, palette),
        forge_widgets::metric_card("Rust", "1.95.0", None::<&str>, palette),
        forge_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, palette),
    ]
    .spacing(12);

    let log_path_label = iced::widget::text(format!("Log directory: {}", log_dir.display()))
        .size(FONT_SM)
        .color(palette.text_muted);
    let open_logs_btn = forge_widgets::primary_button(
        "Open log directory",
        Message::Settings(SettingsMsg::OpenLogDirectoryRequested),
        palette,
    );
    let level_label =
        iced::widget::text("Log level: controlled via RUST_LOG env var (e.g. info, debug, trace).")
            .size(FONT_SM)
            .color(palette.text_muted);

    let logs_card = forge_widgets::card(
        [
            iced::widget::text("Logs & diagnostics")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            log_path_label.into(),
            open_logs_btn,
            level_label.into(),
        ],
        palette,
    );

    iced::widget::container(iced::widget::column![metrics, logs_card].spacing(16))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn settings_storage_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let db_path = forge_platform_core::paths::data_dir().join("forge.db");
    let path_label = iced::widget::text(format!("Database: {}", db_path.display()))
        .size(FONT_SM)
        .color(palette.text_muted);

    let vacuum_btn = forge_widgets::primary_button(
        "Vacuum (export compact snapshot)",
        Message::Settings(SettingsMsg::DbVacuumRequested),
        palette,
    );
    let vacuum_hint = iced::widget::text(
        "Writes a vacuumed snapshot to a temp file; useful before manual backups.",
    )
    .size(FONT_XS)
    .color(palette.text_faint);

    let backup_btn = forge_widgets::primary_button(
        "Backup now",
        Message::Settings(SettingsMsg::DbBackupRequested),
        palette,
    );
    let backup_hint = iced::widget::text("Creates a timestamped DB copy in the data directory.")
        .size(FONT_XS)
        .color(palette.text_faint);

    let storage_card = forge_widgets::card(
        [
            iced::widget::text("Storage & backups")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            path_label.into(),
            vacuum_btn,
            vacuum_hint.into(),
            backup_btn,
            backup_hint.into(),
        ],
        palette,
    );

    iced::widget::container(storage_card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn settings_queues_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let thread_hint = format!(
        "Tokio threadpool: {} worker(s) (auto-sized to system).",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    );
    let card = forge_widgets::card(
        [
            iced::widget::text("Queues & threading")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(thread_hint)
                .size(FONT_SM)
                .color(palette.text_muted)
                .into(),
            iced::widget::text(
                "Per-queue concurrency limits and blocking flags are managed on the Queues screen.",
            )
            .size(FONT_XS)
            .color(palette.text_faint)
            .into(),
        ],
        palette,
    );
    iced::widget::container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn settings_language_pane(palette: &ForgePalette) -> Element<'static, Message> {
    use iced::widget::{Space, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let header = row![
        tabler_icon(Icon::Globe, 18.0, p.brand),
        text("Language & region")
            .size(forge_widgets::tokens::FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let rows: [(&str, &str); 4] = [
        ("Interface language", "Ukrainian (uk-UA)"),
        ("Region", "Ukraine"),
        ("Date format", "DD.MM.YYYY"),
        ("First day of week", "Monday"),
    ];

    let mut list = column![].spacing(0);
    let count = rows.len();
    for (i, (label, value)) in rows.into_iter().enumerate() {
        let bottom = if i == count - 1 {
            0_u16
        } else {
            FORGE_SETTINGS_ROW_BORDER as u16
        };
        let _ = bottom;
        let row_el = container(
            row![
                text(label).size(FONT_SM).color(p.text_primary),
                Space::new().width(Length::Fill),
                container(text(value).size(FONT_SM).color(p.text_secondary).font(mono))
                    .padding([3_u16, 8_u16])
                    .style(move |_: &iced::Theme| container::Style {
                        background: Some(iced::Background::Color(p.surface_overlay)),
                        border: iced::Border {
                            radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                            ..Default::default()
                        },
                        ..container::Style::default()
                    }),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10_u16, 0_u16])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| {
            let border_color = if i + 1 == count {
                iced::Color::TRANSPARENT
            } else {
                p.border_regular
            };
            container::Style {
                border: iced::Border {
                    color: border_color,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }
        });
        list = list.push(row_el);
    }

    let body = column![header, list].spacing(18);

    iced::widget::container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .into()
}

fn settings_shortcuts_pane(palette: &ForgePalette) -> Element<'static, Message> {
    use iced::widget::{Space, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let header = row![
        tabler_icon(Icon::Keyboard, 18.0, p.brand),
        text("Shortcuts")
            .size(forge_widgets::tokens::FONT_LG)
            .color(p.text_primary),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let subtitle = text("Quick keys across Forge")
        .size(FONT_SM)
        .color(p.text_muted);

    let rows: [(&str, &str); 6] = [
        ("Save", "Ctrl + S"),
        ("New action", "Ctrl + N"),
        ("Quick switcher", "Ctrl + K"),
        ("Toggle Live Chat", "Ctrl + Shift + C"),
        ("Toggle Event Feed", "Ctrl + Shift + E"),
        ("Run script", "F5"),
    ];

    let mut list = column![].spacing(0);
    let count = rows.len();
    for (i, (label, key)) in rows.into_iter().enumerate() {
        let key_chip = container(text(key).size(FONT_XS).color(p.text_primary).font(mono))
            .padding([3_u16, 8_u16])
            .style(move |_: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(p.surface_overlay)),
                border: iced::Border {
                    radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                    ..Default::default()
                },
                ..container::Style::default()
            });

        let row_el = container(
            row![
                text(label).size(FONT_SM).color(p.text_primary),
                Space::new().width(Length::Fill),
                key_chip,
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([10_u16, 0_u16])
        .width(Length::Fill)
        .style(move |_: &iced::Theme| {
            let border_color = if i + 1 == count {
                iced::Color::TRANSPARENT
            } else {
                p.border_regular
            };
            container::Style {
                border: iced::Border {
                    color: border_color,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            }
        });
        list = list.push(row_el);
    }

    let note = container(
        text("Keyboard shortcuts not yet bound — labels only for now.")
            .size(FONT_XS)
            .color(p.text_faint)
            .font(mono),
    )
    .padding([8_u16, 0_u16]);

    let body = column![header, subtitle, list, note].spacing(14);

    iced::widget::container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20)
        .into()
}

const FORGE_SETTINGS_ROW_BORDER: f32 = 0.5;

fn settings_notifications_pane(palette: &ForgePalette) -> Element<'static, Message> {
    let card = forge_widgets::card(
        [
            iced::widget::text("Notifications")
                .size(FONT_SM)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(
                "Per-event-type toast customisation lands in beta-2. Errors and connection \
                 changes always surface in the status bar.",
            )
            .size(FONT_SM)
            .color(palette.text_muted)
            .into(),
        ],
        palette,
    );
    iced::widget::container(card)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

fn nav_group_header<'a>(label: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    iced::widget::text(label)
        .font(forge_widgets::tokens::font(
            forge_widgets::tokens::FontRole::Monospace,
        ))
        .size(FONT_XS)
        .color(palette.text_faint)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn platform_overview_card<'a>(
    letter: &'static str,
    color: iced::Color,
    name: &'a str,
    desc: &'a str,
    features: &'static [&'static str],
    connected: bool,
    target: IntegrationId,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::tokens::{Radius, radius};
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let p = *palette;

    let letter_box = container(text(letter).size(22.0).color(p.shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(44.0)
    .height(44.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_: &iced::Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius(Radius::Md).into(),
            color: iced::Color::TRANSPARENT,
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
        .spacing(5)
        .align_y(Alignment::Center),
    )
    .padding([2_u16, 7_u16])
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
    .spacing(8)
    .align_y(Alignment::Center);

    let desc_text = text(desc.to_owned()).size(FONT_SM).color(p.text_muted);

    let mut chip_row = iced::widget::Row::new().spacing(4);
    for f in features {
        let chip = container(text(*f).size(FONT_XS).color(p.text_secondary))
            .padding([2_u16, 7_u16])
            .style(move |_: &iced::Theme| container::Style {
                background: Some(Background::Color(p.shell)),
                border: Border {
                    radius: radius(Radius::Sm).into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });
        chip_row = chip_row.push(chip);
    }

    let info_col = column![title_row, desc_text, chip_row.wrap()].spacing(6);

    let inner = row![
        letter_box,
        container(info_col).width(Length::Fill),
        tabler_icon(Icon::ChevronRight, 16.0, p.text_faint),
    ]
    .spacing(14)
    .align_y(Alignment::Start);

    button(inner)
        .padding([16_u16, 18_u16])
        .width(Length::Fill)
        .on_press(Message::Navigate(Screen::IntegrationDetail(target)))
        .style(
            move |_: &iced::Theme, status: iced::widget::button::Status| {
                let hovered = matches!(
                    status,
                    iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
                );
                iced::widget::button::Style {
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
                    shadow: iced::Shadow::default(),
                    snap: false,
                }
            },
        )
        .into()
}

fn platforms_overview_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};
    use iced::{Length, Padding};

    let p = *palette;

    let title = text("Streaming platforms")
        .size(FONT_MD)
        .color(p.text_primary);
    let subtitle = text("Connect once, Forge listens to all chats and events in one place.")
        .size(FONT_SM)
        .color(p.text_muted);
    let header = column![title, subtitle].spacing(4);

    let twitch_connected = app.rt.twitch_chat_handle.is_some();

    let twitch_card = platform_overview_card(
        "T",
        p.brand,
        "Twitch",
        "Chat, EventSub subscriptions, channel points, bits, raids",
        &["IRC chat", "EventSub", "Channel points", "Bits & subs"],
        twitch_connected,
        IntegrationId::new("twitch"),
        palette,
    );
    let youtube_card = platform_overview_card(
        "Y",
        p.random,
        "YouTube",
        "Live chat, super chats, channel memberships, subscribers",
        &["Live chat", "Super chat", "Memberships"],
        false,
        IntegrationId::new("youtube"),
        palette,
    );
    let kick_card = platform_overview_card(
        "K",
        p.info,
        "Kick",
        "Chat, channel events, subscribers — newer streaming platform",
        &["Chat", "Subs", "Channel events"],
        false,
        IntegrationId::new("kick"),
        palette,
    );
    let trovo_card = platform_overview_card(
        "V",
        p.success,
        "Trovo",
        "Chat, spells, mana, subscribers — Tencent streaming platform",
        &["Chat", "Spells", "Subs"],
        false,
        IntegrationId::new("trovo"),
        palette,
    );

    let grid_row_1 = row![twitch_card, youtube_card]
        .spacing(12)
        .width(Length::Fill);
    let grid_row_2 = row![kick_card, trovo_card].spacing(12).width(Length::Fill);
    let grid = column![grid_row_1, grid_row_2].spacing(12);

    let body = column![header, grid].spacing(18);
    let page_header = simple_page_header(&[("Platforms", true)], palette);
    let body_container = container(scrollable(body).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 22.0,
            right: 28.0,
            bottom: 22.0,
            left: 28.0,
        });

    column![page_header, body_container]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn settings_view<'a>(
    section: &'a SettingsSection,
    ws: &'a crate::settings_websocket::SettingsWebSocketState,
    server: &'a ServerScreenState,
    audio: &'a SettingsAudioState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let nav = iced::widget::column![
        nav_group_header("PREFERENCES", palette),
        settings_section_button("Appearance", SettingsSection::Appearance, section, palette),
        settings_section_button("Language", SettingsSection::Language, section, palette),
        settings_section_button("Shortcuts", SettingsSection::Shortcuts, section, palette),
        settings_section_button(
            "Notifications",
            SettingsSection::Notifications,
            section,
            palette,
        ),
        iced::widget::Space::new().height(6),
        nav_group_header("ENGINE", palette),
        settings_section_button("Audio", SettingsSection::Audio, section, palette),
        settings_section_button("Scripting", SettingsSection::Scripting, section, palette),
        settings_section_button("Queues", SettingsSection::Queues, section, palette),
        settings_section_button("Storage", SettingsSection::Storage, section, palette),
        settings_section_button("WebSocket", SettingsSection::WebSocket, section, palette),
        iced::widget::Space::new().height(6),
        nav_group_header("ABOUT", palette),
        settings_section_button("Version", SettingsSection::Version, section, palette),
        settings_section_button(
            "Diagnostics",
            SettingsSection::Diagnostics,
            section,
            palette,
        ),
    ]
    .spacing(2)
    .padding([12_u16, 8_u16])
    .width(Length::Fixed(200.0));

    let nav_container = iced::widget::container(nav)
        .height(Length::Fill)
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.shell)),
            border: iced::Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let pane: Element<'a, Message> = match section {
        SettingsSection::Diagnostics => settings_diagnostics_pane(palette),
        SettingsSection::Audio => settings_audio_view(audio, palette),
        SettingsSection::WebSocket => {
            settings_websocket_view(ws, &server.bearer_token, server.token_revealed, palette)
        }
        SettingsSection::Storage => settings_storage_pane(palette),
        SettingsSection::Queues => settings_queues_pane(palette),
        SettingsSection::Notifications => settings_notifications_pane(palette),
        SettingsSection::Language => settings_language_pane(palette),
        SettingsSection::Shortcuts => settings_shortcuts_pane(palette),
        other => {
            let label = format!("Settings · {other:?}");
            iced::widget::container(forge_widgets::empty_state(
                label,
                "Coming with alpha-N.",
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    let page_header = simple_page_header(&[("Settings", true)], palette);
    let body = iced::widget::row![nav_container, pane].spacing(0);

    iced::widget::column![page_header, body]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn actions_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let actions_state = &app.actions;

    let total = actions_state.total_actions();
    let visible = actions_state.visible_actions();

    let page_header = actions_page_header(actions_state, palette);

    let mut tree_col: iced::widget::Column<'_, Message> = column![].spacing(0);

    if actions_state.loading {
        tree_col = tree_col.push(
            container(text("Loading...").size(FONT_XS).color(p.text_muted))
                .padding([16, 14])
                .width(Length::Fill),
        );
    } else if total == 0 {
        tree_col = tree_col.push(
            container(text("No actions yet").size(FONT_XS).color(p.text_faint))
                .padding([16, 14])
                .width(Length::Fill),
        );
    } else {
        for group in &actions_state.tree {
            let filtered: Vec<_> = group
                .actions
                .iter()
                .filter(|a| actions_state.action_passes_filter(a))
                .collect();

            if filtered.is_empty() {
                continue;
            }

            let is_collapsed = actions_state.collapsed_groups.contains(&group.category);
            tree_col = tree_col.push(actions_group_header(group, is_collapsed, palette));

            if !is_collapsed {
                for summary in &filtered {
                    let selected = actions_state.selected == Some(summary.id);
                    let menu_open = actions_state.action_menu_open == Some(summary.id);
                    let rename_buf = actions_state
                        .renaming_action
                        .as_ref()
                        .filter(|(id, _)| *id == summary.id)
                        .map(|(_, n)| n.as_str());
                    tree_col = tree_col.push(actions_tree_row(
                        summary, selected, menu_open, rename_buf, palette,
                    ));
                }
            }
        }
    }

    let tree_scrollable = scrollable(tree_col).height(Length::Fill);

    let left_panel = container(tree_scrollable)
        .width(Length::Fixed(290.0))
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let right_panel = actions_detail_panel(actions_state, palette);

    let footer = actions_footer(visible, total, palette);

    let body = row![left_panel, right_panel]
        .spacing(0)
        .height(Length::Fill);

    let body_and_footer: Element<'_, Message> = container(column![body, footer].spacing(0))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    let main_view: Element<'_, Message> = if let Some(open_id) = actions_state.action_menu_open {
        let open_summary = actions_state
            .tree
            .iter()
            .flat_map(|g| g.actions.iter())
            .find(|a| a.id == open_id);
        let menu_top_offset = if open_summary.is_some() {
            compute_action_menu_y_offset(actions_state, open_id)
        } else {
            None
        };
        if let (Some(summary), Some(top_y)) = (open_summary, menu_top_offset) {
            let toggle_label = if summary.enabled { "Disable" } else { "Enable" };
            let menu_items: Vec<forge_widgets::MenuItem<Message>> = vec![
                forge_widgets::MenuItem::Item {
                    label: "Rename\u{2026}".into(),
                    icon: Some(Icon::InfoCircle),
                    on_press: Message::Actions(ActionsMsg::RenameStarted(open_id)),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Item {
                    label: "Duplicate".into(),
                    icon: Some(Icon::Copy),
                    on_press: Message::Actions(ActionsMsg::DuplicateAction(open_id)),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Item {
                    label: toggle_label.to_owned(),
                    icon: Some(Icon::Bolt),
                    on_press: Message::Actions(ActionsMsg::ToggleEnabled(
                        open_id,
                        !summary.enabled,
                    )),
                    shortcut: None,
                    color: None,
                    disabled: false,
                },
                forge_widgets::MenuItem::Divider,
                forge_widgets::MenuItem::Item {
                    label: "Delete\u{2026}".into(),
                    icon: Some(Icon::Eraser),
                    on_press: Message::Actions(ActionsMsg::DeleteAction(open_id)),
                    shortcut: None,
                    color: Some(p.random),
                    disabled: false,
                },
            ];
            let panel = forge_widgets::menu_panel(menu_items, palette);
            let overlay = container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(iced::Padding {
                    top: top_y,
                    right: 0.0,
                    bottom: 0.0,
                    left: 90.0,
                })
                .align_x(iced::Alignment::Start)
                .align_y(iced::Alignment::Start);
            iced::widget::stack![body_and_footer, overlay].into()
        } else {
            body_and_footer
        }
    } else {
        body_and_footer
    };

    let main_view: Element<'_, Message> =
        if let Some(form) = app.actions.add_sub_action_modal.as_ref() {
            let modal_el = add_sub_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.actions.add_trigger_modal.as_ref() {
            let modal_el = add_trigger_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.actions.add_action_modal.as_ref() {
            let modal_el = add_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else {
            main_view
        };

    iced::widget::column![page_header, main_view]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn actions_page_header<'a>(
    state: &'a crate::actions::ActionsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    let p = *palette;

    let crumb_chevron = tabler_icon(Icon::ChevronRight, 11.0, p.text_faint);
    let crumb_chevron_2 = tabler_icon(Icon::ChevronRight, 11.0, p.text_faint);
    let crumbs_left = row![
        tabler_icon(Icon::Home, 13.0, p.text_faint),
        crumb_chevron,
        text("Automation").size(FONT_SM).color(p.text_muted),
        crumb_chevron_2,
        text("Actions").size(FONT_SM).color(p.text_primary),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);

    let chip_all = forge_widgets::filter_chip(
        palette,
        "All",
        p.brand,
        state.filter == ActionsFilter::All,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::All)),
    );
    let chip_chat = forge_widgets::filter_chip(
        palette,
        "Chat",
        p.info,
        state.filter == ActionsFilter::Chat,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Chat)),
    );
    let chip_timers = forge_widgets::filter_chip(
        palette,
        "Timers",
        p.warning,
        state.filter == ActionsFilter::Timers,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Timers)),
    );
    let chip_points = forge_widgets::filter_chip(
        palette,
        "Points",
        p.accent_pink_light,
        state.filter == ActionsFilter::Points,
        Message::Actions(ActionsMsg::FilterChanged(ActionsFilter::Points)),
    );
    let chips = row![chip_all, chip_chat, chip_timers, chip_points].spacing(4);

    let divider = container(iced::widget::Space::new().width(0.5).height(16.0))
        .width(0.5)
        .height(16.0)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.border_regular)),
            ..iced::widget::container::Style::default()
        });

    let search = forge_widgets::search_input(
        "Search actions...",
        &state.search,
        |q| Message::Actions(ActionsMsg::SearchChanged(q)),
        palette,
    );

    let new_btn = forge_widgets::primary_button_small(
        "+ New action",
        Message::Actions(ActionsMsg::OpenAddActionModal),
        palette,
    );

    let right_side = row![
        chips,
        divider,
        container(search).width(Length::Fixed(180.0)),
        new_btn,
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);

    let inner = row![
        crumbs_left,
        iced::widget::Space::new().width(Length::Fill),
        right_side,
    ]
    .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([10_u16, 16_u16])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn actions_group_header<'a>(
    group: &'a crate::actions::ActionsGroup,
    collapsed: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text};

    let p = *palette;
    let chevron_icon = if collapsed {
        Icon::ChevronRight
    } else {
        Icon::ChevronDown
    };
    let chevron_el = tabler_icon(chevron_icon, 11.0, p.text_faint);

    let cat_el = text(group.category.display_name())
        .size(FONT_XS)
        .color(p.text_muted)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let count_el = text(group.actions.len().to_string())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let inner = row![
        chevron_el,
        cat_el,
        iced::widget::Space::new().width(Length::Fill),
        count_el,
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);

    let cat = group.category.clone();

    button(container(inner).width(Length::Fill).padding([8, 16]))
        .on_press(Message::Actions(ActionsMsg::ToggleGroupCollapsed(cat)))
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg_color = match status {
                iced::widget::button::Status::Hovered => p.elevated,
                _ => p.shell,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: p.text_muted,
                border: iced::Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn actions_tree_row<'a>(
    summary: &'a crate::actions::ActionSummary,
    selected: bool,
    menu_open: bool,
    rename_buf: Option<&'a str>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::menu_button_trigger;
    use iced::widget::{button, container, row, text, text_input};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);
    let action_id = summary.id;

    let state_icon = if summary.enabled {
        tabler_icon(Icon::CircleCheckFilled, 13.0, p.success)
    } else {
        tabler_icon(Icon::Circle, 13.0, p.text_faint)
    };

    let name_color = if !summary.enabled {
        p.text_faint
    } else if selected {
        p.text_primary
    } else {
        p.text_secondary
    };

    let name_el: Element<'a, Message> = if let Some(buf) = rename_buf {
        text_input("", buf)
            .id(action_rename_input_id())
            .on_input(|s| Message::Actions(ActionsMsg::RenameBufferChanged(s)))
            .on_submit(Message::Actions(ActionsMsg::RenameSubmit))
            .size(FONT_SM)
            .padding(iced::Padding {
                top: 2.0,
                bottom: 2.0,
                left: 6.0,
                right: 6.0,
            })
            .width(Length::Fill)
            .style(
                move |_t: &iced::Theme, _s| iced::widget::text_input::Style {
                    background: iced::Background::Color(p.shell),
                    border: iced::Border {
                        color: p.brand,
                        width: 0.5,
                        radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                    },
                    icon: p.text_muted,
                    placeholder: p.text_muted,
                    value: p.text_primary,
                    selection: iced::Color { a: 0.25, ..p.brand },
                },
            )
            .into()
    } else {
        container(
            text(&summary.name)
                .size(FONT_SM)
                .color(name_color)
                .font(mono)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true)
        .into()
    };

    let count_el = text(summary.sub_action_count.to_string())
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let stripe_color = if selected {
        p.brand
    } else {
        iced::Color::TRANSPARENT
    };
    let stripe = container(iced::widget::Space::new().width(2.0).height(Length::Fill))
        .width(2.0)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(stripe_color)),
            ..iced::widget::container::Style::default()
        });

    let select_btn = button(
        row![state_icon, name_el, count_el,]
            .spacing(8)
            .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(Message::Actions(ActionsMsg::ActionSelected(action_id)))
    .padding(iced::Padding {
        top: 6.0,
        bottom: 6.0,
        left: 32.0,
        right: 8.0,
    })
    .width(Length::Fill)
    .style(move |_theme: &iced::Theme, status| {
        let bg_color = match (selected, status) {
            (true, _) | (false, iced::widget::button::Status::Hovered) => p.base,
            _ => iced::Color::TRANSPARENT,
        };
        iced::widget::button::Style {
            background: Some(iced::Background::Color(bg_color)),
            text_color: name_color,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
            snap: false,
        }
    });

    let menu_btn = menu_button_trigger(
        Icon::DotsVertical,
        menu_open,
        Message::Actions(ActionsMsg::ToggleActionMenu(action_id)),
        palette,
    );

    let right_col = container(menu_btn)
        .padding(iced::Padding {
            top: 2.0,
            bottom: 2.0,
            left: 0.0,
            right: 6.0,
        })
        .align_y(iced::Alignment::Center);

    row![stripe, select_btn, right_col]
        .spacing(0)
        .width(Length::Fill)
        .align_y(iced::Alignment::Center)
        .into()
}

fn actions_detail_panel<'a>(
    state: &'a crate::actions::ActionsState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;

    if state.selected.is_none() {
        return container(forge_widgets::empty_state(
            "No action selected",
            "Select an action from the list to view its details.",
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into();
    }

    let Some(detail) = &state.detail else {
        return container(text("Loading...").size(FONT_XS).color(p.text_muted))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([24, 24])
            .into();
    };

    let action = &detail.action;

    let enabled_dot_color = if action.enabled {
        p.success
    } else {
        p.text_faint
    };
    let enabled_dot_size = 5.0_f32;
    let enabled_dot = container(
        iced::widget::Space::new()
            .width(enabled_dot_size)
            .height(enabled_dot_size),
    )
    .width(enabled_dot_size)
    .height(enabled_dot_size)
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(enabled_dot_color)),
        border: iced::Border {
            radius: (enabled_dot_size / 2.0).into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let status_label = if action.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let status_badge = container(
        row![
            enabled_dot,
            text(status_label).size(FONT_XS).color(enabled_dot_color)
        ]
        .spacing(4)
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([3, 8])
    .style(move |_theme: &iced::Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(p.shell)),
        border: iced::Border {
            color: p.border_regular,
            width: 0.5,
            radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
        },
        ..iced::widget::container::Style::default()
    });

    let name_el = text(&action.name)
        .size(FONT_LG)
        .color(p.text_primary)
        .font(iced::Font {
            weight: iced::font::Weight::Medium,
            ..iced::Font::DEFAULT
        });

    let name_row = row![name_el, status_badge]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center);

    let test_btn = forge_widgets::ghost_button_with_icon(
        Icon::PlayerPlay,
        "Test run",
        Message::Actions(ActionsMsg::TestTrigger(action.id)),
        palette,
    );
    let dup_btn = forge_widgets::ghost_button_with_icon(
        Icon::Copy,
        "Duplicate",
        Message::Actions(ActionsMsg::DuplicateAction(action.id)),
        palette,
    );
    let action_btns = row![test_btn, dup_btn].spacing(6);

    let header_row = row![container(name_row).width(Length::Fill), action_btns,]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Top);

    let mut detail_col = column![header_row].spacing(0);

    if let Some(desc) = &action.description {
        let desc_el = text(desc.as_str()).size(FONT_XS).color(p.text_muted);
        detail_col = detail_col.push(container(desc_el).padding([4, 0]));
    }

    detail_col = detail_col.push(iced::widget::Space::new().height(14.0));

    if state.telemetry_loading {
        let placeholder =
            crate::actions::telemetry_grid(&forge_storage::ActionTelemetry::default(), palette);
        detail_col = detail_col.push(placeholder);
        detail_col = detail_col.push(iced::widget::Space::new().height(18.0));
    } else if let Some(tel) = &state.telemetry {
        let grid = crate::actions::telemetry_grid(tel, palette);
        detail_col = detail_col.push(grid);
        detail_col = detail_col.push(iced::widget::Space::new().height(18.0));
    }

    detail_col = detail_col.push(section_header_with_add(
        &format!("TRIGGERS \u{00b7} {}", detail.triggers.len()),
        "Add trigger",
        p.warning,
        Message::Actions(ActionsMsg::OpenAddTriggerModal(action.id)),
        palette,
    ));
    detail_col = detail_col.push(iced::widget::Space::new().height(8.0));

    if detail.triggers.is_empty() {
        detail_col = detail_col.push(empty_placeholder_card(
            Icon::Bolt,
            p.warning,
            "No triggers \u{2014} this action will never fire on its own",
            palette,
        ));
    } else {
        for trigger in &detail.triggers {
            let kind_str = crate::actions::trigger_label_of(&trigger.kind);
            let trigger_row = container(
                row![
                    tabler_icon(Icon::Bolt, FONT_SM, p.brand),
                    text(kind_str).size(FONT_SM).color(p.text_secondary),
                ]
                .spacing(8)
                .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .padding([18, 12])
            .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(p.shell)),
                border: iced::Border {
                    color: p.border_input,
                    width: 0.5,
                    radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                },
                ..iced::widget::container::Style::default()
            });
            detail_col = detail_col.push(trigger_row);
            detail_col = detail_col.push(iced::widget::Space::new().height(6.0));
        }
    }
    detail_col = detail_col.push(iced::widget::Space::new().height(14.0));

    detail_col = detail_col.push(section_header_with_add(
        &format!("SUB-ACTIONS \u{00b7} {}", action.sub_actions.len()),
        "Add sub-action",
        p.brand,
        Message::AddSubAction(AddSubActionMsg::OpenRequested(action.id)),
        palette,
    ));
    detail_col = detail_col.push(iced::widget::Space::new().height(8.0));

    if action.sub_actions.is_empty() {
        detail_col = detail_col.push(empty_placeholder_card(
            Icon::Plus,
            p.brand,
            "No steps yet \u{2014} add one",
            palette,
        ));
    } else {
        for (i, spec) in action.sub_actions.iter().enumerate() {
            let step_label = format!("{}. {}", i + 1, spec.kind_label());
            let step_row = container(text(step_label).size(FONT_SM).color(p.text_secondary))
                .width(Length::Fill)
                .padding([18, 12])
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(p.shell)),
                    border: iced::Border {
                        color: p.border_input,
                        width: 0.5,
                        radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                    },
                    ..iced::widget::container::Style::default()
                });
            detail_col = detail_col.push(step_row);
            detail_col = detail_col.push(iced::widget::Space::new().height(6.0));
        }
    }

    container(scrollable(container(detail_col).padding([18, 24])).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn section_header_with_add<'a>(
    label: &str,
    add_label: &'static str,
    add_color: iced::Color,
    on_add: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text};
    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let label_el = text(label.to_owned())
        .size(FONT_XS)
        .color(p.text_muted)
        .font(mono);

    let add_btn = button(
        row![
            tabler_icon(Icon::Plus, 11.0, add_color),
            text(add_label).size(FONT_XS).color(add_color),
        ]
        .spacing(4)
        .align_y(iced::alignment::Vertical::Center),
    )
    .on_press(on_add)
    .padding([2_u16, 4_u16])
    .style(
        move |_theme: &iced::Theme, _status| iced::widget::button::Style {
            background: None,
            border: iced::Border::default(),
            text_color: iced::Color::TRANSPARENT,
            shadow: iced::Shadow::default(),
            snap: false,
        },
    );

    container(
        row![
            label_el,
            iced::widget::Space::new().width(Length::Fill),
            add_btn,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([6_u16, 14_u16])
    .width(Length::Fill)
    .into()
}

fn action_rename_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("forge:action_rename")
}

pub(crate) fn sheet_chrome<'a>(
    title: &'a str,
    on_close: Message,
    body: Element<'a, Message>,
    footer: Option<Element<'a, Message>>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    let p = *palette;

    let title_el = text(title)
        .size(forge_widgets::tokens::FONT_MD)
        .color(p.text_primary);

    let close_btn = button(tabler_icon(Icon::X, 14.0, p.text_muted))
        .on_press(on_close)
        .padding(6)
        .style(move |_t: &iced::Theme, status| {
            let bg = match status {
                iced::widget::button::Status::Hovered => {
                    Some(iced::Background::Color(p.surface_overlay))
                }
                _ => None,
            };
            iced::widget::button::Style {
                background: bg,
                border: iced::Border {
                    radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                    ..Default::default()
                },
                text_color: iced::Color::TRANSPARENT,
                shadow: iced::Shadow::default(),
                snap: false,
            }
        });

    let header = container(
        row![
            title_el,
            iced::widget::Space::new().width(Length::Fill),
            close_btn,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .padding([12_u16, 16_u16])
    .width(Length::Fill)
    .style(move |_t: &iced::Theme| container::Style {
        border: iced::Border {
            color: p.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    });

    let body_wrap = container(body).width(Length::Fill).height(Length::Fill);

    let mut col = column![header, body_wrap]
        .width(Length::Fill)
        .height(Length::Fill);

    if let Some(footer_el) = footer {
        let footer_container = container(footer_el)
            .padding([12_u16, 16_u16])
            .width(Length::Fill)
            .style(move |_t: &iced::Theme| container::Style {
                border: iced::Border {
                    color: p.border_regular,
                    width: 0.5,
                    radius: 0.0.into(),
                },
                ..container::Style::default()
            });
        col = col.push(footer_container);
    }

    col.into()
}

fn compute_action_menu_y_offset(
    state: &crate::actions::ActionsState,
    open_id: forge_types::ActionId,
) -> Option<f32> {
    const PAGE_HEADER_H: f32 = 40.0;
    const GROUP_HEADER_H: f32 = 28.0;
    const ROW_H: f32 = 30.0;

    let mut y = PAGE_HEADER_H;
    for group in &state.tree {
        let visible: Vec<&crate::actions::ActionSummary> = group
            .actions
            .iter()
            .filter(|a| state.action_passes_filter(a))
            .collect();
        if visible.is_empty() {
            continue;
        }
        y += GROUP_HEADER_H;
        if state.collapsed_groups.contains(&group.category) {
            continue;
        }
        for action in visible {
            if action.id == open_id {
                return Some(y + ROW_H);
            }
            y += ROW_H;
        }
    }
    None
}

fn empty_placeholder_card<'a>(
    icon: Icon,
    icon_color: iced::Color,
    label: &'static str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{column, container, text};
    let p = *palette;

    let inner = column![
        tabler_icon(icon, 16.0, icon_color),
        text(label).size(FONT_XS).color(p.text_muted),
    ]
    .spacing(6)
    .align_x(iced::Alignment::Center);

    container(inner)
        .padding([18_u16, 12_u16])
        .width(Length::Fill)
        .align_x(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: None,
            border: iced::Border {
                color: p.border_input,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn actions_footer<'a>(
    visible: usize,
    total: usize,
    palette: &'a ForgePalette,
) -> iced::widget::Container<'a, Message> {
    use iced::widget::{container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let left_str = format!(
        "Showing {} of {} \u{00b7} grouped by trigger",
        visible, total
    );
    let left_el = text(left_str).size(FONT_XS).color(p.text_faint).font(mono);

    let storage_el = text("Storage: \u{2014}")
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let dot_size = 6.0_f32;
    let green_dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.success)),
            border: iced::Border {
                radius: (dot_size / 2.0).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let saved_el = text("Auto-saved just now")
        .size(FONT_XS)
        .color(p.text_faint)
        .font(mono);

    let right = row![storage_el, green_dot, saved_el]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let inner = row![container(left_el).width(Length::Fill), right,]
        .spacing(0)
        .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([7, 16])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.shell)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
}

fn add_action_modal_view<'a>(
    form: &'a AddActionForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::{BannerKind, ModalProps, ToggleProps};
    use iced::widget::{column, row, text};

    let name_count = format!("{}/64", form.name.len().min(64));
    let name_counter = text(name_count)
        .size(FONT_XS)
        .color(palette.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let name_input = forge_widgets::text_input_field(
        "My automation",
        &form.name,
        |v| Message::AddAction(AddActionMsg::NameChanged(v)),
        palette,
    );

    let name_row = row![name_input, name_counter]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let name_block = column![
        forge_widgets::section_header("NAME", None, palette),
        name_row,
    ]
    .spacing(6);

    let group_input = forge_widgets::text_input_field(
        "Examples",
        &form.group,
        |v| Message::AddAction(AddActionMsg::GroupChanged(v)),
        palette,
    );

    let group_block = column![
        forge_widgets::section_header("GROUP", None, palette),
        group_input,
    ]
    .spacing(6);

    let queue_names: Vec<String> = form.queue_options.iter().map(|(_, n)| n.clone()).collect();
    let p = *palette;
    let queue_select: Element<'_, Message> = iced::widget::pick_list(
        queue_names,
        form.selected_queue_name.clone(),
        |name: String| Message::AddAction(AddActionMsg::QueueSelected(name)),
    )
    .padding(forge_widgets::inputs::input_padding())
    .width(Length::Fill)
    .style(move |_theme, status| {
        use iced::widget::pick_list;
        let border_color = match status {
            pick_list::Status::Opened { .. } => p.border_active,
            _ => p.border_regular,
        };
        pick_list::Style {
            text_color: p.text_primary,
            placeholder_color: p.text_muted,
            handle_color: p.text_muted,
            background: iced::Background::Color(p.shell),
            border: iced::Border {
                color: border_color,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
        }
    })
    .into();

    let queue_block = column![
        forge_widgets::section_header("QUEUE", None, palette),
        queue_select,
    ]
    .spacing(6);

    let two_col = row![group_block, queue_block].spacing(12);

    let desc_input = forge_widgets::text_input_field(
        "Plays a sound, shows overlay alert...",
        &form.description,
        |v| Message::AddAction(AddActionMsg::DescriptionChanged(v)),
        palette,
    );

    let desc_block = column![
        forge_widgets::section_header("DESCRIPTION", None, palette),
        desc_input,
    ]
    .spacing(6);

    let enabled_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Enabled",
            description: "Action runs when a trigger fires.",
            value: form.enabled,
            on_toggle: Message::AddAction(AddActionMsg::EnabledToggled(!form.enabled)),
        },
    );

    let concurrent_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Concurrent execution",
            description: "Allow parallel runs in this queue.",
            value: form.concurrent,
            on_toggle: Message::AddAction(AddActionMsg::ConcurrentToggled(!form.concurrent)),
        },
    );

    let bypass_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Bypass queue pause",
            description: "Always run even if queue is paused.",
            value: form.bypass_pause,
            on_toggle: Message::AddAction(AddActionMsg::BypassPauseToggled(!form.bypass_pause)),
        },
    );

    let random_pick_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Random pick",
            description: "Run ONE random sub-action per trigger instead of all.",
            value: form.random_pick,
            on_toggle: Message::AddAction(AddActionMsg::RandomPickToggled(!form.random_pick)),
        },
    );

    let behavior_header = forge_widgets::section_header("BEHAVIOR", None, palette);

    let mut body_col = column![
        name_block,
        two_col,
        desc_block,
        behavior_header,
        enabled_toggle,
        concurrent_toggle,
        bypass_toggle,
        random_pick_toggle,
    ]
    .spacing(14);

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::AddAction(AddActionMsg::Cancel),
        palette,
    );

    let create_on_press = Message::AddAction(AddActionMsg::Submit);
    let create_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Create action", create_on_press, palette)
    } else {
        forge_widgets::secondary_button("Create action", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, create_btn].spacing(8);

    let footer: Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .into();

    forge_widgets::modal(
        palette,
        ModalProps {
            title: "New action",
            on_close: Message::AddAction(AddActionMsg::Cancel),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
    )
}

fn add_trigger_modal_view<'a>(
    form: &'a AddTriggerForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BannerKind;
    use iced::widget::{column, row, scrollable, text};
    use iced::{Alignment, Background, Length};

    let search_input = forge_widgets::search_input(
        "Filter trigger types...",
        &form.search,
        |v| Message::AddTrigger(AddTriggerMsg::SearchChanged(v)),
        palette,
    );

    let chip_all = forge_widgets::category_chip(
        palette,
        "All",
        palette.brand,
        form.category == TriggerCategory::All,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(TriggerCategory::All)),
    );
    let chip_chat = forge_widgets::category_chip(
        palette,
        "Chat",
        palette.brand,
        form.category == TriggerCategory::Chat,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(TriggerCategory::Chat)),
    );
    let chip_subs = forge_widgets::category_chip(
        palette,
        "Subscriptions",
        palette.brand,
        form.category == TriggerCategory::Subscriptions,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(
            TriggerCategory::Subscriptions,
        )),
    );
    let chip_bits = forge_widgets::category_chip(
        palette,
        "Bits",
        palette.bits,
        form.category == TriggerCategory::Bits,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(TriggerCategory::Bits)),
    );
    let chip_raids = forge_widgets::category_chip(
        palette,
        "Raids",
        palette.random,
        form.category == TriggerCategory::Raids,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(TriggerCategory::Raids)),
    );
    let chip_obs = forge_widgets::category_chip(
        palette,
        "OBS",
        palette.brand,
        form.category == TriggerCategory::Obs,
        Message::AddTrigger(AddTriggerMsg::CategorySelected(TriggerCategory::Obs)),
    );

    let chips_row = row![
        chip_all, chip_chat, chip_subs, chip_bits, chip_raids, chip_obs
    ]
    .spacing(6);

    let visible = form.visible_kinds();
    let is_empty = visible.is_empty();
    let mut grid_col = column![].spacing(6);
    for kind in visible {
        let selected = form.selected_kind.as_ref() == Some(&kind);
        let lbl = kind_label(&kind);
        let summ = kind_summary(&kind);
        let card = trigger_picker_card(
            lbl,
            summ,
            selected,
            palette,
            Message::AddTrigger(AddTriggerMsg::KindSelected(kind)),
        );
        grid_col = grid_col.push(card);
    }

    if form.selected_kind.is_none() && is_empty {
        grid_col = grid_col.push(
            text("No trigger types match your filter.")
                .size(FONT_SM)
                .color(palette.text_faint),
        );
    }

    let mut config_col = column![].spacing(10);

    if let Some(kind) = &form.selected_kind {
        match kind {
            forge_types::TriggerKind::TwitchChatCommand => {
                let cmd_input = forge_widgets::text_input_field(
                    "!quote",
                    &form.config.command_name,
                    |v| Message::AddTrigger(AddTriggerMsg::CommandNameChanged(v)),
                    palette,
                );
                let cmd_block = column![
                    forge_widgets::section_header("COMMAND NAME", None, palette),
                    cmd_input,
                ]
                .spacing(6);

                let cooldown_input = forge_widgets::text_input_field(
                    "0",
                    &form.config.cooldown_secs,
                    |v| Message::AddTrigger(AddTriggerMsg::CooldownChanged(v)),
                    palette,
                );
                let cooldown_block = column![
                    forge_widgets::section_header("COOLDOWN (SECS)", None, palette),
                    cooldown_input,
                ]
                .spacing(6);

                let p = *palette;
                let perm_options: Vec<String> = vec![
                    "Everyone".to_string(),
                    "Subscriber".to_string(),
                    "Vip".to_string(),
                    "Moderator".to_string(),
                    "Broadcaster".to_string(),
                ];
                let selected_perm = permission_label(&form.config.permission).to_string();
                let perm_select: Element<'_, Message> =
                    iced::widget::pick_list(perm_options, Some(selected_perm), |name: String| {
                        Message::AddTrigger(AddTriggerMsg::PermissionSelected(
                            permission_from_label(&name),
                        ))
                    })
                    .padding(forge_widgets::inputs::input_padding())
                    .width(Length::Fill)
                    .style(move |_theme, status| {
                        use iced::widget::pick_list;
                        let border_color = match status {
                            pick_list::Status::Opened { .. } => p.border_active,
                            _ => p.border_regular,
                        };
                        pick_list::Style {
                            text_color: p.text_primary,
                            placeholder_color: p.text_muted,
                            handle_color: p.text_muted,
                            background: iced::Background::Color(p.shell),
                            border: iced::Border {
                                color: border_color,
                                width: 0.5,
                                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                            },
                        }
                    })
                    .into();

                let perm_block = column![
                    forge_widgets::section_header("PERMISSION", None, palette),
                    perm_select,
                ]
                .spacing(6);

                config_col = config_col
                    .push(cmd_block)
                    .push(cooldown_block)
                    .push(perm_block);
            }
            forge_types::TriggerKind::TwitchCheer => {
                let bits_input = forge_widgets::text_input_field(
                    "1",
                    &form.config.min_bits,
                    |v| Message::AddTrigger(AddTriggerMsg::MinBitsChanged(v)),
                    palette,
                );
                let bits_block = column![
                    forge_widgets::section_header("MINIMUM BITS", None, palette),
                    bits_input,
                ]
                .spacing(6);
                config_col = config_col.push(bits_block);
            }
            _ => {
                config_col = config_col.push(
                    text("No configuration required for this trigger type.")
                        .size(FONT_XS)
                        .color(palette.text_muted),
                );
            }
        }
    }

    let mut body_col =
        column![search_input, chips_row, scrollable(grid_col).height(200),].spacing(10);

    if form.selected_kind.is_some() {
        body_col = body_col.push(
            iced::widget::container(column![].spacing(0))
                .width(Length::Fill)
                .height(0.5)
                .style(move |_theme: &iced::Theme| iced::widget::container::Style {
                    background: Some(Background::Color(palette.border_regular)),
                    ..iced::widget::container::Style::default()
                }),
        );
        body_col = body_col.push(forge_widgets::section_header("CONFIGURE", None, palette));
        body_col = body_col.push(config_col);
    }

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::AddTrigger(AddTriggerMsg::Cancel),
        palette,
    );

    let save_on_press = Message::AddTrigger(AddTriggerMsg::Submit);
    let save_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Add trigger", save_on_press, palette)
    } else {
        forge_widgets::secondary_button("Add trigger", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, save_btn].spacing(8);

    let footer: Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into();

    let panel = sheet_chrome(
        "Add trigger",
        Message::AddTrigger(AddTriggerMsg::Cancel),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::AddTrigger(AddTriggerMsg::Cancel),
        forge_widgets::SheetEdge::Right,
        480.0,
        palette,
    )
}

fn log_level_label(level: &forge_types::LogLevel) -> &'static str {
    match level {
        forge_types::LogLevel::Trace => "Trace",
        forge_types::LogLevel::Debug => "Debug",
        forge_types::LogLevel::Info => "Info",
        forge_types::LogLevel::Warn => "Warn",
        forge_types::LogLevel::Error => "Error",
    }
}

fn log_level_from_label(label: &str) -> forge_types::LogLevel {
    match label {
        "Trace" => forge_types::LogLevel::Trace,
        "Debug" => forge_types::LogLevel::Debug,
        "Warn" => forge_types::LogLevel::Warn,
        "Error" => forge_types::LogLevel::Error,
        _ => forge_types::LogLevel::Info,
    }
}

fn add_sub_action_modal_view<'a>(
    form: &'a AddSubActionForm,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BannerKind;
    use iced::Length;
    use iced::widget::{column, row, text};

    let chip_send_chat = forge_widgets::category_chip(
        palette,
        "Send chat",
        palette.brand,
        form.kind == SubActionKindChoice::SendChat,
        Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::SendChat)),
    );
    let chip_set_global = forge_widgets::category_chip(
        palette,
        "Set global",
        palette.warning,
        form.kind == SubActionKindChoice::SetGlobal,
        Message::AddSubAction(AddSubActionMsg::KindSelected(
            SubActionKindChoice::SetGlobal,
        )),
    );
    let chip_delay = forge_widgets::category_chip(
        palette,
        "Delay",
        palette.info,
        form.kind == SubActionKindChoice::Delay,
        Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::Delay)),
    );
    let chip_log = forge_widgets::category_chip(
        palette,
        "Log",
        palette.text_muted,
        form.kind == SubActionKindChoice::Log,
        Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::Log)),
    );
    let chip_play_sound = forge_widgets::category_chip(
        palette,
        "Play sound",
        palette.success,
        form.kind == SubActionKindChoice::PlaySound,
        Message::AddSubAction(AddSubActionMsg::KindSelected(
            SubActionKindChoice::PlaySound,
        )),
    );
    let chip_speak = forge_widgets::category_chip(
        palette,
        "Speak",
        palette.info,
        form.kind == SubActionKindChoice::Speak,
        Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::Speak)),
    );
    let chip_read_file = forge_widgets::category_chip(
        palette,
        "Read file",
        palette.random,
        form.kind == SubActionKindChoice::ReadFile,
        Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::ReadFile)),
    );
    let chip_random_int = forge_widgets::category_chip(
        palette,
        "Random int",
        palette.warning,
        form.kind == SubActionKindChoice::RandomInt,
        Message::AddSubAction(AddSubActionMsg::KindSelected(
            SubActionKindChoice::RandomInt,
        )),
    );
    let chips_row = row![
        chip_send_chat,
        chip_set_global,
        chip_delay,
        chip_log,
        chip_play_sound,
        chip_speak,
        chip_read_file,
        chip_random_int,
    ]
    .spacing(6);

    let config_block: iced::Element<'_, Message> = match form.kind {
        SubActionKindChoice::SendChat => {
            let msg_input = forge_widgets::text_input_field(
                "Hello %user%!",
                &form.config.send_chat_message,
                |v| Message::AddSubAction(AddSubActionMsg::SendChatMessageChanged(v)),
                palette,
            );
            let helper = text("Variables: %user%, %message%, %args%")
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let msg_block = column![
                forge_widgets::section_header("MESSAGE", None, palette),
                msg_input,
                helper,
            ]
            .spacing(4);

            let p = *palette;
            let target_options: Vec<String> = vec!["twitch".to_string()];
            let selected_target = form.config.send_chat_target.clone();
            let target_select: iced::Element<'_, Message> =
                iced::widget::pick_list(target_options, Some(selected_target), |name: String| {
                    Message::AddSubAction(AddSubActionMsg::SendChatTargetChanged(name))
                })
                .padding(forge_widgets::inputs::input_padding())
                .width(Length::Fill)
                .style(move |_theme, status| {
                    use iced::widget::pick_list;
                    let border_color = match status {
                        pick_list::Status::Opened { .. } => p.border_active,
                        _ => p.border_regular,
                    };
                    pick_list::Style {
                        text_color: p.text_primary,
                        placeholder_color: p.text_muted,
                        handle_color: p.text_muted,
                        background: iced::Background::Color(p.shell),
                        border: iced::Border {
                            color: border_color,
                            width: 0.5,
                            radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                        },
                    }
                })
                .into();
            let target_block = column![
                forge_widgets::section_header("TARGET PLATFORM", None, palette),
                target_select,
            ]
            .spacing(6);

            column![msg_block, target_block].spacing(12).into()
        }
        SubActionKindChoice::SetGlobal => {
            let name_input = forge_widgets::text_input_field(
                "my_counter",
                &form.config.set_global_name,
                |v| Message::AddSubAction(AddSubActionMsg::SetGlobalNameChanged(v)),
                palette,
            );
            let name_block = column![
                forge_widgets::section_header("VARIABLE NAME", None, palette),
                name_input,
            ]
            .spacing(6);

            let val_input = forge_widgets::text_input_field(
                "%user% or 42",
                &form.config.set_global_value,
                |v| Message::AddSubAction(AddSubActionMsg::SetGlobalValueChanged(v)),
                palette,
            );
            let helper = text("Supports variable interpolation")
                .size(FONT_XS)
                .color(palette.warning)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace));
            let val_block = column![
                forge_widgets::section_header("VALUE", None, palette),
                val_input,
                helper,
            ]
            .spacing(4);

            column![name_block, val_block].spacing(12).into()
        }
        SubActionKindChoice::Delay => {
            let ms_input = forge_widgets::text_input_field(
                "500",
                &form.config.delay_ms,
                |v| Message::AddSubAction(AddSubActionMsg::DelayMsChanged(v)),
                palette,
            );
            column![
                forge_widgets::section_header("MILLISECONDS", None, palette),
                ms_input,
            ]
            .spacing(6)
            .into()
        }
        SubActionKindChoice::Log => {
            let p = *palette;
            let level_options: Vec<String> = vec![
                "Trace".to_string(),
                "Debug".to_string(),
                "Info".to_string(),
                "Warn".to_string(),
                "Error".to_string(),
            ];
            let selected_level = log_level_label(&form.config.log_level).to_string();
            let level_select: iced::Element<'_, Message> =
                iced::widget::pick_list(level_options, Some(selected_level), |name: String| {
                    Message::AddSubAction(AddSubActionMsg::LogLevelSelected(log_level_from_label(
                        &name,
                    )))
                })
                .padding(forge_widgets::inputs::input_padding())
                .width(Length::Fill)
                .style(move |_theme, status| {
                    use iced::widget::pick_list;
                    let border_color = match status {
                        pick_list::Status::Opened { .. } => p.border_active,
                        _ => p.border_regular,
                    };
                    pick_list::Style {
                        text_color: p.text_primary,
                        placeholder_color: p.text_muted,
                        handle_color: p.text_muted,
                        background: iced::Background::Color(p.shell),
                        border: iced::Border {
                            color: border_color,
                            width: 0.5,
                            radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                        },
                    }
                })
                .into();
            let level_block = column![
                forge_widgets::section_header("LEVEL", None, palette),
                level_select,
            ]
            .spacing(6);

            let msg_input = forge_widgets::text_input_field(
                "Action started",
                &form.config.log_message,
                |v| Message::AddSubAction(AddSubActionMsg::LogMessageChanged(v)),
                palette,
            );
            let msg_block = column![
                forge_widgets::section_header("MESSAGE", None, palette),
                msg_input,
            ]
            .spacing(6);

            column![level_block, msg_block].spacing(12).into()
        }
        SubActionKindChoice::PlaySound => {
            if form.available_clips.is_empty() {
                let hint = text("No clips yet \u{2014} add one in the Soundboard screen first.")
                    .size(FONT_SM)
                    .color(palette.text_muted);
                column![forge_widgets::section_header("CLIP", None, palette), hint]
                    .spacing(6)
                    .into()
            } else {
                let p = *palette;
                let clip_names: Vec<String> = form
                    .available_clips
                    .iter()
                    .map(|(_, n)| n.clone())
                    .collect();
                let selected_name = form.config.play_sound_clip_id.and_then(|id| {
                    form.available_clips
                        .iter()
                        .find(|(cid, _)| *cid == id)
                        .map(|(_, n)| n.clone())
                });
                let clips_for_closure = form.available_clips.clone();
                let clip_select: iced::Element<'_, Message> =
                    iced::widget::pick_list(clip_names, selected_name, move |name: String| {
                        let clip_id = clips_for_closure
                            .iter()
                            .find(|(_, n)| *n == name)
                            .map(|(id, _)| *id)
                            .unwrap_or_default();
                        Message::AddSubAction(AddSubActionMsg::PlaySoundClipSelected(clip_id))
                    })
                    .padding(forge_widgets::inputs::input_padding())
                    .width(Length::Fill)
                    .style(move |_theme, status| {
                        use iced::widget::pick_list;
                        let border_color = match status {
                            pick_list::Status::Opened { .. } => p.border_active,
                            _ => p.border_regular,
                        };
                        pick_list::Style {
                            text_color: p.text_primary,
                            placeholder_color: p.text_muted,
                            handle_color: p.text_muted,
                            background: iced::Background::Color(p.shell),
                            border: iced::Border {
                                color: border_color,
                                width: 0.5,
                                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
                            },
                        }
                    })
                    .into();
                column![
                    forge_widgets::section_header("CLIP", None, palette),
                    clip_select
                ]
                .spacing(6)
                .into()
            }
        }
        SubActionKindChoice::Speak => {
            use iced::widget::column;
            let text_block = column![
                forge_widgets::section_header("TEXT", None, palette),
                forge_widgets::inputs::text_input_field(
                    "Text to speak…",
                    &form.config.speak_text,
                    |v| Message::AddSubAction(AddSubActionMsg::SpeakTextChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            let voice_block = column![
                forge_widgets::section_header("VOICE OVERRIDE (optional)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "Leave blank to use alias resolver",
                    &form.config.speak_voice_override,
                    |v| Message::AddSubAction(AddSubActionMsg::SpeakVoiceOverrideChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            column![text_block, voice_block].spacing(12).into()
        }
        SubActionKindChoice::ReadFile => {
            use iced::widget::column;
            let path_block = column![
                forge_widgets::section_header("PATH (relative to assets sandbox)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "greetings/welcome.txt",
                    &form.config.read_file_path,
                    |v| Message::AddSubAction(AddSubActionMsg::ReadFilePathChanged(v)),
                    palette,
                ),
                text("Sandboxed under data_dir/assets/ · no ../ traversal · max 1 MiB")
                    .size(FONT_XS)
                    .color(palette.text_muted)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            ]
            .spacing(4);
            let target_block = column![
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
                forge_widgets::inputs::text_input_field(
                    "welcome_text",
                    &form.config.read_file_target_var,
                    |v| Message::AddSubAction(AddSubActionMsg::ReadFileTargetVarChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            column![path_block, target_block].spacing(12).into()
        }
        SubActionKindChoice::RandomInt => {
            use iced::widget::column;
            let min_block = column![
                forge_widgets::section_header("MIN", None, palette),
                forge_widgets::inputs::text_input_field(
                    "1",
                    &form.config.random_int_min,
                    |v| Message::AddSubAction(AddSubActionMsg::RandomIntMinChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            let max_block = column![
                forge_widgets::section_header("MAX", None, palette),
                forge_widgets::inputs::text_input_field(
                    "100",
                    &form.config.random_int_max,
                    |v| Message::AddSubAction(AddSubActionMsg::RandomIntMaxChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            let target_block = column![
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
                forge_widgets::inputs::text_input_field(
                    "dice_roll",
                    &form.config.random_int_target_var,
                    |v| Message::AddSubAction(AddSubActionMsg::RandomIntTargetVarChanged(v)),
                    palette,
                ),
            ]
            .spacing(6);
            column![row![min_block, max_block].spacing(8), target_block]
                .spacing(12)
                .into()
        }
    };

    let mut body_col = column![chips_row, config_block].spacing(16);

    if let Some(err) = form.error.as_deref() {
        body_col = body_col.push(forge_widgets::live_status_banner(
            BannerKind::Error,
            err,
            None,
            palette,
        ));
    }

    let (btn_label, title_label) = if form.editing_index.is_some() {
        ("Save changes", "Edit step")
    } else {
        ("Add step", "Add step")
    };

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::AddSubAction(AddSubActionMsg::Cancel),
        palette,
    );

    let add_on_press = Message::AddSubAction(AddSubActionMsg::Submit);
    let add_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button(btn_label, add_on_press, palette)
    } else {
        forge_widgets::secondary_button(btn_label, Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, add_btn].spacing(8);

    let footer: iced::Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
                .size(FONT_XS)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(iced::alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .into();

    let panel = sheet_chrome(
        title_label,
        Message::AddSubAction(AddSubActionMsg::Cancel),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::AddSubAction(AddSubActionMsg::Cancel),
        forge_widgets::SheetEdge::Right,
        480.0,
        palette,
    )
}

fn trigger_picker_card<'a>(
    label: &'a str,
    summary: &'a str,
    selected: bool,
    palette: &'a ForgePalette,
    on_press: Message,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let icon_el = container(tabler_icon(Icon::Bolt, FONT_SM, palette.brand))
        .width(24)
        .height(24)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.surface_overlay)),
            border: Border {
                radius: forge_widgets::radius(forge_widgets::Radius::Sm).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        });

    let label_col = column![
        text(label).size(FONT_SM).color(palette.text_primary),
        text(summary)
            .size(FONT_XS)
            .color(palette.text_muted)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
    ]
    .spacing(1);

    let inner = row![icon_el, container(label_col).width(Length::Fill),]
        .spacing(8)
        .align_y(Alignment::Center);

    let border_color = if selected {
        palette.brand
    } else {
        palette.border_regular
    };
    let border_width = if selected { 1.0 } else { 0.5 };

    button(inner)
        .on_press(on_press)
        .padding(iced::Padding {
            top: 8.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        })
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, _status| button::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: border_color,
                width: border_width,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
            text_color: palette.text_primary,
            shadow: iced::Shadow::default(),
            snap: false,
        })
        .into()
}

fn permission_label(perm: &forge_types::CommandPermission) -> &'static str {
    match perm {
        forge_types::CommandPermission::Everyone => "Everyone",
        forge_types::CommandPermission::Subscriber => "Subscriber",
        forge_types::CommandPermission::Vip => "Vip",
        forge_types::CommandPermission::Moderator => "Moderator",
        forge_types::CommandPermission::Broadcaster => "Broadcaster",
    }
}

fn permission_from_label(label: &str) -> forge_types::CommandPermission {
    match label {
        "Subscriber" => forge_types::CommandPermission::Subscriber,
        "Vip" => forge_types::CommandPermission::Vip,
        "Moderator" => forge_types::CommandPermission::Moderator,
        "Broadcaster" => forge_types::CommandPermission::Broadcaster,
        _ => forge_types::CommandPermission::Everyone,
    }
}

fn coming_soon_view(screen_label: String, palette: &ForgePalette) -> Element<'static, Message> {
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

fn breadcrumb_icon_for(screen: &Screen) -> Icon {
    match screen {
        Screen::Home => Icon::Home,
        Screen::Actions | Screen::ActionEditor(_) | Screen::Queues => Icon::Bolt,
        Screen::Commands => Icon::Terminal,
        Screen::Platforms => Icon::Broadcast,
        Screen::StreamApps | Screen::Integrations | Screen::IntegrationDetail(_) => {
            Icon::LayoutGrid
        }
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

fn screen_label(screen: &Screen) -> &'static str {
    match screen {
        Screen::Home => "Home",
        Screen::Actions => "Actions",
        Screen::ActionEditor(_) => "Actions",
        Screen::Queues => "Queues",
        Screen::Commands => "Commands",
        Screen::Platforms => "Platforms",
        Screen::StreamApps => "Stream apps",
        Screen::Integrations => "Integrations",
        Screen::IntegrationDetail(_) => "Integration",
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

fn integration_active(screen: &Screen, id: &str) -> bool {
    matches!(screen, Screen::IntegrationDetail(s) if s.as_str() == id)
}

fn nav_items_for<'a>(app: &'a App, palette: &'a ForgePalette) -> Sidebar<'a, Message> {
    let is_home = matches!(app.screen, Screen::Home);
    let is_actions = matches!(app.screen, Screen::Actions | Screen::ActionEditor(_));
    let is_queues = matches!(app.screen, Screen::Queues);
    let is_commands = matches!(app.screen, Screen::Commands);
    let is_live_chat = matches!(app.screen, Screen::LiveChat);
    let is_event_feed = matches!(app.screen, Screen::EventFeed);
    let is_globals = matches!(app.screen, Screen::Globals);
    let is_soundboard = matches!(app.screen, Screen::Soundboard);
    let is_tts = matches!(app.screen, Screen::Tts(_));
    let is_server = matches!(app.screen, Screen::Server);
    let is_settings = matches!(app.screen, Screen::Settings(_));

    let twitch_target = Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("twitch")));
    let obs_target = Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("obs")));

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
            icon: Icon::Terminal,
            label: "Commands",
            active: is_commands,
            on_press: Message::Navigate(Screen::Commands),
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
            active: integration_active(&app.screen, "twitch"),
            on_press: twitch_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.random,
            label: "YouTube",
            active: integration_active(&app.screen, "youtube"),
            on_press: Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("youtube"))),
        },
        NavItem::FlatLink {
            dot_color: palette.info,
            label: "Kick",
            active: integration_active(&app.screen, "kick"),
            on_press: Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("kick"))),
        },
        NavItem::FlatLink {
            dot_color: palette.success,
            label: "Trovo",
            active: integration_active(&app.screen, "trovo"),
            on_press: Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("trovo"))),
        },
        NavItem::MiniLabel("Stream apps"),
        NavItem::FlatLink {
            dot_color: palette.success,
            label: "OBS Studio",
            active: integration_active(&app.screen, "obs"),
            on_press: obs_target.clone(),
        },
        NavItem::FlatLink {
            dot_color: palette.warning,
            label: "VTube Studio",
            active: integration_active(&app.screen, "vtube"),
            on_press: Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("vtube"))),
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

fn tts_tab_button<'a>(
    label: &'static str,
    section: TtsSection,
    active: &TtsSection,
    palette: &'a ForgePalette,
) -> iced::widget::Button<'a, Message> {
    use iced::widget::{button, column, container, text};
    let is_active = *active == section;
    let fg = if is_active {
        palette.text_primary
    } else {
        palette.text_muted
    };
    let indicator_color = if is_active {
        palette.brand
    } else {
        iced::Color::TRANSPARENT
    };
    let inner = column![
        text(label).size(FONT_SM).color(fg),
        container(iced::widget::Space::new())
            .width(iced::Length::Fill)
            .height(2)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(indicator_color)),
                ..iced::widget::container::Style::default()
            }),
    ]
    .spacing(5);
    button(inner)
        .on_press(Message::Navigate(Screen::Tts(section)))
        .padding([7_u16, 14_u16])
        .style(|_, _| iced::widget::button::Style {
            background: None,
            ..iced::widget::button::Style::default()
        })
}

fn tts_section_view<'a>(
    app: &'a App,
    section: &'a TtsSection,
    palette: &'a ForgePalette,
) -> iced::Element<'a, Message> {
    use iced::widget::{column, container, row};
    let tab_bar = container(
        row![
            tts_tab_button("Dashboard", TtsSection::Dashboard, section, palette),
            tts_tab_button("Engines", TtsSection::Engines, section, palette),
            tts_tab_button("Voice aliases", TtsSection::Aliases, section, palette),
            tts_tab_button("Filters", TtsSection::Filters, section, palette),
            tts_tab_button("Triggers", TtsSection::Triggers, section, palette),
        ]
        .spacing(2),
    )
    .width(iced::Length::Fill)
    .style(move |_| iced::widget::container::Style {
        background: Some(iced::Background::Color(palette.shell)),
        border: iced::Border {
            color: palette.border_regular,
            width: 0.5,
            radius: 0.0.into(),
        },
        ..iced::widget::container::Style::default()
    });

    let content: iced::Element<'a, Message> = match section {
        TtsSection::Dashboard => tts_dashboard_view(&app.tts_dashboard, palette),
        TtsSection::Engines => tts_engines_view(&app.tts_engines, palette),
        TtsSection::Aliases => voice_aliases_view(&app.tts_aliases, palette),
        TtsSection::Filters => tts_filters_view(&app.tts_filters, palette),
        TtsSection::Triggers => tts_triggers_view(&app.tts_triggers, palette),
    };

    let section_label = match section {
        TtsSection::Dashboard => "Dashboard",
        TtsSection::Engines => "Engines",
        TtsSection::Aliases => "Voice aliases",
        TtsSection::Filters => "Filters",
        TtsSection::Triggers => "Triggers",
    };
    let page_header = simple_page_header(&[("TTS", false), (section_label, true)], palette);

    column![page_header, tab_bar, content]
        .spacing(0)
        .width(iced::Length::Fill)
        .height(iced::Length::Fill)
        .into()
}

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
            icon: Some(breadcrumb_icon_for(&app.screen)),
            label: screen_label(&app.screen),
            on_press: None::<Message>,
        }],
        palette,
    );

    let sidebar = sidebar(palette, nav_items_for(app, palette));

    let screen_content: Element<'_, Message> = match &app.screen {
        Screen::Home => home_view(app, palette),
        Screen::LiveChat => live_chat_view(&app.live_chat, &app.viewers, palette),
        Screen::Globals => globals_view(app, palette),
        Screen::Actions => actions_view(app, palette),
        Screen::ActionEditor(id) => action_editor_view(app, *id, palette),
        Screen::Queues => queues_view(&app.queues, palette),
        Screen::Commands => crate::commands_view::commands_view(&app.commands, palette),
        Screen::Settings(section) => settings_view(
            section,
            &app.settings_websocket,
            &app.server_screen,
            &app.settings_audio,
            palette,
        ),
        Screen::ScriptEditor => script_editor_view(app, palette),
        Screen::Platforms => platforms_overview_view(app, palette),
        Screen::StreamApps => stream_apps_view(app, palette),
        Screen::EventFeed => event_feed_view(&app.event_feed, palette),
        Screen::Server => server_screen_view(&app.server_screen, palette),
        Screen::IntegrationDetail(id) => {
            if id.as_str() == "twitch" && app.rt.twitch_chat_handle.is_none() {
                crate::twitch_panel::twitch_disconnected_view(&app.twitch_panel, palette)
            } else if id.as_str() == "obs" && app.rt.obs_client.is_none() {
                crate::obs_panel::obs_disconnected_view(&app.obs_panel, palette)
            } else if let Some((color, info)) = crate::platform_generic::registry(id, palette) {
                crate::platform_generic::platform_generic_view(color, info, palette)
            } else if let Some(state) = app.integration_detail.as_ref() {
                let inner = integration_detail_view(state, palette);
                if id.as_str() == "twitch" && app.rt.twitch_reauth_required {
                    iced::widget::container(
                        iced::widget::column![
                            crate::twitch_panel::twitch_reauth_banner(palette),
                            inner,
                        ]
                        .spacing(12.0),
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
        Screen::Soundboard => soundboard_view(&app.soundboard, palette),
        Screen::Tts(section) => tts_section_view(app, section, palette),
        other => coming_soon_view(format!("{other:?}"), palette),
    };

    let screen_uses_own_header = matches!(
        &app.screen,
        Screen::Actions
            | Screen::ActionEditor(_)
            | Screen::LiveChat
            | Screen::Home
            | Screen::Globals
            | Screen::Queues
            | Screen::Commands
            | Screen::EventFeed
            | Screen::Platforms
            | Screen::StreamApps
            | Screen::IntegrationDetail(_)
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

fn format_short_duration(d: time::Duration) -> String {
    let secs = d.whole_seconds().max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

async fn scan_overlay_root(root: &std::path::Path) -> crate::server_screen::OverlayListingSnapshot {
    use crate::server_screen::{
        OverlayListingSnapshot, OwnedFileMime, OwnedOverlayEntry, OwnedOverlayKind,
    };

    let root_str = root.to_string_lossy().into_owned();
    let mut read_dir = match tokio::fs::read_dir(root).await {
        Ok(rd) => rd,
        Err(_) => {
            return OverlayListingSnapshot {
                root: root_str,
                entries: Vec::new(),
            };
        }
    };

    let mut entries: Vec<OwnedOverlayEntry> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            let mut count: u32 = 0;
            if let Ok(mut child) = tokio::fs::read_dir(entry.path()).await {
                while let Ok(Some(_)) = child.next_entry().await {
                    count = count.saturating_add(1);
                }
            }
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::Dir,
                size_bytes: 0,
                child_count: count,
            });
        } else {
            let mime = OwnedFileMime::from_path(&entry.path());
            entries.push(OwnedOverlayEntry {
                name,
                kind: OwnedOverlayKind::File { mime },
                size_bytes: meta.len(),
                child_count: 0,
            });
        }
    }

    entries.sort_by(|a, b| {
        let dir_a = matches!(a.kind, OwnedOverlayKind::Dir);
        let dir_b = matches!(b.kind, OwnedOverlayKind::Dir);
        match (dir_a, dir_b) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase()),
        }
    });

    OverlayListingSnapshot {
        root: root_str,
        entries,
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    use iced::advanced::subscription::{EventStream, Hasher, Recipe, from_recipe};
    use iced::futures::StreamExt as _;

    struct BusRecipe(Arc<EventBus>);

    impl Recipe for BusRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let bus = self.0;
            iced::stream::channel(
                64,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let mut stream = bus.subscribe();
                    loop {
                        if let Ok(event) = stream.recv().await {
                            let _ = tx.try_send(Message::EventArrived(Arc::new(event)));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let bus = from_recipe(BusRecipe(app.rt.bus.clone()));

    struct ServerMetricsRecipe(Arc<crate::server_subsystem::ServerSubsystem>);

    impl Recipe for ServerMetricsRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            "server-metrics-tick".hash(state);
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let subsystem = self.0;
            iced::stream::channel(
                4,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    let mut tick_count: u32 = 0;
                    loop {
                        ticker.tick().await;
                        tick_count = tick_count.wrapping_add(1);
                        let Some(info) = subsystem.server_info().await else {
                            continue;
                        };
                        let bus_adapter = subsystem.bus_adapter().await;
                        let clients_guard = info.connected_clients.read().await;
                        let mut rows: Vec<crate::server_screen::OwnedClientRow> = Vec::new();
                        let mut events_per_second_total: f32 = 0.0;
                        for (client_id, client) in clients_guard.iter() {
                            let eps = client.events_per_second();
                            events_per_second_total += eps;
                            let subscriptions = match bus_adapter.as_ref() {
                                Some(adapter) => {
                                    let filters = adapter.current_subscriptions(*client_id).await;
                                    filters
                                        .into_iter()
                                        .map(|f| {
                                            let label = match (&f.source, &f.kind) {
                                                (Some(s), Some(k)) => {
                                                    format!("{}.{}", event_source_label(*s), k)
                                                }
                                                (Some(s), None) => {
                                                    format!("{}.*", event_source_label(*s))
                                                }
                                                (None, Some(k)) => k.clone(),
                                                (None, None) => "*".to_owned(),
                                            };
                                            let source =
                                                f.source.unwrap_or(forge_events::EventSource::Core);
                                            crate::server_screen::OwnedSubscriptionChip {
                                                label,
                                                source,
                                            }
                                        })
                                        .collect()
                                }
                                None => Vec::new(),
                            };
                            let liveness = if eps > 0.0 {
                                crate::server_screen::ClientLiveness::Active
                            } else {
                                crate::server_screen::ClientLiveness::Idle
                            };
                            rows.push(crate::server_screen::OwnedClientRow {
                                identification: (**client.identification.load()).clone(),
                                client_type_label: client.client_type.load().type_str().to_owned(),
                                liveness,
                                subscriptions,
                                events_per_second: eps,
                                uptime_short: format_short_duration(client.uptime()),
                            });
                        }
                        drop(clients_guard);
                        let kbps = info.bandwidth.current_bps() as f32 / 1000.0;
                        let peak_kbps = info.bandwidth.peak() as f32 / 1000.0;
                        let total_bytes = info.bandwidth.total();
                        let stats = crate::server_screen::ServerStats {
                            events_per_second: events_per_second_total,
                            events_per_second_avg: events_per_second_total,
                            http_requests: info.http_requests(),
                            bandwidth_kbps: kbps,
                            bandwidth_peak_kbps: peak_kbps,
                            total_bytes_sent: total_bytes,
                            total_events_out: info.events_out(),
                        };
                        let snapshot = crate::server_screen::ServerInfoSnapshot {
                            uptime_seconds: info.uptime_seconds(),
                            connected_clients: rows,
                            stats,
                        };
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::ServerInfoArrived(
                            snapshot,
                        )));
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::BandwidthTick(kbps)));

                        let should_scan = tick_count == 1 || tick_count.is_multiple_of(5);
                        if should_scan && let Some(root) = subsystem.overlay_root().await {
                            let listing = scan_overlay_root(root.as_ref()).await;
                            let _ = tx.try_send(Message::Server(
                                ServerScreenMsg::OverlayListingArrived(listing),
                            ));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let server_tick = if matches!(app.screen, Screen::Server) {
        from_recipe(ServerMetricsRecipe(Arc::clone(&app.rt.server_subsystem)))
    } else {
        Subscription::none()
    };

    let soundboard_keys = if matches!(app.screen, Screen::Soundboard) {
        iced::keyboard::listen().filter_map(soundboard_hotkey_filter)
    } else {
        Subscription::none()
    };

    struct SpeakEventRecipe(Arc<forge_speak_queue::SpeakQueueHandle>);

    impl Recipe for SpeakEventRecipe {
        type Output = Message;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash as _;
            "speak-event-stream".hash(state);
            (Arc::as_ptr(&self.0) as usize).hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: EventStream,
        ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
            let mut rx = self.0.subscribe();
            iced::stream::channel(
                64,
                |mut tx: iced::futures::channel::mpsc::Sender<Message>| async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                let _ =
                                    tx.try_send(Message::Tts(crate::message::TtsMsg::Dashboard(
                                        crate::message::TtsDashMsg::SpeakEventReceived(event),
                                    )));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let tts_events = if matches!(app.screen, Screen::Tts(_))
        && let Some(handle) = app.rt.speak_queue.as_ref()
    {
        from_recipe(SpeakEventRecipe(Arc::clone(handle)))
    } else {
        Subscription::none()
    };

    let toast_tick = iced::time::every(std::time::Duration::from_millis(200))
        .map(|instant| Message::Toast(crate::message::ToastMsg::Tick(instant)));

    let outside_click = iced::event::listen_with(|event, status, _window| match (event, status) {
        (
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_)),
            iced::event::Status::Ignored,
        ) => Some(Message::OutsideClick),
        _ => None,
    });

    if let Some(state) = app.integration_detail.as_ref() {
        Subscription::batch([
            bus,
            health_subscription(state),
            server_tick,
            soundboard_keys,
            tts_events,
            toast_tick,
            outside_click,
        ])
    } else {
        Subscription::batch([
            bus,
            server_tick,
            soundboard_keys,
            tts_events,
            toast_tick,
            outside_click,
        ])
    }
}

pub fn theme_callback(app: &App) -> Theme {
    app.theme.clone()
}

fn soundboard_hotkey_filter(event: iced::keyboard::Event) -> Option<Message> {
    use iced::keyboard::Event::KeyPressed;
    use iced::keyboard::Key::Character;
    use iced::keyboard::key::Named;

    let KeyPressed { key, modifiers, .. } = event else {
        return None;
    };

    let label = match &key {
        Character(c) => {
            if modifiers.control() {
                format!("Ctrl+{}", c.to_uppercase())
            } else if modifiers.shift() {
                format!("Shift+{}", c.to_uppercase())
            } else {
                return None;
            }
        }
        iced::keyboard::Key::Named(Named::F1) => "F1".to_string(),
        iced::keyboard::Key::Named(Named::F2) => "F2".to_string(),
        iced::keyboard::Key::Named(Named::F3) => "F3".to_string(),
        iced::keyboard::Key::Named(Named::F4) => "F4".to_string(),
        iced::keyboard::Key::Named(Named::F5) => "F5".to_string(),
        iced::keyboard::Key::Named(Named::F6) => "F6".to_string(),
        iced::keyboard::Key::Named(Named::F7) => "F7".to_string(),
        iced::keyboard::Key::Named(Named::F8) => "F8".to_string(),
        iced::keyboard::Key::Named(Named::F9) => "F9".to_string(),
        iced::keyboard::Key::Named(Named::F10) => "F10".to_string(),
        iced::keyboard::Key::Named(Named::F11) => "F11".to_string(),
        iced::keyboard::Key::Named(Named::F12) => "F12".to_string(),
        _ => return None,
    };

    Some(Message::Soundboard(
        crate::message::SoundboardMsg::HotkeyPressed(label),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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
    fn settings_reconnect_twitch_with_no_handle_dispatches_task() {
        let mut app = App::default();
        let task = update(
            &mut app,
            Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::Twitch)),
        );
        let _ = task;
        assert!(app.rt.twitch_chat_handle.is_none());
    }

    #[test]
    fn settings_reconnect_youtube_is_noop() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::YouTube)),
        );
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
    fn chat_submit_empty_input_is_noop() {
        let mut app = App::default();
        app.live_chat.chat_input = String::new();
        let _ = update(&mut app, Message::ChatSubmit);
        assert!(app.live_chat.chat_input.is_empty());
    }

    #[test]
    fn chat_submit_clears_input_and_dispatches_task() {
        let mut app = App::default();
        app.live_chat.chat_input = "hello chat".into();
        let _ = update(&mut app, Message::ChatSubmit);
        assert!(app.live_chat.chat_input.is_empty());
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

        let registry = Arc::new(forge_runtime::ScriptRegistry::new());
        let engine = forge_runtime::spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&dp),
            Arc::clone(&registry),
            None,
            None,
            None,
        );
        let scheduler =
            forge_runtime::QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
        let parser = forge_runtime::CommandParser::spawn(
            Arc::clone(&bus),
            Arc::clone(&dp),
            scheduler.clone(),
        );

        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&sqlite) as Arc<dyn CredentialsRepo>
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
                backend: sqlite,
                bus,
                script_registry: registry,
                server_subsystem,
                action_engine: Some(engine),
                scheduler: Some(scheduler),
                command_parser: Some(parser),
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
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            commands: crate::commands_view::CommandsState::new(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            integration_detail: None,
            server_screen: ServerScreenState::default(),
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            obs_panel: crate::obs_panel::ObsPanelState::default(),
            soundboard: SoundboardState::new(),
            settings_audio: SettingsAudioState::new(),
            tts_dashboard: TtsDashState::new(),
            tts_engines: TtsEnginesState::new(),
            tts_aliases: VoiceAliasesState::new(),
            tts_filters: TtsFiltersState::new(),
            tts_triggers: TtsTriggersState::new(),
        };

        assert!(app.rt.action_engine.is_some());
        assert!(app.rt.scheduler.is_some());
        assert!(app.rt.command_parser.is_some());
    }

    #[test]
    fn runtime_handles_absent_when_storage_offline() {
        let app = App {
            storage_offline: true,
            ..App::default()
        };

        assert!(app.rt.action_engine.is_none());
        assert!(app.rt.scheduler.is_none());
        assert!(app.rt.command_parser.is_none());
    }

    #[test]
    fn chat_sent_err_logs_and_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::LiveChat));
        let _ = update(&mut app, Message::ChatSent(Err("rate limited".into())));
        assert_eq!(app.screen, Screen::LiveChat);
    }

    #[test]
    fn navigate_to_actions_sets_loading_true() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        assert_eq!(app.screen, Screen::Actions);
    }

    #[test]
    fn tree_loaded_ok_clears_loading_flag() {
        let mut app = App::default();
        app.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TreeLoaded(Ok(vec![]))),
        );
        assert!(!app.actions.loading);
        assert!(app.actions.tree.is_empty());
    }

    #[test]
    fn tree_loaded_err_clears_loading_flag() {
        let mut app = App::default();
        app.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TreeLoaded(Err("db error".into()))),
        );
        assert!(!app.actions.loading);
    }

    #[test]
    fn action_selected_updates_selected_field() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        let _ = update(&mut app, Message::Actions(ActionsMsg::ActionSelected(id)));
        assert_eq!(app.actions.selected, Some(id));
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
            triggers: vec![],
            commands: vec![],
        };
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::DetailLoaded(Ok(detail))),
        );
        assert!(app.actions.detail.is_some());
        assert_eq!(app.actions.detail.as_ref().unwrap().action.name, "!quote");
    }

    #[test]
    fn detail_loaded_err_clears_detail() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::DetailLoaded(Err("not found".into()))),
        );
        assert!(app.actions.detail.is_none());
    }

    #[test]
    fn telemetry_loaded_ok_stores_and_clears_loading() {
        let mut app = App::default();
        app.actions.telemetry_loading = true;
        let t = forge_storage::ActionTelemetry {
            runs_today: 42,
            ..Default::default()
        };
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TelemetryLoaded(Ok(t.clone()))),
        );
        assert!(!app.actions.telemetry_loading);
        assert_eq!(app.actions.telemetry.as_ref().unwrap().runs_today, 42);
    }

    #[test]
    fn telemetry_loaded_err_clears_loading_and_data() {
        let mut app = App::default();
        app.actions.telemetry_loading = true;
        app.actions.telemetry = Some(forge_storage::ActionTelemetry::default());
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TelemetryLoaded(Err("timeout".into()))),
        );
        assert!(!app.actions.telemetry_loading);
        assert!(app.actions.telemetry.is_none());
    }

    #[test]
    fn action_selected_sets_telemetry_loading_true() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        let _ = update(&mut app, Message::Actions(ActionsMsg::ActionSelected(id)));
        assert!(app.actions.telemetry_loading);
        assert!(app.actions.telemetry.is_none());
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
        app.actions.detail = Some(crate::actions::ActionDetail {
            sub_action_avg_ms: vec![],
            action,
            triggers: vec![],
            commands: vec![],
        });
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::ToggleEnabled(id, false)),
        );
        assert!(!app.actions.detail.as_ref().unwrap().action.enabled);
    }

    #[test]
    fn actions_delete_clears_selection_and_detail() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        app.actions.selected = Some(id);
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::ActionDeleted(Ok(()))),
        );
        assert!(app.actions.selected.is_none());
        assert!(app.actions.detail.is_none());
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
        assert!(app.actions.add_action_modal.is_none());
        let _ = update(&mut app, Message::AddAction(AddActionMsg::OpenRequested));
        assert!(app.actions.add_action_modal.is_some());
    }

    #[test]
    fn cancel_clears_modal() {
        let mut app = App::default();
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(&mut app, Message::AddAction(AddActionMsg::Cancel));
        assert!(app.actions.add_action_modal.is_none());
    }

    #[test]
    fn name_changed_updates_form() {
        let mut app = App::default();
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::AddAction(AddActionMsg::NameChanged("Sub raid".to_string())),
        );
        assert_eq!(
            app.actions.add_action_modal.as_ref().unwrap().name,
            "Sub raid"
        );
    }

    #[test]
    fn submit_with_invalid_form_is_noop() {
        let mut app = App::default();
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(&mut app, Message::AddAction(AddActionMsg::Submit));
        assert!(app.actions.add_action_modal.is_some(), "modal remains open");
    }

    #[test]
    fn saved_ok_closes_modal_and_sets_selected() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let new_id = ActionId::new();
        let _ = update(
            &mut app,
            Message::AddAction(AddActionMsg::Saved(Ok(new_id))),
        );
        assert!(app.actions.add_action_modal.is_none());
    }

    #[test]
    fn saved_err_keeps_modal_open_with_error() {
        let mut app = App::default();
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = update(
            &mut app,
            Message::AddAction(AddActionMsg::Saved(Err("db locked".to_string()))),
        );
        let form = app.actions.add_action_modal.as_ref().unwrap();
        assert_eq!(form.error.as_deref(), Some("db locked"));
        assert!(!form.saving);
    }

    #[test]
    fn view_compiles_actions_with_open_modal() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        app.actions.add_action_modal = Some(crate::actions::AddActionForm::new());
        let _ = view(&app);
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn submit_with_valid_form_sets_saving_and_saved_ok_stores_action() {
        use forge_storage::DataProvider;
        use forge_types::{Queue, QueueId};

        let dp = Arc::new(
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
                backend: Arc::clone(&dp),
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
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            commands: crate::commands_view::CommandsState::new(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            integration_detail: None,
            server_screen: ServerScreenState::default(),
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            obs_panel: crate::obs_panel::ObsPanelState::default(),
            soundboard: SoundboardState::new(),
            settings_audio: SettingsAudioState::new(),
            tts_dashboard: TtsDashState::new(),
            tts_engines: TtsEnginesState::new(),
            tts_aliases: VoiceAliasesState::new(),
            tts_filters: TtsFiltersState::new(),
            tts_triggers: TtsTriggersState::new(),
        };

        let mut form = crate::actions::AddActionForm::new();
        form.name = "My test action".to_string();
        form.set_queue_options(vec![(queue.id, "default".to_string())]);
        app.actions.add_action_modal = Some(form);

        let _ = update(&mut app, Message::AddAction(AddActionMsg::Submit));
        assert!(app.actions.add_action_modal.as_ref().unwrap().saving);

        let saved_id = forge_types::ActionId::new();
        let _ = update(
            &mut app,
            Message::AddAction(AddActionMsg::Saved(Ok(saved_id))),
        );
        assert!(app.actions.add_action_modal.is_none());
    }

    #[test]
    fn open_add_sub_action_modal_creates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        assert!(app.actions.add_sub_action_modal.is_none());
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::OpenRequested(id)),
        );
        assert!(app.actions.add_sub_action_modal.is_some());
        assert_eq!(
            app.actions
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
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(&mut app, Message::AddSubAction(AddSubActionMsg::Cancel));
        assert!(app.actions.add_sub_action_modal.is_none());
    }

    #[test]
    fn kind_selected_updates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::KindSelected(SubActionKindChoice::Delay)),
        );
        assert_eq!(
            app.actions.add_sub_action_modal.as_ref().unwrap().kind,
            SubActionKindChoice::Delay,
        );
    }

    #[test]
    fn send_chat_message_changed_updates_form() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::SendChatMessageChanged(
                "Hello %user%!".to_string(),
            )),
        );
        assert_eq!(
            app.actions
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
        app.actions.add_sub_action_modal = Some(form);
        let _ = update(&mut app, Message::AddSubAction(AddSubActionMsg::Submit));
        let f = app.actions.add_sub_action_modal.as_ref().unwrap();
        assert!(f.error.is_some());
    }

    #[test]
    fn add_sub_action_saved_ok_closes_modal() {
        use forge_types::ActionId;
        let mut app = App::default();
        let id = ActionId::new();
        app.actions.add_sub_action_modal = Some(crate::actions::AddSubActionForm::new(id));
        app.actions.selected = Some(id);
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::Saved(Ok(()))),
        );
        assert!(app.actions.add_sub_action_modal.is_none());
    }

    #[test]
    fn add_sub_action_saved_err_keeps_modal_with_error() {
        use forge_types::ActionId;
        let mut app = App::default();
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::Saved(Err("db locked".to_string()))),
        );
        let f = app.actions.add_sub_action_modal.as_ref().unwrap();
        assert_eq!(f.error.as_deref(), Some("db locked"));
        assert!(!f.saving);
    }

    #[test]
    fn view_compiles_actions_with_add_sub_action_modal() {
        use forge_types::ActionId;
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let _ = view(&app);
    }

    #[test]
    fn clips_loaded_populates_available_clips() {
        use forge_types::{ActionId, ClipId};
        let mut app = App::default();
        app.actions.add_sub_action_modal =
            Some(crate::actions::AddSubActionForm::new(ActionId::new()));
        let clip_id = ClipId::new();
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::ClipsLoaded(vec![(
                clip_id,
                "Airhorn".to_string(),
            )])),
        );
        let clips = &app
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
        app.actions.add_sub_action_modal = Some(form);
        let clip_id = ClipId::new();
        let _ = update(
            &mut app,
            Message::AddSubAction(AddSubActionMsg::PlaySoundClipSelected(clip_id)),
        );
        assert_eq!(
            app.actions
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
        app.actions.add_sub_action_modal = Some(form);
        let _ = update(&mut app, Message::AddSubAction(AddSubActionMsg::Submit));
        let f = app.actions.add_sub_action_modal.as_ref().unwrap();
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
        app.actions.add_sub_action_modal = Some(form);
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
        app.home.actions_count = Some(47);
        app.home.commands_count = Some(23);
        app.home.triggers_fired = Some(1284);
        app.home.globals_count = Some(31);
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
            commands_count: 3,
            triggers_fired: 42,
            globals_count: 7,
        };
        let _ = update(&mut app, Message::Home(HomeMsg::StatsLoaded(Ok(data))));
        assert_eq!(app.home.actions_count, Some(5));
        assert_eq!(app.home.commands_count, Some(3));
        assert_eq!(app.home.triggers_fired, Some(42));
        assert_eq!(app.home.globals_count, Some(7));
    }

    #[test]
    fn home_stats_loaded_err_leaves_nones() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(
            &mut app,
            Message::Home(HomeMsg::StatsLoaded(Err("db error".into()))),
        );
        assert!(app.home.actions_count.is_none());
        assert!(app.home.commands_count.is_none());
        assert!(app.home.triggers_fired.is_none());
        assert!(app.home.globals_count.is_none());
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
        app.home.actions_count = Some(12);
        app.home.commands_count = Some(5);
        app.home.triggers_fired = Some(99);
        app.home.globals_count = Some(3);
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
        app.home.actions_count = Some(47);
        app.home.triggers_fired = Some(1284);
        let _ = view(&app);
    }

    #[test]
    fn navigate_to_integration_detail_sets_screen() {
        use forge_platform_core::IntegrationId;
        let mut app = App::default();
        let id = IntegrationId::new("obs");
        let _ = update(
            &mut app,
            Message::Navigate(Screen::IntegrationDetail(id.clone())),
        );
        assert_eq!(app.screen, Screen::IntegrationDetail(id));
    }

    #[test]
    fn view_compiles_integration_detail_without_state() {
        use forge_platform_core::IntegrationId;
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("obs"))),
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
            Message::ObsBootResult(Ok(ObsClientRef::new(Arc::new(client)))),
        );
        assert!(app.rt.obs_client.is_some());
        assert!(app.integration_detail.is_some());
    }

    #[test]
    fn obs_boot_result_err_leaves_obs_client_none() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::ObsBootResult(Err("connection refused".into())),
        );
        assert!(app.rt.obs_client.is_none());
        assert!(app.integration_detail.is_none());
    }
}
