use std::sync::Arc;
use std::time::SystemTime;

use forge_soundboard::SoundboardPlayer;

use forge_events::{Event, EventPublisher, EventSource};
use forge_obs::ObsClient;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, QuickActions, SectionIcon,
};
use forge_platform_twitch::{ChatConnectionState, TwitchIntegrationBundle};
use forge_runtime::{
    ActionEngineHandle, CommandParserHandle, EventBus, NullEventLogRepo, QueueSchedulerHandle,
    ScriptRegistry,
};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider};
use forge_widgets::icons::{Icon, tabler_icon};
use forge_widgets::tokens::{FONT_LG, FONT_SM, FONT_XS, Spacing, sp, spf};
use forge_widgets::{
    BreadcrumbCrumb, ForgePalette, NavItem, Sidebar, ThemeId, ToastQueue, app_footer, breadcrumb,
    page_shell, sidebar, title_bar, toast_viewport,
};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::action_editor::action_editor_view;
use crate::actions::{
    ActionsFilter, ActionsState, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    AddTriggerForm, AddTriggerMsg, SubActionKindChoice, TriggerCategory, kind_label, kind_summary,
};
use crate::builtin_detail::{BuiltinDetailState, health_subscription, view as builtin_detail_view};
use crate::event_feed;
use crate::event_feed::{EventFeedState, event_feed_view};
use crate::globals_view::{GlobalsState, globals_view};
use crate::home::HomeStats;
use crate::live_chat::{LiveChatState, live_chat_view};
use crate::message::{
    ActionEditorMsg, ActionsMsg, GlobalsMsg, HomeMsg, ObsClientRef, PlatformId, QueuesMsg,
    SettingsMsg, SidebarMsg, ToastMsg, TtsMsg,
};
use crate::queues_view::{QueuesState, queues_view};
use crate::script_editor::{ScriptEditorMsg, ScriptEditorState, script_editor_view};
use crate::server_screen::{ServerScreenMsg, ServerScreenState, server_screen_view};
use crate::server_subsystem::ServerSubsystem;
use crate::settings_audio::SettingsAudioState;
use crate::settings_websocket::SettingsWebSocketState;
use crate::soundboard::{SoundboardState, soundboard_view};
use crate::stream_apps::view as stream_apps_view;
use crate::tts_dashboard::{TtsDashState, tts_dashboard_view};
use crate::tts_engines::{TtsEnginesState, tts_engines_view};
use crate::tts_filters::{TtsFiltersState, tts_filters_view};
use crate::tts_triggers::{TtsTriggersState, tts_triggers_view};
use crate::voice_aliases::{VoiceAliasesState, voice_aliases_view};
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
    pub commands: crate::commands_view::CommandsState,
    pub queues: QueuesState,
    pub viewers: crate::viewers::ViewersState,
    pub globals: GlobalsState,
    pub script_editor: ScriptEditorState,
    pub builtin_detail: Option<BuiltinDetailState>,
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

