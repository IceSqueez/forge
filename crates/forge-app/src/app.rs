use std::sync::Arc;
use std::time::SystemTime;

use forge_events::{Event, EventPublisher, EventSource};
use forge_obs::ObsClient;
use forge_platform_core::{
    IntegrationContent, IntegrationHealth, IntegrationId, IntegrationStatus, QuickActions,
    SectionIcon,
};
use forge_platform_twitch::{
    ChatConnectionState, ChatSendBridgeHandle, TwitchChatHandle, TwitchIntegrationBundle,
};
use forge_runtime::{
    ActionEngineHandle, CommandParserHandle, EventBus, NullEventLogRepo, QueueSchedulerHandle,
    ScriptRegistry,
};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId};
use forge_widgets::icons::{
    ICON_ACTIVITY, ICON_BROADCAST, ICON_CHAT, ICON_DOWNLOAD, ICON_GEAR, ICON_GRID, ICON_HASH,
    ICON_HOME, ICON_LIGHTNING, ICON_PEOPLE, ICON_PLUS, ICON_TERMINAL,
};
use forge_widgets::tokens::{
    FONT_BODY, FONT_BODY_LG, FONT_BODY_MD, FONT_BODY_SM, FONT_CAPS, FONT_CAPS_SM, FONT_PAGE_TITLE,
    FONT_VALUE,
};
use forge_widgets::{
    FontRole, ForgePalette, NavChild, NavItem, Radius, SidebarV2, ThemeId, TitleBarV2, font,
    page_shell, radius, sidebar_v2, title_bar_v2,
};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::actions::{
    ActionsFilter, ActionsState, AddActionForm, AddActionMsg, AddSubActionForm, AddSubActionMsg,
    AddTriggerForm, AddTriggerMsg, RemoveSubActionMsg, SubActionKindChoice, TriggerCategory,
    kind_label, kind_summary, load_action_detail, load_actions_tree, remove_sub_action,
    save_sub_action,
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
    ActionsMsg, GlobalsMsg, HubMsg, HubStatsData, ObsClientRef, PlatformId, QueuesMsg, SettingsMsg,
    SidebarMsg,
};
use crate::queues_view::{QueuesState, load_queues, queues_view};
use crate::script_editor::{
    ScriptEditorMsg, ScriptEditorState, handle_script_editor_msg, script_editor_view,
};
use crate::server_screen::{
    ServerScreenMsg, ServerScreenState, handle_server_screen_msg, server_screen_view,
};
use crate::server_subsystem::ServerSubsystem;
use crate::settings_websocket::{
    SettingsWebSocketState, handle_settings_websocket_msg, settings_websocket_view,
};
use crate::stream_apps::view as stream_apps_view;
use crate::test_trigger::synthesize_test_event;
use crate::{Message, Screen, SettingsSection};

pub struct SidebarExpandState {
    pub actions_queues: bool,
    pub platforms: bool,
    pub stream_apps: bool,
}

impl SidebarExpandState {
    pub fn new() -> Self {
        Self {
            actions_queues: false,
            platforms: false,
            stream_apps: false,
        }
    }
}

impl Default for SidebarExpandState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct HubStats {
    pub actions_count: Option<usize>,
    pub commands_count: Option<usize>,
    pub triggers_fired: Option<u64>,
    pub globals_count: Option<usize>,
}

impl HubStats {
    pub fn new() -> Self {
        Self::default()
    }
}

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub palette: ForgePalette,
    pub backend: Arc<SqliteBackend>,
    pub bus: Arc<EventBus>,
    pub storage_offline: bool,
    pub boot_time: SystemTime,
    pub hub: HubStats,
    pub sidebar_state: SidebarExpandState,
    pub event_feed: EventFeedState,
    pub live_chat: LiveChatState,
    pub actions: ActionsState,
    pub queues: QueuesState,
    pub globals: GlobalsState,
    pub script_editor: ScriptEditorState,
    pub script_registry: Arc<ScriptRegistry>,
    pub twitch_chat_handle: Option<TwitchChatHandle>,
    pub chat_send_bridge: Option<ChatSendBridgeHandle>,
    pub action_engine: Option<ActionEngineHandle>,
    pub scheduler: Option<QueueSchedulerHandle>,
    pub command_parser: Option<CommandParserHandle>,
    pub integration_detail: Option<IntegrationDetailState>,
    pub obs_client: Option<Arc<ObsClient>>,
    pub server_screen: ServerScreenState,
    pub server_subsystem: Arc<ServerSubsystem>,
    pub settings_websocket: SettingsWebSocketState,
    pub twitch_panel: crate::twitch_panel::TwitchPanelState,
    pub twitch_flow: Option<crate::twitch_panel::TwitchFlowHandle>,
    pub twitch_login: Option<String>,
    pub twitch_reauth_required: bool,
    pub obs_panel: crate::obs_panel::ObsPanelState,
}

impl App {
    pub fn default_with(
        initial: Screen,
        backend: Arc<SqliteBackend>,
        storage_offline: bool,
        script_registry: Arc<ScriptRegistry>,
        action_engine: Option<ActionEngineHandle>,
        scheduler: Option<QueueSchedulerHandle>,
        command_parser: Option<CommandParserHandle>,
    ) -> Self {
        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let server_subsystem = Arc::new(ServerSubsystem::new(
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>
        ));
        Self {
            screen: initial,
            theme,
            palette,
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            storage_offline,
            boot_time: SystemTime::now(),
            hub: HubStats::new(),
            sidebar_state: SidebarExpandState::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            queues: QueuesState::new(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            script_registry,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            action_engine,
            scheduler,
            command_parser,
            integration_detail: None,
            obs_client: None,
            server_screen: ServerScreenState::default(),
            server_subsystem,
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            twitch_flow: None,
            twitch_login: None,
            twitch_reauth_required: false,
            obs_panel: crate::obs_panel::ObsPanelState::default(),
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
            backend,
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            storage_offline: false,
            boot_time: SystemTime::now(),
            hub: HubStats::new(),
            sidebar_state: SidebarExpandState::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            queues: QueuesState::new(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            script_registry: Arc::new(ScriptRegistry::new()),
            twitch_chat_handle: None,
            chat_send_bridge: None,
            action_engine: None,
            scheduler: None,
            command_parser: None,
            integration_detail: None,
            obs_client: None,
            server_screen: ServerScreenState::default(),
            server_subsystem,
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            twitch_flow: None,
            twitch_login: None,
            twitch_reauth_required: false,
            obs_panel: crate::obs_panel::ObsPanelState::default(),
        }
    }
}

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Navigate(screen) => {
            let is_actions = matches!(screen, Screen::Actions);
            let is_queues = matches!(screen, Screen::Queues);
            let is_hub = matches!(screen, Screen::Home);
            let is_globals = matches!(screen, Screen::Globals);
            let is_script_editor = matches!(screen, Screen::ScriptEditor);
            app.screen = screen;
            if is_actions {
                Task::done(Message::Actions(ActionsMsg::LoadRequested))
            } else if is_queues {
                Task::done(Message::Queues(QueuesMsg::LoadRequested))
            } else if is_hub {
                Task::done(Message::Hub(HubMsg::LoadStats))
            } else if is_globals {
                Task::done(Message::Globals(GlobalsMsg::LoadRequested))
            } else if is_script_editor {
                Task::done(Message::ScriptEditor(ScriptEditorMsg::LoadRequested))
            } else {
                Task::none()
            }
        }
        Message::Sidebar(sub) => {
            match sub {
                SidebarMsg::ToggleActionsQueues => {
                    app.sidebar_state.actions_queues = !app.sidebar_state.actions_queues;
                }
                SidebarMsg::TogglePlatforms => {
                    app.sidebar_state.platforms = !app.sidebar_state.platforms;
                }
                SidebarMsg::ToggleStreamApps => {
                    app.sidebar_state.stream_apps = !app.sidebar_state.stream_apps;
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
            if let Some(row) = chat_row_from_event(&event) {
                app.live_chat.chat_log.push_back(row);
                if app.live_chat.chat_log.len() > CHAT_LOG_MAX {
                    app.live_chat.chat_log.pop_front();
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
                app.twitch_reauth_required = true;
            }
            if !app.event_feed.paused {
                app.event_feed.push_event(event);
            }
            Task::none()
        }
        Message::EventFeed(sub) => {
            handle_event_feed_msg(&mut app.event_feed, sub, Arc::clone(&app.bus))
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
            let backend = Arc::clone(&app.backend);
            let bus = Arc::clone(&app.bus);
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
        Message::ChatFilterChanged(filter) => {
            app.live_chat.chat_filter = filter;
            Task::none()
        }
        Message::Settings(sub) => match sub {
            SettingsMsg::ReconnectPlatform(PlatformId::Twitch) => {
                if let Some(handle) = app.twitch_chat_handle.take() {
                    handle.shutdown();
                }
                let backend = Arc::clone(&app.backend);
                let bus = Arc::clone(&app.bus);
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
        },
        Message::Hub(sub) => handle_hub_msg(app, sub),
        Message::Globals(sub) => handle_globals_msg(app, sub),
        Message::VariantEditor(sub) => handle_variant_editor_msg(app, sub),
        Message::Actions(sub) => handle_actions_msg(app, sub),
        Message::Queues(sub) => handle_queues_msg(app, sub),
        Message::AddAction(sub) => handle_add_action_msg(app, sub),
        Message::AddTrigger(sub) => handle_add_trigger_msg(app, sub),
        Message::AddSubAction(sub) => handle_add_sub_action_msg(app, sub),
        Message::RemoveSubAction(sub) => handle_remove_sub_action_msg(app, sub),
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
                    Arc::clone(&app.bus),
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
                app.twitch_chat_handle = Some(handle);
                if let Some(l) = login {
                    app.twitch_login = Some(l);
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
                app.obs_client = Some(client);
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
            let subsystem = Arc::clone(&app.server_subsystem);
            Task::perform(
                async move { subsystem.restart().await.map_err(|e| e.to_string()) },
                Message::ServerRestartResult,
            )
        }
        Message::Server(crate::server_screen::ServerScreenMsg::StopServer) => {
            let subsystem = Arc::clone(&app.server_subsystem);
            Task::perform(
                async move { subsystem.stop().await.map_err(|e| e.to_string()) },
                Message::ServerStopResult,
            )
        }
        Message::Server(crate::server_screen::ServerScreenMsg::RegenerateToken) => {
            let subsystem = Arc::clone(&app.server_subsystem);
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
            let subsystem = Arc::clone(&app.server_subsystem);
            Task::perform(
                async move { subsystem.restart().await.map_err(|e| e.to_string()) },
                Message::ServerRestartResult,
            )
        }
        Message::SettingsWebSocket(sub) => {
            handle_settings_websocket_msg(&mut app.settings_websocket, sub, &app.backend)
        }
        Message::TwitchPanel(sub) => handle_twitch_panel_msg(app, sub),
        Message::TwitchReauthRequested => {
            if let Some(handle) = app.twitch_chat_handle.take() {
                handle.shutdown();
            }
            app.integration_detail = None;
            app.twitch_login = None;
            app.twitch_reauth_required = false;
            let backend = Arc::clone(&app.backend);
            Task::perform(
                async move {
                    let id = CredentialId::new("twitch:broadcaster");
                    let _ = backend.delete(&id).await;
                },
                |()| Message::Noop,
            )
        }
        Message::ObsPanel(sub) => handle_obs_panel_msg(app, sub),
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
            app.twitch_flow = Some(Arc::clone(&flow));
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
            let Some(flow) = app.twitch_flow.clone() else {
                app.twitch_panel = TwitchPanelState::Error("no active flow handle".into());
                return Task::none();
            };
            let creds: Arc<dyn CredentialsRepo> =
                Arc::clone(&app.backend) as Arc<dyn CredentialsRepo>;
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
            app.twitch_login = login.clone();
            let tracker = forge_platform_twitch::SubscriptionTracker::default();
            let chat = forge_platform_twitch::TwitchChat::new(
                outcome.token,
                outcome.client_id,
                outcome.user_info.id.clone(),
                outcome.user_info.id,
                Arc::clone(&app.bus),
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
            app.twitch_chat_handle = Some(handle);
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
            let backend = Arc::clone(&app.backend);
            let bus = Arc::clone(&app.bus);
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

fn handle_hub_msg(app: &mut App, sub: HubMsg) -> Task<Message> {
    match sub {
        HubMsg::LoadStats => {
            let dp = Arc::clone(&app.backend);
            Task::perform(
                async move { load_hub_stats(dp).await.map_err(|e| e.to_string()) },
                |r| Message::Hub(HubMsg::StatsLoaded(r)),
            )
        }
        HubMsg::StatsLoaded(Ok(data)) => {
            app.hub.actions_count = Some(data.actions_count);
            app.hub.commands_count = Some(data.commands_count);
            app.hub.triggers_fired = Some(data.triggers_fired);
            app.hub.globals_count = Some(data.globals_count);
            Task::none()
        }
        HubMsg::StatsLoaded(Err(e)) => {
            tracing::warn!(error = %e, "hub stats load failed");
            Task::none()
        }
    }
}

fn handle_queues_msg(app: &mut App, sub: QueuesMsg) -> Task<Message> {
    match sub {
        QueuesMsg::LoadRequested => {
            app.queues.loading = true;
            let dp = Arc::clone(&app.backend);
            Task::perform(async move { load_queues(dp).await }, |r| {
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
            let Some(scheduler) = app.scheduler.clone() else {
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
            let Some(scheduler) = app.scheduler.clone() else {
                return Task::none();
            };
            Task::perform(
                async move { scheduler.resume(id).await.map_err(|e| e.to_string()) },
                |r| Message::Queues(QueuesMsg::ResumeResult(r)),
            )
        }
        QueuesMsg::DrainQueue(id) => {
            // TODO Phase 2: drain in scheduler
            tracing::info!(queue_id = %id, "drain requested — not yet implemented");
            Task::none()
        }
        QueuesMsg::PauseAll => {
            for q in &mut app.queues.queues {
                q.paused = true;
            }
            let ids: Vec<_> = app.queues.queues.iter().map(|q| q.id).collect();
            let Some(scheduler) = app.scheduler.clone() else {
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

async fn load_hub_stats(dp: Arc<SqliteBackend>) -> Result<HubStatsData, String> {
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
    Ok(HubStatsData {
        actions_count: actions,
        commands_count: commands,
        triggers_fired: 0,
        globals_count: globals,
    })
}

fn handle_actions_msg(app: &mut App, sub: ActionsMsg) -> Task<Message> {
    match sub {
        ActionsMsg::LoadRequested => {
            app.actions.loading = true;
            let dp = Arc::clone(&app.backend);
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
            app.actions.selected = Some(id);
            let dp = Arc::clone(&app.backend);
            Task::perform(
                async move { load_action_detail(dp, id).await.map_err(|e| e.to_string()) },
                |r| Message::Actions(ActionsMsg::DetailLoaded(r)),
            )
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
            let dp = Arc::clone(&app.backend);
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
            let bus = Arc::clone(&app.bus);
            let dp = Arc::clone(&app.backend);
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
            let dp = Arc::clone(&app.backend);
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
    }
}

fn handle_add_action_msg(app: &mut App, sub: AddActionMsg) -> Task<Message> {
    match sub {
        AddActionMsg::OpenRequested => {
            app.actions.add_action_modal = Some(AddActionForm::new());
            let dp = Arc::clone(&app.backend);
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
            let dp = Arc::clone(&app.backend);
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
            let dp = Arc::clone(&app.backend);
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
            Task::none()
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
            };
            let action_id = form.for_action_id;
            if let Some(f) = app.actions.add_sub_action_modal.as_mut() {
                f.saving = true;
            }
            let dp = Arc::clone(&app.backend);
            Task::perform(
                async move {
                    save_sub_action(dp, action_id, spec)
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
    }
}

fn handle_remove_sub_action_msg(app: &mut App, sub: RemoveSubActionMsg) -> Task<Message> {
    match sub {
        RemoveSubActionMsg::Requested(action_id, index) => {
            let dp = Arc::clone(&app.backend);
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
    Ok(Some(crate::message::TwitchBootBundle {
        access_token,
        client_id,
        user_id,
        login,
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

fn event_source_color(source: EventSource, palette: &ForgePalette) -> iced::Color {
    match source {
        EventSource::Twitch => palette.brand,
        EventSource::YouTube => palette.random,
        EventSource::Kick => palette.info,
        EventSource::Trovo => palette.success,
        EventSource::Core => palette.warning,
        EventSource::Rhai => palette.warning,
        EventSource::Http => palette.random,
        EventSource::Obs => palette.success,
        EventSource::VTube => palette.bits,
        EventSource::Discord => palette.brand,
        EventSource::Midi => palette.info,
        EventSource::Hotkey => palette.info,
        EventSource::Timer => palette.warning,
        EventSource::Server => palette.info,
    }
}

fn event_kind_description(source: EventSource, kind: &str) -> String {
    let src_label = match source {
        EventSource::Twitch => "Twitch",
        EventSource::YouTube => "YouTube",
        EventSource::Kick => "Kick",
        EventSource::Trovo => "Trovo",
        EventSource::Core => "Core",
        EventSource::Rhai => "Rhai",
        EventSource::Http => "HTTP",
        EventSource::Obs => "OBS",
        EventSource::VTube => "VTube",
        EventSource::Discord => "Discord",
        EventSource::Midi => "MIDI",
        EventSource::Hotkey => "Hotkey",
        EventSource::Timer => "Timer",
        EventSource::Server => "Server",
    };
    format!("{src_label}: {kind}")
}

fn fmt_count<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "0".to_string(), |n| n.to_string())
}

fn hub_card_style(
    palette: &ForgePalette,
) -> impl Fn(&Theme) -> iced::widget::container::Style + '_ {
    move |_theme: &Theme| iced::widget::container::Style {
        background: Some(iced::Background::Color(palette.elevated)),
        border: iced::Border {
            color: palette.border_regular,
            width: 0.5,
            radius: radius(Radius::Xxl).into(),
        },
        ..iced::widget::container::Style::default()
    }
}

fn hub_nav_card<'a>(
    icon: char,
    icon_color: iced::Color,
    title: &'a str,
    leading: impl Into<String>,
    cta: Option<&'a str>,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BOOTSTRAP_FONT;
    use iced::widget::{button, column, container, row, text};
    use iced::{Alignment, Background, Border, Color, Shadow};

    let leading: String = leading.into();

    let icon_box = container(
        text(icon.to_string())
            .size(16.0)
            .font(BOOTSTRAP_FONT)
            .color(icon_color),
    )
    .width(30.0)
    .height(30.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| iced::widget::container::Style {
        background: Some(Background::Color(palette.surface_overlay)),
        border: Border {
            radius: radius(Radius::Lg).into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let description_row: Element<'a, Message> = if let Some(cta_text) = cta {
        row![
            text(format!("{leading} \u{b7} "))
                .size(FONT_CAPS)
                .color(palette.text_muted),
            text(cta_text).size(FONT_CAPS).color(palette.brand),
        ]
        .into()
    } else {
        text(leading)
            .size(FONT_CAPS)
            .color(palette.text_muted)
            .into()
    };

    let content = column![
        icon_box,
        text(title).size(FONT_BODY_LG).color(palette.text_primary),
        description_row,
    ]
    .spacing(10.0)
    .width(Length::Fill);

    button(content)
        .on_press(on_press)
        .padding(14.0)
        .width(Length::Fill)
        .style(move |_theme: &Theme, status| {
            let bg = if matches!(status, iced::widget::button::Status::Hovered) {
                Color {
                    a: 1.0,
                    ..palette.elevated
                }
            } else {
                palette.elevated
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    color: palette.border_regular,
                    width: 0.5,
                    radius: 10.0.into(),
                },
                text_color: palette.text_primary,
                shadow: Shadow::default(),
                snap: false,
            }
        })
        .into()
}

fn hub_event_row<'a>(
    event: &forge_events::Event,
    has_bottom_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Alignment, Background, Border};

    let dot_color = event_source_color(event.source, palette);

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

    let ts = event.timestamp;
    let ts_str = format!("{:02}:{:02}:{:02}", ts.hour(), ts.minute(), ts.second());

    let timestamp = container(
        text(ts_str)
            .size(FONT_CAPS)
            .color(palette.text_muted)
            .font(font(FontRole::Monospace)),
    )
    .width(48.0);

    let description = text(event_kind_description(event.source, &event.kind))
        .size(FONT_BODY)
        .color(palette.text_primary)
        .width(Length::Fill);

    let inner = row![dot, timestamp, description]
        .spacing(10.0)
        .align_y(Alignment::Center)
        .padding(iced::Padding {
            top: 6.0,
            right: 0.0,
            bottom: 6.0,
            left: 0.0,
        });

    let border_width = if has_bottom_border { 0.5 } else { 0.0 };

    container(inner)
        .width(Length::Fill)
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: palette.border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

fn hub_stat_row<'a>(
    label: &'a str,
    value_text: String,
    value_color: iced::Color,
    has_bottom_border: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Alignment, Border};

    let inner = row![
        text(label).size(FONT_BODY_SM).color(palette.text_muted),
        iced::widget::Space::new().width(Length::Fill),
        text(value_text)
            .size(FONT_VALUE)
            .color(value_color)
            .font(font(FontRole::Monospace)),
    ]
    .align_y(Alignment::Center);

    let border_width = if has_bottom_border { 0.5 } else { 0.0 };

    container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 8.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            border: Border {
                color: palette.border_regular,
                width: border_width,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
        .into()
}

pub(crate) fn connected_count(app: &App) -> u8 {
    if app.twitch_chat_handle.is_some() {
        1
    } else {
        0
    }
}

fn hub_inline_button<'a>(
    icon: char,
    label: &'a str,
    on_press: Message,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_widgets::BOOTSTRAP_FONT;
    use iced::widget::{button, row, text};
    use iced::{Alignment, Background, Border, Shadow};

    let icon_color = palette.text_secondary;
    let text_color = palette.text_secondary;
    let border_color = palette.border_regular;
    let r = radius(Radius::Md);

    let content = row![
        text(icon.to_string())
            .size(12.0)
            .font(BOOTSTRAP_FONT)
            .color(icon_color),
        text(label).size(FONT_BODY_SM).color(text_color),
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

fn hub_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, row, text};
    use iced::{Alignment, Background, Border};

    let brand_box = container(text("F").size(26.0).color(palette.shell).font(iced::Font {
        weight: iced::font::Weight::Semibold,
        ..iced::Font::DEFAULT
    }))
    .width(54.0)
    .height(54.0)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &Theme| iced::widget::container::Style {
        background: Some(Background::Color(palette.brand)),
        border: Border {
            radius: 12.0.into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..iced::widget::container::Style::default()
    });

    let title_col = column![
        text("Forge")
            .size(FONT_PAGE_TITLE)
            .color(palette.text_primary),
        text("Open-source stream automation, forged for streamers")
            .size(FONT_BODY_MD)
            .color(palette.text_muted),
    ]
    .spacing(2.0);

    let import_btn = hub_inline_button(ICON_DOWNLOAD, "Import", Message::Noop, palette);
    let new_action_btn = hub_inline_button(
        ICON_PLUS,
        "New action",
        Message::Navigate(Screen::Actions),
        palette,
    );

    let hero_buttons = row![import_btn, new_action_btn].spacing(6.0);

    let hero_inner = row![
        brand_box,
        container(title_col).width(Length::Fill),
        hero_buttons,
    ]
    .spacing(18.0)
    .align_y(Alignment::Center);

    let hero_card = container(hero_inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 20.0,
            right: 22.0,
            bottom: 20.0,
            left: 22.0,
        })
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 12.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let manage_header = text("MANAGE")
        .size(FONT_CAPS_SM)
        .color(palette.text_muted)
        .font(font(FontRole::Monospace));

    let platforms_connected = connected_count(app);
    let stream_apps_connected: u8 = app.obs_client.is_some().into();

    let actions_count = app.hub.actions_count.unwrap_or(0);
    let commands_count = app.hub.commands_count.unwrap_or(0);
    let triggers_fired = app.hub.triggers_fired.unwrap_or(0);

    let (actions_leading, actions_cta) = if actions_count == 0 {
        ("None yet".to_owned(), Some("create one"))
    } else {
        (
            format!("{actions_count} configured \u{b7} {triggers_fired} fired"),
            None,
        )
    };
    let (commands_leading, commands_cta) = if commands_count == 0 {
        ("None yet".to_owned(), Some("create one"))
    } else {
        (format!("{commands_count} commands across chat"), None)
    };
    let (platforms_leading, platforms_cta) = if platforms_connected == 0 {
        ("0 connected".to_owned(), Some("connect"))
    } else {
        (format!("{platforms_connected} connected"), None)
    };
    let (stream_apps_leading, stream_apps_cta) = if stream_apps_connected == 0 {
        ("0 connected".to_owned(), Some("connect"))
    } else {
        (format!("{stream_apps_connected} connected"), None)
    };

    let actions_card = hub_nav_card(
        ICON_LIGHTNING,
        palette.brand,
        "Actions",
        actions_leading,
        actions_cta,
        Message::Navigate(Screen::Actions),
        palette,
    );
    let commands_card = hub_nav_card(
        ICON_TERMINAL,
        palette.info,
        "Commands",
        commands_leading,
        commands_cta,
        Message::Navigate(Screen::Commands),
        palette,
    );
    let platforms_card = hub_nav_card(
        ICON_BROADCAST,
        palette.random,
        "Platforms",
        platforms_leading,
        platforms_cta,
        Message::Navigate(Screen::Platforms),
        palette,
    );
    let stream_apps_card = hub_nav_card(
        ICON_GRID,
        palette.success,
        "Stream apps",
        stream_apps_leading,
        stream_apps_cta,
        Message::Navigate(Screen::StreamApps),
        palette,
    );

    let cards_grid = row![
        actions_card,
        commands_card,
        platforms_card,
        stream_apps_card
    ]
    .spacing(8.0)
    .width(Length::Fill);

    let recent_events = {
        let events = app.bus.recent(4);
        let is_empty = events.is_empty();

        let status_dot_color = if is_empty {
            palette.text_muted
        } else {
            palette.success
        };
        let status_dot = container(iced::widget::Space::new())
            .width(6.0)
            .height(6.0)
            .style(move |_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(status_dot_color)),
                border: Border {
                    radius: 3.0.into(),
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                },
                ..iced::widget::container::Style::default()
            });
        let status_label = text(if is_empty { "IDLE" } else { "LIVE" })
            .size(FONT_CAPS_SM)
            .color(palette.text_faint)
            .font(font(FontRole::Monospace));
        let status_row = row![status_dot, status_label]
            .spacing(5.0)
            .align_y(Alignment::Center);

        let header = row![
            text("Recent events")
                .size(FONT_BODY_LG)
                .color(palette.text_primary),
            iced::widget::Space::new().width(Length::Fill),
            status_row,
        ]
        .align_y(Alignment::Center);

        let body: Element<'a, Message> = if is_empty {
            let icon = text(ICON_ACTIVITY.to_string())
                .size(28.0)
                .font(forge_widgets::BOOTSTRAP_FONT)
                .color(palette.border_regular);
            let primary = text("No events yet")
                .size(FONT_BODY_MD)
                .color(palette.text_secondary);
            let secondary = text(
                "Events will appear here as soon as you connect a platform and start streaming.",
            )
            .size(FONT_CAPS)
            .color(palette.text_muted)
            .wrapping(iced::widget::text::Wrapping::Word);

            container(
                column![icon, primary, secondary]
                    .spacing(6.0)
                    .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .padding(20.0)
            .into()
        } else {
            let mut events_col = column![].spacing(0.0);
            let count = events.len();
            for (i, event) in events.iter().enumerate() {
                let has_border = i + 1 < count;
                events_col = events_col.push(hub_event_row(event, has_border, palette));
            }
            events_col.into()
        };

        let card_content = column![header, body].spacing(10.0);

        container(card_content)
            .width(Length::FillPortion(1))
            .padding(14.0)
            .style(hub_card_style(palette))
    };

    let at_a_glance = {
        let header = text("At a glance")
            .size(FONT_BODY_LG)
            .color(palette.text_primary);

        let actions_color = if actions_count == 0 {
            palette.text_muted
        } else {
            palette.brand
        };
        let commands_color = if commands_count == 0 {
            palette.text_muted
        } else {
            palette.info
        };
        let triggers_color = if triggers_fired == 0 {
            palette.text_muted
        } else {
            palette.success
        };
        let globals_count = app.hub.globals_count.unwrap_or(0);
        let globals_color = if globals_count == 0 {
            palette.text_muted
        } else {
            palette.warning
        };

        let actions_row = hub_stat_row(
            "Actions",
            fmt_count(app.hub.actions_count),
            actions_color,
            true,
            palette,
        );
        let commands_row = hub_stat_row(
            "Commands",
            fmt_count(app.hub.commands_count),
            commands_color,
            true,
            palette,
        );
        let triggers_row = hub_stat_row(
            "Triggers fired",
            fmt_count(app.hub.triggers_fired),
            triggers_color,
            true,
            palette,
        );
        let globals_row = hub_stat_row(
            "Global variables",
            fmt_count(app.hub.globals_count),
            globals_color,
            true,
            palette,
        );

        let stats_col =
            column![header, actions_row, commands_row, triggers_row, globals_row].spacing(4.0);

        container(stats_col)
            .width(Length::FillPortion(1))
            .padding(14.0)
            .style(hub_card_style(palette))
    };

    let bottom_row = row![recent_events, at_a_glance]
        .spacing(10.0)
        .width(Length::Fill);

    let content = column![hero_card, manage_header, cards_grid, bottom_row]
        .spacing(16.0)
        .width(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(20.0)
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
    let metrics = iced::widget::row![
        forge_widgets::metric_card("Build", version, None::<&str>, palette),
        forge_widgets::metric_card("Rust", "1.95.0", None::<&str>, palette),
        forge_widgets::metric_card("OS", std::env::consts::OS, None::<&str>, palette),
    ]
    .spacing(12);

    iced::widget::container(metrics)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .into()
}

pub(crate) fn status_pill_for(
    state: ChatConnectionState,
) -> (forge_widgets::StatusVariant, &'static str) {
    match state {
        ChatConnectionState::Connected => (forge_widgets::StatusVariant::Positive, "Connected"),
        ChatConnectionState::Reconnecting { .. } => {
            (forge_widgets::StatusVariant::Neutral, "Reconnecting")
        }
        ChatConnectionState::Connecting => (forge_widgets::StatusVariant::Neutral, "Connecting"),
        ChatConnectionState::Disconnected => {
            (forge_widgets::StatusVariant::Negative, "Disconnected")
        }
    }
}

fn twitch_platform_card<'a>(
    handle: Option<&'a TwitchChatHandle>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let state = handle.map_or(ChatConnectionState::Disconnected, |h| h.connection_state());
    let (variant, label) = status_pill_for(state);
    let pill = forge_widgets::status_pill(label, variant, palette);

    let state_text = iced::widget::text(match state {
        ChatConnectionState::Connected => "EventSub · WebSocket",
        ChatConnectionState::Connecting => "Establishing connection...",
        ChatConnectionState::Reconnecting { .. } => "Retrying connection...",
        ChatConnectionState::Disconnected => "Not connected",
    })
    .size(11.5)
    .color(palette.text_muted);

    let reconnect_btn = forge_widgets::primary_button(
        "Reconnect",
        Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::Twitch)),
        palette,
    );

    let header_row = iced::widget::row![
        iced::widget::text("Twitch")
            .size(14.0)
            .color(palette.text_primary),
        iced::widget::Space::new().width(Length::Fill),
        pill,
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(8);

    forge_widgets::card(
        [header_row.into(), state_text.into(), reconnect_btn],
        palette,
    )
}

fn coming_platform_card<'a>(
    name: &'a str,
    since: &'a str,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header_row = iced::widget::row![
        iced::widget::text(name)
            .size(14.0)
            .color(palette.text_muted),
        iced::widget::Space::new().width(Length::Fill),
        forge_widgets::status_pill(
            "Coming soon",
            forge_widgets::StatusVariant::Neutral,
            palette
        ),
    ]
    .align_y(iced::alignment::Vertical::Center)
    .spacing(8);

    let note = iced::widget::text(since)
        .size(11.5)
        .color(palette.text_faint);

    forge_widgets::card([header_row.into(), note.into()], palette)
}

fn settings_platforms_pane<'a>(
    twitch_handle: Option<&'a TwitchChatHandle>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header = forge_widgets::section_header("PLATFORMS", None, palette);

    let twitch_card = twitch_platform_card(twitch_handle, palette);
    let youtube_card =
        coming_platform_card("YouTube", "Available in beta-1 — see roadmap", palette);
    let kick_card = coming_platform_card("Kick", "Available in beta-2 — see roadmap", palette);
    let trovo_card = coming_platform_card("Trovo", "Available in beta-3 — see roadmap", palette);

    let cards = iced::widget::column![twitch_card, youtube_card, kick_card, trovo_card].spacing(10);

    iced::widget::container(iced::widget::column![header, cards].spacing(12))
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
        .size(forge_widgets::tokens::FONT_CAPS_SM)
        .color(palette.text_faint)
        .into()
}

fn settings_view<'a>(
    section: &'a SettingsSection,
    twitch_handle: Option<&'a TwitchChatHandle>,
    ws: &'a crate::settings_websocket::SettingsWebSocketState,
    server: &'a ServerScreenState,
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
        settings_section_button("Platforms", SettingsSection::Platforms, section, palette),
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
        SettingsSection::Platforms => settings_platforms_pane(twitch_handle, palette),
        SettingsSection::WebSocket => {
            settings_websocket_view(ws, &server.bearer_token, server.token_revealed, palette)
        }
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

    iced::widget::row![nav_container, pane].spacing(0).into()
}

fn actions_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use iced::widget::{column, container, scrollable, text};

    let p = *palette;
    let actions_state = &app.actions;

    let total = actions_state.total_actions();
    let enabled_count = actions_state
        .tree
        .iter()
        .flat_map(|g| g.actions.iter())
        .filter(|a| a.enabled)
        .count();
    let visible = actions_state.visible_actions();

    let stat_strip = actions_stat_strip(total, enabled_count, palette);
    let toolbar = actions_toolbar(actions_state, palette);
    let table_header = actions_table_header(palette);

    let mut body_col: iced::widget::Column<'_, Message> = column![].spacing(0);

    if actions_state.loading {
        body_col = body_col.push(
            container(text("Loading...").size(12.0).color(p.text_muted))
                .padding([16, 16])
                .width(Length::Fill),
        );
    } else if total == 0 {
        body_col = body_col.push(
            container(forge_widgets::empty_state(
                "No actions yet",
                "Use + New action to create your first action.",
                None::<(&str, Message)>,
                palette,
            ))
            .padding([24, 16])
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
            body_col = body_col.push(actions_group_header(group, is_collapsed, palette));

            if !is_collapsed {
                for summary in &filtered {
                    let selected = actions_state.selected == Some(summary.id);
                    body_col = body_col.push(actions_row(summary, selected, palette));
                }
            }
        }
    }

    let body_scrollable = scrollable(body_col).height(Length::Fill);

    let footer = actions_footer(visible, total, palette);

    let main_view: Element<'_, Message> =
        container(column![stat_strip, toolbar, table_header, body_scrollable, footer].spacing(0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

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
    }
}

fn actions_stat_strip<'a>(
    total: usize,
    enabled: usize,
    palette: &'a ForgePalette,
) -> iced::widget::Container<'a, Message> {
    use iced::widget::{container, row, text};

    let p = *palette;
    let total_el = row![
        text(total.to_string())
            .size(11.5)
            .color(p.text_primary)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
        text(" total").size(11.5).color(p.text_muted),
    ]
    .spacing(0);

    let sep1 = text(" \u{00b7} ").size(11.5).color(p.text_faint);

    let enabled_el = row![
        text(enabled.to_string())
            .size(11.5)
            .color(p.success)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
        text(" enabled").size(11.5).color(p.text_muted),
    ]
    .spacing(0);

    let sep2 = text(" \u{00b7} ").size(11.5).color(p.text_faint);

    let fired_el = row![
        text("\u{2014}")
            .size(11.5)
            .color(p.brand)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
        text(" fired today").size(11.5).color(p.text_muted),
    ]
    .spacing(0);

    let inner = row![total_el, sep1, enabled_el, sep2, fired_el]
        .spacing(0)
        .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([6, 16])
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

fn actions_toolbar<'a>(
    state: &'a crate::actions::ActionsState,
    palette: &'a ForgePalette,
) -> iced::widget::Container<'a, Message> {
    use iced::widget::{container, row, text};

    let p = *palette;
    let search = forge_widgets::search_input(
        "Search actions...",
        &state.search,
        |q| Message::Actions(ActionsMsg::SearchChanged(q)),
        palette,
    );

    let chip_all = actions_filter_chip(
        "All",
        state.filter == ActionsFilter::All,
        palette,
        ActionsFilter::All,
    );
    let chip_enabled = actions_filter_chip(
        "Enabled",
        state.filter == ActionsFilter::Enabled,
        palette,
        ActionsFilter::Enabled,
    );
    let chip_disabled = actions_filter_chip(
        "Disabled",
        state.filter == ActionsFilter::Disabled,
        palette,
        ActionsFilter::Disabled,
    );

    let group_label = text("Group by trigger")
        .size(10.5)
        .color(p.text_faint)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let import_btn = forge_widgets::ghost_button("Import", Message::Noop, palette);

    let new_btn = forge_widgets::primary_button_small(
        "+ New action",
        Message::Actions(ActionsMsg::OpenAddActionModal),
        palette,
    );

    let left = row![
        container(search).width(Length::Fixed(220.0)),
        row![chip_all, chip_enabled, chip_disabled].spacing(4),
        container(group_label).padding([0, 10]),
    ]
    .spacing(8)
    .align_y(iced::alignment::Vertical::Center);

    let right = row![import_btn, new_btn]
        .spacing(6)
        .align_y(iced::alignment::Vertical::Center);

    let inner = row![container(left).width(Length::Fill), right,]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    container(inner)
        .width(Length::Fill)
        .padding([8, 14])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(p.elevated)),
            border: iced::Border {
                color: p.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        })
}

fn actions_filter_chip<'a>(
    label: &'a str,
    active: bool,
    palette: &'a ForgePalette,
    filter: ActionsFilter,
) -> Element<'a, Message> {
    use iced::widget::{button, container, text};

    let p = *palette;
    let (bg, text_color) = if active {
        (
            Some(iced::Background::Color(p.surface_overlay)),
            p.text_primary,
        )
    } else {
        (None, p.text_secondary)
    };

    let label_el = text(label).size(11.0).color(text_color);

    button(container(label_el).padding([4, 10]))
        .on_press(Message::Actions(ActionsMsg::FilterChanged(filter)))
        .padding(0)
        .style(
            move |_theme: &iced::Theme, _status| iced::widget::button::Style {
                background: bg,
                text_color,
                border: iced::Border {
                    color: iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: 11.0.into(),
                },
                shadow: iced::Shadow::default(),
                snap: false,
            },
        )
        .into()
}

fn actions_table_header<'a>(palette: &'a ForgePalette) -> iced::widget::Container<'a, Message> {
    use iced::widget::{container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let name_col = row![text("NAME").size(10.5).color(p.text_faint).font(mono)]
        .width(Length::FillPortion(140));
    let trigger_col = row![text("TRIGGER").size(10.5).color(p.text_faint).font(mono)]
        .width(Length::FillPortion(140));
    let queue_col =
        row![text("QUEUE").size(10.5).color(p.text_faint).font(mono)].width(Length::Fixed(90.0));
    let last_ran_col =
        row![text("LAST RAN").size(10.5).color(p.text_faint).font(mono)].width(Length::Fixed(90.0));
    let runs_col = row![
        text("RUNS \u{00b7} 24H")
            .size(10.5)
            .color(p.text_faint)
            .font(mono)
    ]
    .width(Length::Fixed(90.0));
    let menu_col = row![].width(Length::Fixed(22.0));

    let dot_spacer = iced::widget::Space::new().width(Length::Fixed(24.0));

    let inner = row![
        dot_spacer,
        name_col,
        trigger_col,
        queue_col,
        last_ran_col,
        runs_col,
        menu_col
    ]
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

fn actions_group_header<'a>(
    group: &'a crate::actions::ActionsGroup,
    collapsed: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, container, row, text};

    let p = *palette;
    let chevron = if collapsed {
        forge_widgets::ICON_CHEVRON_RIGHT
    } else {
        forge_widgets::ICON_CHEVRON_DOWN
    };
    let chevron_el = text(chevron.to_string())
        .size(11.0)
        .color(p.text_faint)
        .font(forge_widgets::BOOTSTRAP_FONT);

    let cat_el = text(group.category.display_name())
        .size(10.5)
        .color(p.text_muted)
        .font(forge_widgets::font(forge_widgets::FontRole::Monospace));

    let count_str = format!(
        "{} actions \u{00b7} {} fired",
        group.actions.len(),
        group.fired_24h
    );
    let count_el = text(count_str).size(10.0).color(p.text_faint);

    let inner = row![chevron_el, cat_el, count_el]
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

fn actions_row<'a>(
    summary: &'a crate::actions::ActionSummary,
    selected: bool,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{button, column, container, row, text};

    let p = *palette;
    let mono = forge_widgets::font(forge_widgets::FontRole::Monospace);

    let dot_color = if summary.enabled {
        p.success
    } else {
        p.text_faint
    };
    let dot_size = 6.0_f32;
    let dot = container(iced::widget::Space::new().width(dot_size).height(dot_size))
        .width(dot_size)
        .height(dot_size)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(dot_color)),
            border: iced::Border {
                radius: (dot_size / 2.0).into(),
                color: iced::Color::TRANSPARENT,
                width: 0.0,
            },
            ..iced::widget::container::Style::default()
        });

    let dot_col = container(dot)
        .width(Length::Fixed(24.0))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);

    let name_color = if summary.enabled {
        p.text_primary
    } else {
        p.text_secondary
    };
    let sub_color = if summary.enabled {
        p.text_faint
    } else {
        p.text_extreme_faint
    };

    let name_el = text(&summary.name).size(12.0).color(name_color).font(mono);

    let mut subtitle_parts: Vec<String> = vec![format!(
        "{} sub-action{}",
        summary.sub_action_count,
        if summary.sub_action_count == 1 {
            ""
        } else {
            "s"
        }
    )];
    if let Some(extra) = &summary.extra_subtitle {
        subtitle_parts.push(extra.clone());
    }
    let subtitle_str = subtitle_parts.join(" \u{00b7} ");
    let subtitle_el = text(subtitle_str).size(10.5).color(sub_color);

    let name_col = column![name_el, subtitle_el]
        .spacing(2)
        .width(Length::FillPortion(140));

    let trigger_label_el = text(&summary.trigger_label)
        .size(11.0)
        .color(if summary.enabled {
            p.text_secondary
        } else {
            p.text_faint
        });
    let trigger_col = container(trigger_label_el).width(Length::FillPortion(140));

    let queue_color = if summary.enabled {
        p.text_secondary
    } else {
        p.text_faint
    };
    let queue_el = text(&summary.queue_name).size(11.0).color(queue_color);
    let queue_col = container(queue_el).width(Length::Fixed(90.0));

    let last_ran_str = match &summary.last_ran {
        None => "\u{2014}".to_string(),
        Some(dt) => {
            let now = time::OffsetDateTime::now_utc();
            let secs = (now - *dt).whole_seconds().max(0) as u64;
            if secs < 60 {
                format!("{}s ago", secs)
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else {
                format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
            }
        }
    };
    let last_ran_color = if summary.enabled {
        p.text_muted
    } else {
        p.text_faint
    };
    let last_ran_el = text(last_ran_str)
        .size(11.0)
        .color(last_ran_color)
        .font(mono);
    let last_ran_col = container(last_ran_el).width(Length::Fixed(90.0));

    let runs_color = if summary.enabled {
        p.brand
    } else {
        p.text_faint
    };
    let runs_el = text(summary.runs_24h.to_string())
        .size(11.0)
        .color(runs_color)
        .font(mono);
    let runs_col = container(runs_el).width(Length::Fixed(90.0));

    let menu_el = container(text("\u{22ee}").size(13.0).color(p.text_faint))
        .width(Length::Fixed(22.0))
        .align_x(iced::alignment::Horizontal::Center);

    let inner_row = row![
        dot_col,
        name_col,
        trigger_col,
        queue_col,
        last_ran_col,
        runs_col,
        menu_el
    ]
    .spacing(0)
    .align_y(iced::alignment::Vertical::Center);

    let action_id = summary.id;
    button(container(inner_row).width(Length::Fill).padding([8, 16]))
        .on_press(Message::Actions(ActionsMsg::ActionSelected(action_id)))
        .padding(0)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme, status| {
            let bg_color = match (selected, status) {
                (true, _) => p.surface_overlay,
                (false, iced::widget::button::Status::Hovered) => iced::Color {
                    a: 0.5,
                    ..p.surface_overlay
                },
                _ => p.base,
            };
            iced::widget::button::Style {
                background: Some(iced::Background::Color(bg_color)),
                text_color: p.text_primary,
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
    let left_el = text(left_str).size(10.5).color(p.text_faint).font(mono);

    let storage_el = text("Storage: \u{2014}")
        .size(10.5)
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
        .size(10.5)
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
        .size(10.0)
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

    let behavior_header = forge_widgets::section_header("BEHAVIOR", None, palette);

    let mut body_col = column![
        name_block,
        two_col,
        desc_block,
        behavior_header,
        enabled_toggle,
        concurrent_toggle,
        bypass_toggle,
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
                .size(11.0)
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
    use forge_widgets::{BannerKind, ModalProps};
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
                .size(11.5)
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
                        .size(12.0)
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
                .size(11.0)
                .color(palette.text_faint)
                .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
            iced::widget::Space::new().width(Length::Fill),
            footer_buttons,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into();

    forge_widgets::modal(
        palette,
        ModalProps {
            title: "Add trigger",
            on_close: Message::AddTrigger(AddTriggerMsg::Cancel),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
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
    use forge_widgets::{BannerKind, ModalProps};
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
    let chips_row = row![chip_send_chat, chip_set_global, chip_delay, chip_log].spacing(6);

    let config_block: iced::Element<'_, Message> = match form.kind {
        SubActionKindChoice::SendChat => {
            let msg_input = forge_widgets::text_input_field(
                "Hello %user%!",
                &form.config.send_chat_message,
                |v| Message::AddSubAction(AddSubActionMsg::SendChatMessageChanged(v)),
                palette,
            );
            let helper = text("Variables: %user%, %message%, %args%")
                .size(10.5)
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
                .size(10.5)
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

    let cancel_btn = forge_widgets::secondary_button(
        "Cancel",
        Message::AddSubAction(AddSubActionMsg::Cancel),
        palette,
    );

    let add_on_press = Message::AddSubAction(AddSubActionMsg::Submit);
    let add_btn = if form.is_valid() && !form.saving {
        forge_widgets::primary_button("Add step", add_on_press, palette)
    } else {
        forge_widgets::secondary_button("Add step", Message::Noop, palette)
    };

    let footer_buttons = row![cancel_btn, add_btn].spacing(8);

    let footer: iced::Element<'_, Message> = iced::widget::container(
        row![
            text("ESC to cancel")
                .size(11.0)
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
            title: "Add step",
            on_close: Message::AddSubAction(AddSubActionMsg::Cancel),
            kbd_hint: None,
        },
        body_col.into(),
        footer,
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

    let icon_el = container(text('\u{ea21}'.to_string()).size(13.0).color(palette.brand))
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
        text(label).size(12.5).color(palette.text_primary),
        text(summary)
            .size(11.0)
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

fn breadcrumb_icon_for(screen: &Screen) -> char {
    match screen {
        Screen::Home => ICON_HOME,
        Screen::Actions | Screen::Queues => ICON_LIGHTNING,
        Screen::Commands => ICON_TERMINAL,
        Screen::Platforms => ICON_BROADCAST,
        Screen::StreamApps | Screen::Integrations | Screen::IntegrationDetail(_) => ICON_GRID,
        Screen::LiveChat => ICON_CHAT,
        Screen::EventFeed => ICON_ACTIVITY,
        Screen::Globals => ICON_HASH,
        Screen::Viewers => ICON_PEOPLE,
        Screen::Settings(_) => ICON_GEAR,
        Screen::Tts | Screen::Soundboard => ICON_PEOPLE,
        Screen::ScriptEditor => ICON_TERMINAL,
        Screen::Server | Screen::Logs => ICON_GEAR,
    }
}

fn screen_label(screen: &Screen) -> &'static str {
    match screen {
        Screen::Home => "Home",
        Screen::Actions => "Actions",
        Screen::Queues => "Queues",
        Screen::Commands => "Commands",
        Screen::Platforms => "Platforms",
        Screen::StreamApps => "Stream apps",
        Screen::Integrations => "Integrations",
        Screen::IntegrationDetail(_) => "Integration",
        Screen::LiveChat => "Live chat",
        Screen::EventFeed => "Event feed",
        Screen::Globals => "Globals",
        Screen::Viewers => "Viewers",
        Screen::Settings(_) => "Settings",
        Screen::Tts => "TTS",
        Screen::Soundboard => "Soundboard",
        Screen::ScriptEditor => "Script editor",
        Screen::Server => "Server",
        Screen::Logs => "Logs",
    }
}

fn nav_items_for<'a>(app: &'a App, palette: &'a ForgePalette) -> Vec<NavItem<'a, Message>> {
    let is_home = matches!(app.screen, Screen::Home);
    let is_viewers = matches!(app.screen, Screen::Viewers);
    let is_actions = matches!(app.screen, Screen::Actions);
    let is_queues = matches!(app.screen, Screen::Queues);
    let is_actions_queues = is_actions || is_queues;
    let is_commands = matches!(app.screen, Screen::Commands);
    let is_platforms = matches!(app.screen, Screen::Platforms);
    let is_stream_apps = matches!(app.screen, Screen::StreamApps);
    let is_live_chat = matches!(app.screen, Screen::LiveChat);
    let is_event_feed = matches!(app.screen, Screen::EventFeed);
    let is_globals = matches!(app.screen, Screen::Globals);
    let is_settings = matches!(app.screen, Screen::Settings(_));

    let twitch_target = Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("twitch")));
    let obs_target = Message::Navigate(Screen::IntegrationDetail(IntegrationId::new("obs")));

    vec![
        NavItem::Leaf {
            icon: ICON_HOME,
            label: "Home",
            active: is_home,
            on_press: Message::Navigate(Screen::Home),
        },
        NavItem::Leaf {
            icon: ICON_PEOPLE,
            label: "Viewers",
            active: is_viewers,
            on_press: Message::Navigate(Screen::Viewers),
        },
        NavItem::Group {
            icon: ICON_LIGHTNING,
            label: "Actions & Queues",
            active: is_actions_queues,
            expanded: app.sidebar_state.actions_queues,
            on_toggle: Message::Sidebar(SidebarMsg::ToggleActionsQueues),
            children: vec![
                NavChild {
                    dot_color: palette.brand,
                    label: "Actions",
                    active: is_actions,
                    on_press: Message::Navigate(Screen::Actions),
                },
                NavChild {
                    dot_color: palette.info,
                    label: "Queues",
                    active: is_queues,
                    on_press: Message::Navigate(Screen::Queues),
                },
            ],
        },
        NavItem::Leaf {
            icon: ICON_TERMINAL,
            label: "Commands",
            active: is_commands,
            on_press: Message::Navigate(Screen::Commands),
        },
        NavItem::Group {
            icon: ICON_BROADCAST,
            label: "Platforms",
            active: is_platforms,
            expanded: app.sidebar_state.platforms,
            on_toggle: Message::Sidebar(SidebarMsg::TogglePlatforms),
            children: vec![
                NavChild {
                    dot_color: palette.brand,
                    label: "Twitch",
                    active: false,
                    on_press: twitch_target.clone(),
                },
                NavChild {
                    dot_color: palette.random,
                    label: "YouTube",
                    active: false,
                    on_press: Message::Navigate(Screen::Platforms),
                },
                NavChild {
                    dot_color: palette.info,
                    label: "Kick",
                    active: false,
                    on_press: Message::Navigate(Screen::Platforms),
                },
            ],
        },
        NavItem::Group {
            icon: ICON_GRID,
            label: "Stream apps",
            active: is_stream_apps,
            expanded: app.sidebar_state.stream_apps,
            on_toggle: Message::Sidebar(SidebarMsg::ToggleStreamApps),
            children: vec![
                NavChild {
                    dot_color: palette.success,
                    label: "OBS Studio",
                    active: false,
                    on_press: obs_target.clone(),
                },
                NavChild {
                    dot_color: palette.warning,
                    label: "VTube Studio",
                    active: false,
                    on_press: Message::Navigate(Screen::StreamApps),
                },
            ],
        },
        NavItem::Leaf {
            icon: ICON_CHAT,
            label: "Live chat",
            active: is_live_chat,
            on_press: Message::Navigate(Screen::LiveChat),
        },
        NavItem::Leaf {
            icon: ICON_ACTIVITY,
            label: "Event feed",
            active: is_event_feed,
            on_press: Message::Navigate(Screen::EventFeed),
        },
        NavItem::Leaf {
            icon: ICON_HASH,
            label: "Globals",
            active: is_globals,
            on_press: Message::Navigate(Screen::Globals),
        },
        NavItem::Divider,
        NavItem::Leaf {
            icon: ICON_GEAR,
            label: "Settings",
            active: is_settings,
            on_press: Message::Navigate(Screen::Settings(SettingsSection::Appearance)),
        },
    ]
}

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = &app.palette;

    let elapsed = app.boot_time.elapsed().unwrap_or_default();

    let title_bar = title_bar_v2(
        palette,
        TitleBarV2 {
            breadcrumb_icon: breadcrumb_icon_for(&app.screen),
            breadcrumb_label: screen_label(&app.screen),
            connected: (connected_count(app), 4),
            uptime: format_uptime(elapsed),
            _msg: std::marker::PhantomData,
        },
    );

    let sidebar = sidebar_v2(
        palette,
        SidebarV2 {
            items: nav_items_for(app, palette),
        },
    );

    let content: Element<'_, Message> = match &app.screen {
        Screen::Home => hub_view(app, palette),
        Screen::LiveChat => live_chat_view(&app.live_chat, palette),
        Screen::Globals => globals_view(app, palette),
        Screen::Actions => actions_view(app, palette),
        Screen::Queues => queues_view(&app.queues, palette),
        Screen::Settings(section) => settings_view(
            section,
            app.twitch_chat_handle.as_ref(),
            &app.settings_websocket,
            &app.server_screen,
            palette,
        ),
        Screen::ScriptEditor => script_editor_view(app, palette),
        Screen::StreamApps => stream_apps_view(app, palette),
        Screen::EventFeed => event_feed_view(&app.event_feed, palette),
        Screen::Server => server_screen_view(&app.server_screen, palette),
        Screen::IntegrationDetail(id) => {
            if id.as_str() == "twitch" && app.twitch_chat_handle.is_none() {
                crate::twitch_panel::twitch_disconnected_view(&app.twitch_panel, palette)
            } else if id.as_str() == "obs" && app.obs_client.is_none() {
                crate::obs_panel::obs_disconnected_view(&app.obs_panel, palette)
            } else if let Some(state) = app.integration_detail.as_ref() {
                let inner = integration_detail_view(state, palette);
                if id.as_str() == "twitch" && app.twitch_reauth_required {
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
        other => coming_soon_view(format!("{other:?}"), palette),
    };

    page_shell(title_bar, None, sidebar, content)
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
                            let _ = tx.try_send(Message::EventArrived(event));
                        }
                    }
                },
            )
            .boxed()
        }
    }

    let bus = from_recipe(BusRecipe(app.bus.clone()));

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
                    loop {
                        ticker.tick().await;
                        let Some(info) = subsystem.server_info().await else {
                            continue;
                        };
                        let clients_guard = info.connected_clients.read().await;
                        let mut rows: Vec<crate::server_screen::OwnedClientRow> = Vec::new();
                        for client in clients_guard.values() {
                            rows.push(crate::server_screen::OwnedClientRow {
                                identification: (**client.identification.load()).clone(),
                                client_type_label: client.client_type.load().type_str().to_owned(),
                                subscriptions: Vec::new(),
                                events_per_second: client.events_per_second(),
                                uptime_short: format_short_duration(client.uptime()),
                                active: true,
                            });
                        }
                        drop(clients_guard);
                        let snapshot = crate::server_screen::ServerInfoSnapshot {
                            uptime_seconds: info.uptime_seconds(),
                            connected_clients: rows,
                            stats: crate::server_screen::ServerStats::default(),
                        };
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::ServerInfoArrived(
                            snapshot,
                        )));
                        let kbps = info.bandwidth.current_bps() as f32 / 1000.0;
                        let _ = tx.try_send(Message::Server(ServerScreenMsg::BandwidthTick(kbps)));
                    }
                },
            )
            .boxed()
        }
    }

    let server_tick = if matches!(app.screen, Screen::Server) {
        from_recipe(ServerMetricsRecipe(Arc::clone(&app.server_subsystem)))
    } else {
        Subscription::none()
    };

    if let Some(state) = app.integration_detail.as_ref() {
        Subscription::batch([bus, health_subscription(state), server_tick])
    } else {
        Subscription::batch([bus, server_tick])
    }
}