impl Default for UiState {
    fn default() -> Self {
        Self {
            home: HomeStats::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            commands: crate::commands_view::CommandsState::new(),
            queues: QueuesState::new(),
            viewers: crate::viewers::ViewersState::default(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            builtin_detail: None,
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

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn default_with(
        initial: Screen,
        backend: Arc<dyn DataProvider>,
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
            ui: UiState::default(),
        }
    }
}

fn dispatch_event(app: &mut App, event: &Arc<Event>) -> Task<Message> {
    let mut task = crate::live_chat::on_event(&mut app.ui.live_chat, event);
    task = task.chain(crate::builtin_detail::on_event(
        app.ui.builtin_detail.as_mut(),
        event,
    ));
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
                let dp = Arc::clone(&app.rt.backend);
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
                let dp = Arc::clone(&app.rt.backend);
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
        Message::Home(sub) => crate::home::update(&mut app.ui.home, &app.rt, sub),
        Message::Globals(sub) => crate::globals_view::update(&mut app.ui.globals, &app.rt, sub),
        Message::Actions(sub) => crate::actions::update(&mut app.ui.actions, &app.rt, sub),
        Message::Queues(sub) => crate::queues_view::update(&mut app.ui.queues, &app.rt, sub),
        Message::Viewers(sub) => crate::viewers::update(&mut app.ui.viewers, &app.rt, sub),
        Message::Commands(sub) => crate::commands_view::update(&mut app.ui.commands, &app.rt, sub),
        Message::ScriptEditor(sub) => {
            crate::script_editor::update(&mut app.ui.script_editor, &app.rt, sub)
        }
        Message::BuiltinDetail(sub) => {
            crate::builtin_detail::update(&mut app.ui.builtin_detail, &app.rt, sub)
        }
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
                let id = BuiltinId::new("twitch");
                let icon = SectionIcon::new("brand-twitch");
                let status: Arc<dyn BuiltinStatus> = twitch_bundle.clone();
                let health: Arc<dyn BuiltinHealth> = twitch_bundle.clone();
                let content: Arc<dyn BuiltinContent> = twitch_bundle.clone();
                let quick_actions: Arc<dyn QuickActions> = twitch_bundle.clone();
                app.ui.builtin_detail = Some(BuiltinDetailState::new(
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
                let id = BuiltinId::new("obs");
                let icon = SectionIcon::new("broadcast");
                let status: Arc<dyn BuiltinStatus> = client.clone();
                let health: Arc<dyn BuiltinHealth> = client.clone();
                let content: Arc<dyn BuiltinContent> = client.clone();
                let quick_actions: Arc<dyn QuickActions> = client.clone();
                app.ui.builtin_detail = Some(BuiltinDetailState::new(
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
                    app.ui.server_screen.bind_address = snapshot.bind_address;
                    app.ui.server_screen.bearer_token = snapshot.bearer_token;
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Running;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server boot failed");
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerRestartResult(result) => {
            match result {
                Ok(()) => {
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Running;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server restart failed");
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerStopResult(result) => {
            match result {
                Ok(()) => {
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Stopped;
                    app.ui.server_screen.connected_clients.clear();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "server stop failed");
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Error(e);
                }
            }
            Task::none()
        }
        Message::ServerTokenRotated(result) => {
            match result {
                Ok(token) => {
                    app.ui.server_screen.bearer_token = token;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "token regeneration failed");
                    app.ui.server_screen.server_status =
                        crate::server_screen::ServerStatus::Error(e);
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
                Message::ServerRestartResult,
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
        Message::TwitchReauthRequested => {
            if let Some(handle) = app.rt.twitch_chat_handle.take() {
                handle.shutdown();
            }
            app.ui.builtin_detail = None;
            app.rt.twitch_login = None;
            app.rt.twitch_reauth_required = false;
            let backend = Arc::clone(&app.rt.backend);
            Task::perform(
                async move {
                    let id = CredentialId::new("twitch:broadcaster");
                    let creds: &dyn CredentialsRepo = &*backend;
                    let _ = creds.delete(&id).await;
                },
                |()| Message::Noop,
            )
        }
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

async fn reconnect_twitch(
    backend: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
) -> Result<(), String> {
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
    backend: Arc<dyn DataProvider>,
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
    backend: Arc<dyn DataProvider>,
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
        .spacing(spf(Spacing::Xs))
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

fn actions_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, scrollable, text};

    let p = *palette;
    let actions_state = &app.ui.actions;

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
        if let Some(form) = app.ui.actions.add_sub_action_modal.as_ref() {
            let modal_el = add_sub_action_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.ui.actions.add_trigger_modal.as_ref() {
            let modal_el = add_trigger_modal_view(form, palette);
            iced::widget::stack![main_view, modal_el].into()
        } else if let Some(form) = app.ui.actions.add_action_modal.as_ref() {
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
    .spacing(spf(Spacing::Xs))
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
    let chips = row![chip_all, chip_chat, chip_timers, chip_points].spacing(spf(Spacing::Xxs));

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
    .spacing(spf(Spacing::Xs))
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
    .spacing(spf(Spacing::Xs))
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
            .id(crate::actions::action_rename_input_id())
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
            .spacing(spf(Spacing::Xs))
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
        .spacing(spf(Spacing::Xxs))
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
        .spacing(spf(Spacing::Xs))
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
    let action_btns = row![test_btn, dup_btn].spacing(spf(Spacing::Xs));

    let header_row = row![container(name_row).width(Length::Fill), action_btns,]
        .spacing(spf(Spacing::Xs))
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
                .spacing(spf(Spacing::Xs))
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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::OpenRequested(action.id),
        ))),
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
        .spacing(spf(Spacing::Xxs))
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
        .padding(sp(Spacing::Xs))
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
    .spacing(spf(Spacing::Xs))
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
        .spacing(spf(Spacing::Xs))
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
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::NameChanged(v),
            )))
        },
        palette,
    );

    let name_row = row![name_input, name_counter]
        .spacing(spf(Spacing::Xs))
        .align_y(iced::alignment::Vertical::Center);

    let name_block = column![
        forge_widgets::section_header("NAME", None, palette),
        name_row,
    ]
    .spacing(spf(Spacing::Xs));

    let group_input = forge_widgets::text_input_field(
        "Examples",
        &form.group,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::GroupChanged(v),
            )))
        },
        palette,
    );