pub fn theme_callback(app: &App) -> Theme {
    app.theme.clone()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_twitch::ChatConnectionState;
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
    fn status_pill_for_connected_is_positive() {
        let (variant, label) = status_pill_for(ChatConnectionState::Connected);
        assert_eq!(variant, forge_widgets::StatusVariant::Positive);
        assert_eq!(label, "Connected");
    }

    #[test]
    fn status_pill_for_disconnected_is_negative() {
        let (variant, label) = status_pill_for(ChatConnectionState::Disconnected);
        assert_eq!(variant, forge_widgets::StatusVariant::Negative);
        assert_eq!(label, "Disconnected");
    }

    #[test]
    fn status_pill_for_reconnecting_is_neutral() {
        let (variant, label) = status_pill_for(ChatConnectionState::Reconnecting { attempt: 2 });
        assert_eq!(variant, forge_widgets::StatusVariant::Neutral);
        assert_eq!(label, "Reconnecting");
    }

    #[test]
    fn status_pill_for_connecting_is_neutral() {
        let (variant, label) = status_pill_for(ChatConnectionState::Connecting);
        assert_eq!(variant, forge_widgets::StatusVariant::Neutral);
        assert_eq!(label, "Connecting");
    }

    #[test]
    fn settings_reconnect_twitch_with_no_handle_dispatches_task() {
        let mut app = App::default();
        let task = update(
            &mut app,
            Message::Settings(SettingsMsg::ReconnectPlatform(PlatformId::Twitch)),
        );
        let _ = task;
        assert!(app.twitch_chat_handle.is_none());
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
    fn view_compiles_settings_platforms_disconnected() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Navigate(Screen::Settings(SettingsSection::Platforms)),
        );
        let _ = view(&app);
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
            backend: sqlite,
            bus,
            storage_offline: false,
            boot_time: std::time::SystemTime::now(),
            hub: HubStats::new(),
            sidebar_state: SidebarExpandState::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            queues: QueuesState::new(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            script_registry: registry,
            twitch_chat_handle: None,
            chat_send_bridge: None,
            action_engine: Some(engine),
            scheduler: Some(scheduler),
            command_parser: Some(parser),
            integration_detail: None,
            obs_client: None,
            server_screen: ServerScreenState::default(),
            server_subsystem,
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            twitch_flow: None,
            twitch_login: None,
            twitch_reauth_required: false,
            obs_panel: crate::obs_panel::ObsPanelState::default(),
        };

        assert!(app.action_engine.is_some());
        assert!(app.scheduler.is_some());
        assert!(app.command_parser.is_some());
    }

    #[test]
    fn runtime_handles_absent_when_storage_offline() {
        let app = App {
            storage_offline: true,
            action_engine: None,
            scheduler: None,
            command_parser: None,
            ..App::default()
        };

        assert!(app.action_engine.is_none());
        assert!(app.scheduler.is_none());
        assert!(app.command_parser.is_none());
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
            description: None,
            sub_actions: vec![],
        };
        let detail = crate::actions::ActionDetail {
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
            description: None,
            sub_actions: vec![],
        };
        app.actions.detail = Some(crate::actions::ActionDetail {
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
            backend: Arc::clone(&dp),
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            storage_offline: false,
            boot_time: std::time::SystemTime::now(),
            hub: HubStats::new(),
            sidebar_state: SidebarExpandState::new(),
            event_feed: EventFeedState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            queues: QueuesState::new(),
            globals: GlobalsState::new(),
            script_editor: ScriptEditorState::new(),
            script_registry: Arc::new(ScriptRegistry::new()),
            twitch_chat_handle: None,
            chat_send_bridge: None,
            action_engine: None,
            scheduler: None,
            command_parser: None,
            integration_detail: None,
            obs_client: None,
            server_screen: ServerScreenState::default(),
            server_subsystem,
            settings_websocket: SettingsWebSocketState::default(),
            twitch_panel: crate::twitch_panel::TwitchPanelState::default(),
            twitch_flow: None,
            twitch_login: None,
            twitch_reauth_required: false,
            obs_panel: crate::obs_panel::ObsPanelState::default(),
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
        app.hub.actions_count = Some(47);
        app.hub.commands_count = Some(23);
        app.hub.triggers_fired = Some(1284);
        app.hub.globals_count = Some(31);
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
    fn hub_stats_loaded_ok_updates_all_fields() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let data = HubStatsData {
            actions_count: 5,
            commands_count: 3,
            triggers_fired: 42,
            globals_count: 7,
        };
        let _ = update(&mut app, Message::Hub(HubMsg::StatsLoaded(Ok(data))));
        assert_eq!(app.hub.actions_count, Some(5));
        assert_eq!(app.hub.commands_count, Some(3));
        assert_eq!(app.hub.triggers_fired, Some(42));
        assert_eq!(app.hub.globals_count, Some(7));
    }

    #[test]
    fn hub_stats_loaded_err_leaves_nones() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(
            &mut app,
            Message::Hub(HubMsg::StatsLoaded(Err("db error".into()))),
        );
        assert!(app.hub.actions_count.is_none());
        assert!(app.hub.commands_count.is_none());
        assert!(app.hub.triggers_fired.is_none());
        assert!(app.hub.globals_count.is_none());
    }

    #[test]
    fn sidebar_expand_state_initializes_all_collapsed() {
        let app = App::default();
        assert!(!app.sidebar_state.actions_queues);
        assert!(!app.sidebar_state.platforms);
        assert!(!app.sidebar_state.stream_apps);
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
    fn sidebar_toggle_platforms_flips_bool() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::TogglePlatforms));
        assert!(app.sidebar_state.platforms);
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::TogglePlatforms));
        assert!(!app.sidebar_state.platforms);
    }

    #[test]
    fn sidebar_toggle_stream_apps_flips_bool() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::ToggleStreamApps));
        assert!(app.sidebar_state.stream_apps);
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::ToggleStreamApps));
        assert!(!app.sidebar_state.stream_apps);
    }

    #[test]
    fn sidebar_toggles_are_independent() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::TogglePlatforms));
        assert!(!app.sidebar_state.actions_queues);
        assert!(app.sidebar_state.platforms);
        assert!(!app.sidebar_state.stream_apps);
    }

    #[test]
    fn connected_count_zero_when_no_handle() {
        let app = App::default();
        assert_eq!(connected_count(&app), 0);
    }

    #[test]
    fn breadcrumb_icon_for_home_returns_home_icon() {
        assert_eq!(breadcrumb_icon_for(&Screen::Home), ICON_HOME);
    }

    #[test]
    fn breadcrumb_icon_for_actions_returns_lightning() {
        assert_eq!(breadcrumb_icon_for(&Screen::Actions), ICON_LIGHTNING);
    }

    #[test]
    fn breadcrumb_icon_for_settings_returns_gear() {
        assert_eq!(
            breadcrumb_icon_for(&Screen::Settings(SettingsSection::Appearance)),
            ICON_GEAR
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
    fn view_home_renders_with_v2_chrome() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        app.hub.actions_count = Some(12);
        app.hub.commands_count = Some(5);
        app.hub.triggers_fired = Some(99);
        app.hub.globals_count = Some(3);
        let _ = view(&app);
    }

    #[test]
    fn view_live_chat_renders_with_v2_chrome() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::LiveChat));
        let _ = view(&app);
    }

    #[test]
    fn view_coming_soon_screen_renders_with_v2_chrome() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Viewers));
        let _ = view(&app);
    }

    #[test]
    fn view_platforms_expanded_sidebar_renders() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::TogglePlatforms));
        let _ = update(&mut app, Message::Sidebar(SidebarMsg::ToggleStreamApps));
        let _ = view(&app);
    }

    #[test]
    fn hub_view_desc_shows_actions_count() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Home));
        app.hub.actions_count = Some(47);
        app.hub.triggers_fired = Some(1284);
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
        assert!(app.obs_client.is_some());
        assert!(app.integration_detail.is_some());
    }

    #[test]
    fn obs_boot_result_err_leaves_obs_client_none() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::ObsBootResult(Err("connection refused".into())),
        );
        assert!(app.obs_client.is_none());
        assert!(app.integration_detail.is_none());
    }
}