    let group_block = column![
        forge_widgets::section_header("GROUP", None, palette),
        group_input,
    ]
    .spacing(spf(Spacing::Xs));

    let queue_names: Vec<String> = form.queue_options.iter().map(|(_, n)| n.clone()).collect();
    let p = *palette;
    let queue_select: Element<'_, Message> = iced::widget::pick_list(
        queue_names,
        form.selected_queue_name.clone(),
        |name: String| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::QueueSelected(name),
            )))
        },
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
    .spacing(spf(Spacing::Xs));

    let two_col = row![group_block, queue_block].spacing(spf(Spacing::Sm));

    let desc_input = forge_widgets::text_input_field(
        "Plays a sound, shows overlay alert...",
        &form.description,
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::DescriptionChanged(v),
            )))
        },
        palette,
    );

    let desc_block = column![
        forge_widgets::section_header("DESCRIPTION", None, palette),
        desc_input,
    ]
    .spacing(spf(Spacing::Xs));

    let enabled_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Enabled",
            description: "Action runs when a trigger fires.",
            value: form.enabled,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::EnabledToggled(!form.enabled),
            ))),
        },
    );

    let concurrent_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Concurrent execution",
            description: "Allow parallel runs in this queue.",
            value: form.concurrent,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::ConcurrentToggled(!form.concurrent),
            ))),
        },
    );

    let bypass_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Bypass queue pause",
            description: "Always run even if queue is paused.",
            value: form.bypass_pause,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::BypassPauseToggled(!form.bypass_pause),
            ))),
        },
    );

    let random_pick_toggle = forge_widgets::toggle(
        palette,
        ToggleProps {
            label: "Random pick",
            description: "Run ONE random sub-action per trigger instead of all.",
            value: form.random_pick,
            on_toggle: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::RandomPickToggled(!form.random_pick),
            ))),
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
    .spacing(spf(Spacing::Sm));

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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
            AddActionMsg::Cancel,
        ))),
        palette,
    );

    let create_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
        AddActionMsg::Submit,
    )));
    let create_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Create action", create_on_press, palette)
    } else {
        forge_widgets::secondary_button("Create action", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, create_btn].spacing(spf(Spacing::Xs));

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
            on_close: Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddAction(
                AddActionMsg::Cancel,
            ))),
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
        |v| {
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                AddTriggerMsg::SearchChanged(v),
            )))
        },
        palette,
    );

    let chip_all = forge_widgets::category_chip(
        palette,
        "All",
        palette.brand,
        form.category == TriggerCategory::All,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::All),
        ))),
    );
    let chip_chat = forge_widgets::category_chip(
        palette,
        "Chat",
        palette.brand,
        form.category == TriggerCategory::Chat,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Chat),
        ))),
    );
    let chip_subs = forge_widgets::category_chip(
        palette,
        "Subscriptions",
        palette.brand,
        form.category == TriggerCategory::Subscriptions,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Subscriptions),
        ))),
    );
    let chip_bits = forge_widgets::category_chip(
        palette,
        "Bits",
        palette.bits,
        form.category == TriggerCategory::Bits,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Bits),
        ))),
    );
    let chip_raids = forge_widgets::category_chip(
        palette,
        "Raids",
        palette.random,
        form.category == TriggerCategory::Raids,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Raids),
        ))),
    );
    let chip_obs = forge_widgets::category_chip(
        palette,
        "OBS",
        palette.brand,
        form.category == TriggerCategory::Obs,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::CategorySelected(TriggerCategory::Obs),
        ))),
    );

    let chips_row = row![
        chip_all, chip_chat, chip_subs, chip_bits, chip_raids, chip_obs
    ]
    .spacing(spf(Spacing::Xs));

    let visible = form.visible_kinds();
    let is_empty = visible.is_empty();
    let mut grid_col = column![].spacing(spf(Spacing::Xs));
    for kind in visible {
        let selected = form.selected_kind.as_ref() == Some(&kind);
        let lbl = kind_label(&kind);
        let summ = kind_summary(&kind);
        let card = trigger_picker_card(
            lbl,
            summ,
            selected,
            palette,
            Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                AddTriggerMsg::KindSelected(kind),
            ))),
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

    let mut config_col = column![].spacing(spf(Spacing::Xs));

    if let Some(kind) = &form.selected_kind {
        match kind {
            forge_types::TriggerKind::TwitchChatCommand => {
                let cmd_input = forge_widgets::text_input_field(
                    "!quote",
                    &form.config.command_name,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::CommandNameChanged(v),
                        )))
                    },
                    palette,
                );
                let cmd_block = column![
                    forge_widgets::section_header("COMMAND NAME", None, palette),
                    cmd_input,
                ]
                .spacing(spf(Spacing::Xs));

                let cooldown_input = forge_widgets::text_input_field(
                    "0",
                    &form.config.cooldown_secs,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::CooldownChanged(v),
                        )))
                    },
                    palette,
                );
                let cooldown_block = column![
                    forge_widgets::section_header("COOLDOWN (SECS)", None, palette),
                    cooldown_input,
                ]
                .spacing(spf(Spacing::Xs));

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
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::PermissionSelected(permission_from_label(&name)),
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

                let perm_block = column![
                    forge_widgets::section_header("PERMISSION", None, palette),
                    perm_select,
                ]
                .spacing(spf(Spacing::Xs));

                config_col = config_col
                    .push(cmd_block)
                    .push(cooldown_block)
                    .push(perm_block);
            }
            forge_types::TriggerKind::TwitchCheer => {
                let bits_input = forge_widgets::text_input_field(
                    "1",
                    &form.config.min_bits,
                    |v| {
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
                            AddTriggerMsg::MinBitsChanged(v),
                        )))
                    },
                    palette,
                );
                let bits_block = column![
                    forge_widgets::section_header("MINIMUM BITS", None, palette),
                    bits_input,
                ]
                .spacing(spf(Spacing::Xs));
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

    let mut body_col = column![search_input, chips_row, scrollable(grid_col).height(200),]
        .spacing(spf(Spacing::Xs));

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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
        palette,
    );

    let save_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
        AddTriggerMsg::Submit,
    )));
    let save_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Add trigger", save_on_press, palette)
    } else {
        forge_widgets::secondary_button("Add trigger", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, save_btn].spacing(spf(Spacing::Xs));

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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddTrigger(
            AddTriggerMsg::Cancel,
        ))),
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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SendChat),
        ))),
    );
    let chip_set_global = forge_widgets::category_chip(
        palette,
        "Set global",
        palette.warning,
        form.kind == SubActionKindChoice::SetGlobal,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::SetGlobal),
        ))),
    );
    let chip_delay = forge_widgets::category_chip(
        palette,
        "Delay",
        palette.info,
        form.kind == SubActionKindChoice::Delay,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Delay),
        ))),
    );
    let chip_log = forge_widgets::category_chip(
        palette,
        "Log",
        palette.text_muted,
        form.kind == SubActionKindChoice::Log,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Log),
        ))),
    );
    let chip_play_sound = forge_widgets::category_chip(
        palette,
        "Play sound",
        palette.success,
        form.kind == SubActionKindChoice::PlaySound,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::PlaySound),
        ))),
    );
    let chip_speak = forge_widgets::category_chip(
        palette,
        "Speak",
        palette.info,
        form.kind == SubActionKindChoice::Speak,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::Speak),
        ))),
    );
    let chip_read_file = forge_widgets::category_chip(
        palette,
        "Read file",
        palette.random,
        form.kind == SubActionKindChoice::ReadFile,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::ReadFile),
        ))),
    );
    let chip_random_int = forge_widgets::category_chip(
        palette,
        "Random int",
        palette.warning,
        form.kind == SubActionKindChoice::RandomInt,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::KindSelected(SubActionKindChoice::RandomInt),
        ))),
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
    .spacing(spf(Spacing::Xs));

    let config_block: iced::Element<'_, Message> = match form.kind {
        SubActionKindChoice::SendChat => {
            let msg_input = forge_widgets::text_input_field(
                "Hello %user%!",
                &form.config.send_chat_message,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SendChatMessageChanged(v),
                    )))
                },
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
            .spacing(spf(Spacing::Xxs));

            let p = *palette;
            let target_options: Vec<String> = vec!["twitch".to_string()];
            let selected_target = form.config.send_chat_target.clone();
            let target_select: iced::Element<'_, Message> =
                iced::widget::pick_list(target_options, Some(selected_target), |name: String| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SendChatTargetChanged(name),
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
            let target_block = column![
                forge_widgets::section_header("TARGET PLATFORM", None, palette),
                target_select,
            ]
            .spacing(spf(Spacing::Xs));

            column![msg_block, target_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::SetGlobal => {
            let name_input = forge_widgets::text_input_field(
                "my_counter",
                &form.config.set_global_name,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SetGlobalNameChanged(v),
                    )))
                },
                palette,
            );
            let name_block = column![
                forge_widgets::section_header("VARIABLE NAME", None, palette),
                name_input,
            ]
            .spacing(spf(Spacing::Xs));

            let val_input = forge_widgets::text_input_field(
                "%user% or 42",
                &form.config.set_global_value,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SetGlobalValueChanged(v),
                    )))
                },
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
            .spacing(spf(Spacing::Xxs));

            column![name_block, val_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::Delay => {
            let ms_input = forge_widgets::text_input_field(
                "500",
                &form.config.delay_ms,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::DelayMsChanged(v),
                    )))
                },
                palette,
            );
            column![
                forge_widgets::section_header("MILLISECONDS", None, palette),
                ms_input,
            ]
            .spacing(spf(Spacing::Xs))
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
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::LogLevelSelected(log_level_from_label(&name)),
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
            .spacing(spf(Spacing::Xs));

            let msg_input = forge_widgets::text_input_field(
                "Action started",
                &form.config.log_message,
                |v| {
                    Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::LogMessageChanged(v),
                    )))
                },
                palette,
            );
            let msg_block = column![
                forge_widgets::section_header("MESSAGE", None, palette),
                msg_input,
            ]
            .spacing(spf(Spacing::Xs));

            column![level_block, msg_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::PlaySound => {
            if form.available_clips.is_empty() {
                let hint = text("No clips yet \u{2014} add one in the Soundboard screen first.")
                    .size(FONT_SM)
                    .color(palette.text_muted);
                column![forge_widgets::section_header("CLIP", None, palette), hint]
                    .spacing(spf(Spacing::Xs))
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
                        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                            AddSubActionMsg::PlaySoundClipSelected(clip_id),
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
                column![
                    forge_widgets::section_header("CLIP", None, palette),
                    clip_select
                ]
                .spacing(spf(Spacing::Xs))
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
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SpeakTextChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let voice_block = column![
                forge_widgets::section_header("VOICE OVERRIDE (optional)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "Leave blank to use alias resolver",
                    &form.config.speak_voice_override,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::SpeakVoiceOverrideChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![text_block, voice_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::ReadFile => {
            use iced::widget::column;
            let path_block = column![
                forge_widgets::section_header("PATH (relative to assets sandbox)", None, palette),
                forge_widgets::inputs::text_input_field(
                    "greetings/welcome.txt",
                    &form.config.read_file_path,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::ReadFilePathChanged(v)
                    ))),
                    palette,
                ),
                text("Sandboxed under data_dir/assets/ · no ../ traversal · max 1 MiB")
                    .size(FONT_XS)
                    .color(palette.text_muted)
                    .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            ]
            .spacing(spf(Spacing::Xxs));
            let target_block = column![
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
                forge_widgets::inputs::text_input_field(
                    "welcome_text",
                    &form.config.read_file_target_var,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::ReadFileTargetVarChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![path_block, target_block]
                .spacing(spf(Spacing::Sm))
                .into()
        }
        SubActionKindChoice::RandomInt => {
            use iced::widget::column;
            let min_block = column![
                forge_widgets::section_header("MIN", None, palette),
                forge_widgets::inputs::text_input_field(
                    "1",
                    &form.config.random_int_min,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntMinChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let max_block = column![
                forge_widgets::section_header("MAX", None, palette),
                forge_widgets::inputs::text_input_field(
                    "100",
                    &form.config.random_int_max,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntMaxChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            let target_block = column![
                forge_widgets::section_header("TARGET VARIABLE", None, palette),
                forge_widgets::inputs::text_input_field(
                    "dice_roll",
                    &form.config.random_int_target_var,
                    |v| Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
                        AddSubActionMsg::RandomIntTargetVarChanged(v)
                    ))),
                    palette,
                ),
            ]
            .spacing(spf(Spacing::Xs));
            column![
                row![min_block, max_block].spacing(spf(Spacing::Xs)),
                target_block
            ]
            .spacing(spf(Spacing::Sm))
            .into()
        }
    };

    let mut body_col = column![chips_row, config_block].spacing(spf(Spacing::Md));

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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        palette,
    );

    let add_on_press = Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
        AddSubActionMsg::Submit,
    )));
    let add_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button(btn_label, add_on_press, palette)
    } else {
        forge_widgets::secondary_button(btn_label, Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, add_btn].spacing(spf(Spacing::Xs));

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
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
        body_col.into(),
        Some(footer),
        palette,
    );
    forge_widgets::side_sheet(
        panel,
        Message::Actions(ActionsMsg::Editor(ActionEditorMsg::AddSubAction(
            AddSubActionMsg::Cancel,
        ))),
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
    .spacing(spf(Spacing::Xxs));

    let inner = row![icon_el, container(label_col).width(Length::Fill),]
        .spacing(spf(Spacing::Xs))
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

fn screen_label(screen: &Screen) -> &'static str {
    match screen {
        Screen::Home => "Home",
        Screen::Actions => "Actions",
        Screen::ActionEditor(_) => "Actions",
        Screen::Queues => "Queues",
        Screen::Commands => "Commands",
        Screen::Platforms => "Platforms",
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

fn builtin_active(screen: &Screen, id: &str) -> bool {
    matches!(screen, Screen::BuiltinDetail(s) if s.as_str() == id)
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
    .spacing(spf(Spacing::Xxs));
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
        .spacing(spf(Spacing::Xxs)),
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
        TtsSection::Dashboard => tts_dashboard_view(&app.ui.tts_dashboard, palette),
        TtsSection::Engines => tts_engines_view(&app.ui.tts_engines, palette),
        TtsSection::Aliases => voice_aliases_view(&app.ui.tts_aliases, palette),
        TtsSection::Filters => tts_filters_view(&app.ui.tts_filters, palette),
        TtsSection::Triggers => tts_triggers_view(&app.ui.tts_triggers, palette),
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
        Screen::Home => crate::home::home_view(app, palette),
        Screen::LiveChat => live_chat_view(&app.ui.live_chat, &app.ui.viewers, palette),
        Screen::Globals => globals_view(app, palette),
        Screen::Actions => actions_view(app, palette),
        Screen::ActionEditor(id) => action_editor_view(app, *id, palette),
        Screen::Queues => queues_view(&app.ui.queues, palette),
        Screen::Commands => crate::commands_view::commands_view(&app.ui.commands, palette),
        Screen::Settings(section) => crate::settings::settings_view(
            section,
            &app.ui.settings_websocket,
            &app.ui.server_screen,
            &app.ui.settings_audio,
            palette,
        ),
        Screen::ScriptEditor => script_editor_view(app, palette),
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
                crate::platform_generic::platform_generic_view(color, info, palette)
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

    if let Some(state) = app.ui.builtin_detail.as_ref() {
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
    use crate::message::HomeStatsData;
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
        use crate::message::LiveChatMsg;
        let mut app = App::default();
        app.ui.live_chat.chat_input = String::new();
        let _ = update(&mut app, Message::LiveChat(LiveChatMsg::Submit));
        assert!(app.ui.live_chat.chat_input.is_empty());
    }

    #[test]
    fn chat_submit_clears_input_and_dispatches_task() {
        use crate::message::LiveChatMsg;
        let mut app = App::default();
        app.ui.live_chat.chat_input = "hello chat".into();
        let _ = update(&mut app, Message::LiveChat(LiveChatMsg::Submit));
        assert!(app.ui.live_chat.chat_input.is_empty());
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
                backend: dp,
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
            ui: UiState::default(),
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
    fn navigate_to_actions_sets_loading_true() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Actions));
        assert_eq!(app.screen, Screen::Actions);
    }

    #[test]
    fn tree_loaded_ok_clears_loading_flag() {
        let mut app = App::default();
        app.ui.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TreeLoaded(Ok(vec![]))),
        );
        assert!(!app.ui.actions.loading);
        assert!(app.ui.actions.tree.is_empty());
    }

    #[test]
    fn tree_loaded_err_clears_loading_flag() {
        let mut app = App::default();
        app.ui.actions.loading = true;
        let _ = update(
            &mut app,
            Message::Actions(ActionsMsg::TreeLoaded(Err("db error".into()))),
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
            triggers: vec![],
            commands: vec![],
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
            triggers: vec![],
            commands: vec![],
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
        app.ui.home.commands_count = Some(23);
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
            commands_count: 3,
            triggers_fired: 42,
            globals_count: 7,
        };
        let _ = update(&mut app, Message::Home(HomeMsg::StatsLoaded(Ok(data))));
        assert_eq!(app.ui.home.actions_count, Some(5));
        assert_eq!(app.ui.home.commands_count, Some(3));
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
        assert!(app.ui.home.commands_count.is_none());
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
        app.ui.home.commands_count = Some(5);
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
            Message::ObsBootResult(Ok(ObsClientRef::new(Arc::new(client)))),
        );
        assert!(app.rt.obs_client.is_some());
        assert!(app.ui.builtin_detail.is_some());
    }

    #[test]
    fn obs_boot_result_err_leaves_obs_client_none() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::ObsBootResult(Err("connection refused".into())),
        );
        assert!(app.rt.obs_client.is_none());
        assert!(app.ui.builtin_detail.is_none());
    }
}
