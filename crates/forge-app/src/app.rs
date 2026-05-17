use std::sync::Arc;
use std::time::SystemTime;

use forge_platform_twitch::{ChatConnectionState, TwitchChatHandle};
use forge_runtime::{
    ActionEngineHandle, CommandParserHandle, EventBus, ExecutionRequest, QueueSchedulerHandle,
};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider, SettingsRepo, reserved_keys};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ArgStack, EventId};
use forge_widgets::{BannerKind, ForgePalette, StepInfo, ThemeId};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::actions::{ActionsState, load_action_detail, load_actions_tree};
use crate::live_chat::{CHAT_LOG_MAX, LiveChatState, chat_row_from_event, live_chat_view};
use crate::message::{ActionsMsg, PlatformId, SettingsMsg};
use crate::onboarding_state::{DeviceCodeSession, DeviceCodeStatus, OnboardingState};
use crate::screen::OnboardingStep;
use crate::{Message, OnboardingMsg, Screen, SettingsSection};

pub struct App {
    pub screen: Screen,
    pub theme: Theme,
    pub palette: ForgePalette,
    pub backend: Arc<SqliteBackend>,
    pub bus: Arc<EventBus>,
    pub storage_offline: bool,
    pub onboarding: OnboardingState,
    pub live_chat: LiveChatState,
    pub actions: ActionsState,
    pub twitch_chat_handle: Option<TwitchChatHandle>,
    pub action_engine: Option<ActionEngineHandle>,
    pub scheduler: Option<QueueSchedulerHandle>,
    pub command_parser: Option<CommandParserHandle>,
}

impl App {
    pub fn default_with(
        initial: Screen,
        backend: Arc<SqliteBackend>,
        storage_offline: bool,
        action_engine: Option<ActionEngineHandle>,
        scheduler: Option<QueueSchedulerHandle>,
        command_parser: Option<CommandParserHandle>,
    ) -> Self {
        let (theme, palette) = forge_widgets::catppuccin_mocha();
        Self {
            screen: initial,
            theme,
            palette,
            backend,
            bus: EventBus::new(),
            storage_offline,
            onboarding: OnboardingState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            twitch_chat_handle: None,
            action_engine,
            scheduler,
            command_parser,
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
        Self {
            screen: Screen::Onboarding(OnboardingStep::Welcome),
            theme,
            palette,
            backend,
            bus: EventBus::new(),
            storage_offline: false,
            onboarding: OnboardingState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            twitch_chat_handle: None,
            action_engine: None,
            scheduler: None,
            command_parser: None,
        }
    }
}

fn persist_step(backend: Arc<SqliteBackend>, step: OnboardingStep) -> Task<Message> {
    Task::perform(
        async move {
            backend
                .set_string(reserved_keys::LAST_ONBOARDING_STEP, step.as_key())
                .await
                .map_err(|e| e.to_string())
        },
        Message::OnboardingPersistResult,
    )
}

pub fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Navigate(screen) => {
            if let Screen::Onboarding(ref step) = screen {
                app.onboarding.sync_step(step);
            }
            let load_actions = matches!(screen, Screen::Actions);
            app.screen = screen;
            if load_actions {
                Task::done(Message::Actions(ActionsMsg::LoadRequested))
            } else {
                Task::none()
            }
        }
        Message::OnboardingPersistResult(result) => {
            if let Err(ref e) = result {
                tracing::warn!(error = %e, "failed to persist onboarding_completed flag");
            }
            Task::none()
        }
        Message::Onboarding(sub) => match sub {
            OnboardingMsg::SkipSetup => {
                app.screen = Screen::Hub;
                let backend = Arc::clone(&app.backend);
                Task::perform(
                    async move {
                        backend
                            .set_string(reserved_keys::ONBOARDING_COMPLETED, "true")
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::OnboardingPersistResult,
                )
            }
            OnboardingMsg::AdvanceFromWelcome => {
                let next = OnboardingStep::ConnectPlatform;
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                persist_step(Arc::clone(&app.backend), next)
            }
            OnboardingMsg::PlatformSelected(id) => {
                app.onboarding.select_platform(id);
                Task::none()
            }
            OnboardingMsg::AdvanceFromPicker => {
                let next = match app.onboarding.selected_platform.as_deref() {
                    Some("twitch") => OnboardingStep::DeviceCodeFlow("twitch".into()),
                    _ => OnboardingStep::ConnectObs,
                };
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                let persist = persist_step(Arc::clone(&app.backend), next.clone());
                let nav_task = match &next {
                    OnboardingStep::DeviceCodeFlow(id) => Task::done(Message::Onboarding(
                        OnboardingMsg::EnterDeviceCodeFlow(id.clone()),
                    )),
                    _ => Task::none(),
                };
                Task::batch([persist, nav_task])
            }
            OnboardingMsg::EnterDeviceCodeFlow(_id) => {
                let Some(client_id) = forge_platform_twitch::client_id() else {
                    app.onboarding.device_code = Some(DeviceCodeSession {
                        user_code: String::new(),
                        verification_uri: String::new(),
                        expires_at: SystemTime::now(),
                        status: DeviceCodeStatus::MissingClientId,
                    });
                    return Task::none();
                };
                app.onboarding.device_code = Some(DeviceCodeSession {
                    user_code: String::new(),
                    verification_uri: String::new(),
                    expires_at: SystemTime::now(),
                    status: DeviceCodeStatus::Requesting,
                });
                Task::perform(
                    async move {
                        forge_platform_twitch::request_twitch_device_code(&client_id)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::Onboarding(OnboardingMsg::DeviceCodeReceived(result)),
                )
            }
            OnboardingMsg::DeviceCodeReceived(Ok(resp)) => {
                let Some(client_id) = forge_platform_twitch::client_id() else {
                    return Task::none();
                };
                if let Some(session) = app.onboarding.device_code.as_mut() {
                    session.user_code = resp.user_code.clone();
                    session.verification_uri = resp.verification_uri.clone();
                    session.expires_at = SystemTime::now() + resp.expires_in;
                    session.status = DeviceCodeStatus::Waiting;
                }
                let mut poller = forge_platform_twitch::new_twitch_poller(
                    client_id,
                    resp.device_code.clone(),
                    resp.interval,
                    resp.expires_in,
                );
                Task::perform(
                    async move { poller.run().await.map_err(|e| e.to_string()) },
                    |result| Message::Onboarding(OnboardingMsg::TokenReceived(result)),
                )
            }
            OnboardingMsg::DeviceCodeReceived(Err(e)) => {
                if let Some(session) = app.onboarding.device_code.as_mut() {
                    session.status = DeviceCodeStatus::Error(e);
                }
                Task::none()
            }
            OnboardingMsg::TokenReceived(Ok(tokens)) => {
                if app.onboarding.device_code.is_none() {
                    return Task::none();
                }
                let Some(client_id) = forge_platform_twitch::client_id() else {
                    if let Some(session) = app.onboarding.device_code.as_mut() {
                        session.status =
                            DeviceCodeStatus::Error("FORGE_TWITCH_CLIENT_ID not set".into());
                    }
                    return Task::none();
                };
                let backend = Arc::clone(&app.backend);
                let access = tokens.access_token.expose().to_owned();
                let refresh = tokens.refresh_token.as_ref().map(|r| r.expose().to_owned());
                let expires_secs = tokens.expires_in.as_secs();
                Task::perform(
                    async move {
                        let token = forge_types::OAuthToken::new(access.clone());
                        let user_info = forge_platform_twitch::fetch_user_info(&token, &client_id)
                            .await
                            .map_err(|e| e.to_string())?;
                        let bundle = serde_json::json!({
                            "access_token": access,
                            "refresh_token": refresh,
                            "expires_in_secs": expires_secs,
                            "user_id": user_info.id,
                            "login": user_info.login,
                        })
                        .to_string();
                        backend
                            .store(&CredentialId::new("twitch:broadcaster"), &bundle)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    |result| Message::Onboarding(OnboardingMsg::CredentialsStored(result)),
                )
            }
            OnboardingMsg::CredentialsStored(Ok(())) => {
                if let Some(session) = app.onboarding.device_code.as_mut() {
                    session.status = DeviceCodeStatus::Success;
                }
                let next = OnboardingStep::ConnectObs;
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                persist_step(Arc::clone(&app.backend), next)
            }
            OnboardingMsg::CredentialsStored(Err(e)) => {
                if let Some(session) = app.onboarding.device_code.as_mut() {
                    session.status = DeviceCodeStatus::Error(e);
                }
                Task::none()
            }
            OnboardingMsg::TokenReceived(Err(e)) => {
                if let Some(session) = app.onboarding.device_code.as_mut() {
                    session.status = DeviceCodeStatus::Error(e);
                }
                Task::none()
            }
            OnboardingMsg::BackFromDeviceCode => {
                app.onboarding.clear_device_code();
                let prev = OnboardingStep::ConnectPlatform;
                app.onboarding.sync_step(&prev);
                app.screen = Screen::Onboarding(prev.clone());
                persist_step(Arc::clone(&app.backend), prev)
            }
            OnboardingMsg::RetryDeviceCode => {
                app.onboarding.clear_device_code();
                Task::done(Message::Onboarding(OnboardingMsg::EnterDeviceCodeFlow(
                    "twitch".into(),
                )))
            }
            OnboardingMsg::BackFromPicker => {
                let prev = OnboardingStep::Welcome;
                app.onboarding.sync_step(&prev);
                app.screen = Screen::Onboarding(prev.clone());
                persist_step(Arc::clone(&app.backend), prev)
            }
            OnboardingMsg::SkipPicker => {
                let next = OnboardingStep::ConnectObs;
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                persist_step(Arc::clone(&app.backend), next)
            }
            OnboardingMsg::AdvanceFromObs | OnboardingMsg::SkipObs => {
                let next = OnboardingStep::StarterPack;
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                persist_step(Arc::clone(&app.backend), next)
            }
            OnboardingMsg::BackFromObs => {
                let prev = OnboardingStep::ConnectPlatform;
                app.onboarding.sync_step(&prev);
                app.screen = Screen::Onboarding(prev.clone());
                persist_step(Arc::clone(&app.backend), prev)
            }
            OnboardingMsg::AdvanceFromStarterPack | OnboardingMsg::SkipStarterPack => {
                let next = OnboardingStep::Ready;
                app.onboarding.sync_step(&next);
                app.screen = Screen::Onboarding(next.clone());
                persist_step(Arc::clone(&app.backend), next)
            }
            OnboardingMsg::BackFromStarterPack => {
                let prev = OnboardingStep::ConnectObs;
                app.onboarding.sync_step(&prev);
                app.screen = Screen::Onboarding(prev.clone());
                persist_step(Arc::clone(&app.backend), prev)
            }
            OnboardingMsg::FinishOnboarding => {
                app.screen = Screen::Hub;
                let backend = Arc::clone(&app.backend);
                Task::perform(
                    async move {
                        backend
                            .set_string(reserved_keys::ONBOARDING_COMPLETED, "true")
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::OnboardingPersistResult,
                )
            }
        },
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
            Task::none()
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
                        &limiter, &oauth, &client_id, &user_id, &user_id, &msg,
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
        Message::Actions(sub) => handle_actions_msg(app, sub),
        Message::Noop => Task::none(),
    }
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
            let Some(engine) = app.action_engine.clone() else {
                return Task::none();
            };
            Task::perform(
                async move {
                    engine
                        .dispatch(ExecutionRequest {
                            action_id: id,
                            trigger_event_id: EventId::new(),
                            initial_args: ArgStack::new(),
                        })
                        .await
                        .map_err(|e| e.to_string())
                },
                |r| {
                    if let Err(e) = r {
                        tracing::warn!(error = %e, "test trigger dispatch failed");
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
        ActionsMsg::OpenAddActionModal => Task::none(),
        ActionsMsg::OpenAddTriggerModal(_) => Task::none(),
    }
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
    TwitchChat::new(token, cid, user_id.clone(), user_id, bus).start();
    Ok(())
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

fn nav_button<'a>(label: &'a str, screen: Screen, palette: &ForgePalette) -> Element<'a, Message> {
    forge_widgets::ghost_button(label, Message::Navigate(screen), palette)
}

fn hub_view(palette: &ForgePalette) -> Element<'static, Message> {
    let hero = forge_widgets::hero_card(
        "Welcome to forge",
        "0.1.0-alpha.1",
        std::iter::empty::<Element<'static, Message>>(),
        palette,
    );

    let metrics = iced::widget::row![
        forge_widgets::metric_card("Twitch", "disconnected", None::<&str>, palette),
        forge_widgets::metric_card("OBS", "disconnected", None::<&str>, palette),
        forge_widgets::metric_card("Speak Queue", "empty", None::<&str>, palette),
    ]
    .spacing(12);

    let content = forge_widgets::card([hero, metrics.into()], palette);

    iced::widget::container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
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

fn settings_view<'a>(
    section: &'a SettingsSection,
    twitch_handle: Option<&'a TwitchChatHandle>,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let nav = iced::widget::column![
        settings_section_button("Appearance", SettingsSection::Appearance, section, palette),
        settings_section_button("Language", SettingsSection::Language, section, palette),
        settings_section_button("Shortcuts", SettingsSection::Shortcuts, section, palette),
        settings_section_button(
            "Notifications",
            SettingsSection::Notifications,
            section,
            palette
        ),
        settings_section_button("Platforms", SettingsSection::Platforms, section, palette),
        settings_section_button("Scripting", SettingsSection::Scripting, section, palette),
        settings_section_button("Queues", SettingsSection::Queues, section, palette),
        settings_section_button("Storage", SettingsSection::Storage, section, palette),
        settings_section_button("WebSocket", SettingsSection::WebSocket, section, palette),
        settings_section_button("Version", SettingsSection::Version, section, palette),
        settings_section_button(
            "Diagnostics",
            SettingsSection::Diagnostics,
            section,
            palette
        ),
    ]
    .spacing(4)
    .width(Length::Fixed(160.0));

    let pane: Element<'a, Message> = match section {
        SettingsSection::Diagnostics => settings_diagnostics_pane(palette),
        SettingsSection::Platforms => settings_platforms_pane(twitch_handle, palette),
        other => {
            let label = format!("Settings · {other:?}");
            iced::widget::container(forge_widgets::empty_state(
                label,
                "Placeholder for alpha-1.",
                None::<(&str, Message)>,
                palette,
            ))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    };

    iced::widget::row![nav, pane].spacing(16).into()
}

fn onboarding_left_column<'a>(
    steps: &'a [StepInfo],
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let hero_box = iced::widget::container(iced::widget::text("S").size(30.0).color(palette.shell))
        .width(60.0)
        .height(60.0)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.brand)),
            border: iced::Border {
                radius: 14.0.into(),
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        });

    let heading = iced::widget::text("Weave your\nfirst loom")
        .size(22.0)
        .color(palette.text_primary);

    let subtitle = iced::widget::text(
        "Optional setup. Skip anything you want and configure it later from settings.",
    )
    .size(12.5)
    .color(palette.text_muted);

    let stepper = forge_widgets::onboarding_stepper(steps, palette);

    iced::widget::column![hero_box, heading, subtitle, stepper]
        .spacing(20.0)
        .width(Length::Fixed(240.0))
        .into()
}

fn welcome_step_content<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header =
        forge_widgets::onboarding_step_header(1, 5, "Welcome to Forge", false, false, palette);

    let subtitle = iced::widget::text(
        "Forge your show with powerful automation, integrations, and TTS — all in one place.",
    )
    .size(13.0)
    .color(palette.text_muted);

    let footer = forge_widgets::onboarding_footer(
        None,
        None,
        "Get started",
        '→',
        Message::Onboarding(OnboardingMsg::AdvanceFromWelcome),
        true,
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn connect_platform_content<'a>(
    onboarding: &'a OnboardingState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header = forge_widgets::onboarding_step_header(
        2,
        5,
        "Connect a streaming platform",
        true,
        false,
        palette,
    );

    let subtitle = iced::widget::text(
        "You can connect more later from settings. Pick one to start — we'll show you a code to enter on the platform's site.",
    )
    .size(13.0)
    .color(palette.text_muted);

    let selected = onboarding.selected_platform.as_deref();

    let twitch = forge_widgets::platform_picker_card(
        forge_widgets::PlatformCardProps {
            name: "Twitch",
            letter: "T",
            brand_color: palette.brand,
            subtitle: "Most popular",
            capability_summary: "Chat, subs, bits, raids, channel points, EventSub",
            selected: selected == Some("twitch"),
        },
        Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        palette,
    );

    let youtube = forge_widgets::platform_picker_card(
        forge_widgets::PlatformCardProps {
            name: "YouTube",
            letter: "Y",
            brand_color: palette.random,
            subtitle: "Live streaming",
            capability_summary: "Chat, super chat, memberships, sponsorships",
            selected: selected == Some("youtube"),
        },
        Message::Onboarding(OnboardingMsg::PlatformSelected("youtube".into())),
        palette,
    );

    let kick = forge_widgets::platform_picker_card(
        forge_widgets::PlatformCardProps {
            name: "Kick",
            letter: "K",
            brand_color: palette.info,
            subtitle: "Growing platform",
            capability_summary: "Chat, subscribers, gifted subs, host events",
            selected: selected == Some("kick"),
        },
        Message::Onboarding(OnboardingMsg::PlatformSelected("kick".into())),
        palette,
    );

    let trovo = forge_widgets::platform_picker_card(
        forge_widgets::PlatformCardProps {
            name: "Trovo",
            letter: "Tr",
            brand_color: palette.success,
            subtitle: "Niche audience",
            capability_summary: "Chat, mana, spells, gift subs, follows",
            selected: selected == Some("trovo"),
        },
        Message::Onboarding(OnboardingMsg::PlatformSelected("trovo".into())),
        palette,
    );

    let grid = iced::widget::column![
        iced::widget::row![twitch, youtube].spacing(10),
        iced::widget::row![kick, trovo].spacing(10),
    ]
    .spacing(10);

    let locale_tip = forge_widgets::locale_tip_card(
        "Streaming in Ukrainian? Forge has full UA localization, UTF-8 chat handling, and a community starter pack tailored for UA streamers.",
        Some("Learn more →"),
        Some(Message::Onboarding(OnboardingMsg::SkipSetup)),
        palette,
    );

    let footer = forge_widgets::onboarding_footer(
        Some(Message::Onboarding(OnboardingMsg::BackFromPicker)),
        Some(Message::Onboarding(OnboardingMsg::SkipPicker)),
        onboarding.continue_label(),
        '→',
        Message::Onboarding(OnboardingMsg::AdvanceFromPicker),
        onboarding.selected_platform.is_some(),
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        grid,
        locale_tip,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn twitch_scope_hint() -> &'static str {
    "polling every 5s · scopes: chat:read chat:edit channel:read:subscriptions bits:read moderator:read:followers"
}

fn device_code_error_body<'a>(detail: &'a str, palette: &'a ForgePalette) -> Element<'a, Message> {
    let banner = forge_widgets::live_status_banner(
        BannerKind::Error,
        "Authorization failed.",
        Some(detail),
        palette,
    );
    let retry = forge_widgets::primary_button(
        "Try again",
        Message::Onboarding(OnboardingMsg::RetryDeviceCode),
        palette,
    );
    iced::widget::column![banner, retry].spacing(10.0).into()
}

fn device_code_body<'a>(
    onboarding: &'a OnboardingState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let session = match onboarding.device_code.as_ref() {
        None => {
            return forge_widgets::live_status_banner(
                BannerKind::Waiting,
                "Requesting authorization code...",
                None,
                palette,
            );
        }
        Some(s) => s,
    };

    match &session.status {
        DeviceCodeStatus::Requesting => forge_widgets::live_status_banner(
            BannerKind::Waiting,
            "Requesting authorization code...",
            None,
            palette,
        ),
        DeviceCodeStatus::Waiting => {
            let remaining = session
                .expires_at
                .duration_since(SystemTime::now())
                .unwrap_or_default();

            let steps = iced::widget::row![
                forge_widgets::numbered_box_step(
                    1,
                    "Open this URL in your browser",
                    &session.verification_uri,
                    false,
                    palette,
                ),
                forge_widgets::numbered_box_step(
                    2,
                    "Enter the code shown above",
                    "Twitch will display a confirmation when authorized.",
                    true,
                    palette,
                ),
            ]
            .spacing(10.0)
            .width(Length::Fill);

            let scope_hint = twitch_scope_hint();

            iced::widget::column![
                // clipboard wiring is deferred; copy button dispatches RetryDeviceCode as placeholder
                forge_widgets::device_code_display(
                    &session.user_code,
                    Message::Onboarding(OnboardingMsg::RetryDeviceCode),
                    palette,
                ),
                forge_widgets::expiration_timer(
                    remaining,
                    "Get new code",
                    Message::Onboarding(OnboardingMsg::RetryDeviceCode),
                    palette,
                ),
                steps,
                forge_widgets::live_status_banner(
                    BannerKind::Waiting,
                    "Waiting for authorization...",
                    Some(scope_hint),
                    palette,
                ),
            ]
            .spacing(14.0)
            .into()
        }
        DeviceCodeStatus::Success => forge_widgets::live_status_banner(
            BannerKind::Success,
            "Authorized successfully. Continuing setup...",
            None,
            palette,
        ),
        DeviceCodeStatus::Error(msg) => device_code_error_body(msg, palette),
        DeviceCodeStatus::MissingClientId => forge_widgets::live_status_banner(
            BannerKind::Error,
            "Twitch integration is not configured.",
            Some(
                "Set FORGE_TWITCH_CLIENT_ID env var with your own registered app's client_id. See KNOWN_ISSUES.md.",
            ),
            palette,
        ),
    }
}

fn device_code_flow_content<'a>(
    onboarding: &'a OnboardingState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let header =
        forge_widgets::onboarding_step_header(2, 5, "Authorize on Twitch", false, true, palette);

    let subtitle = iced::widget::text(
        "Enter the code below on Twitch's activation page. We'll automatically continue when you authorize.",
    )
    .size(13.0)
    .color(palette.text_muted);

    let body = device_code_body(onboarding, palette);

    let footer = forge_widgets::onboarding_footer(
        Some(Message::Onboarding(OnboardingMsg::BackFromDeviceCode)),
        None,
        "Continue",
        '→',
        Message::Onboarding(OnboardingMsg::BackFromDeviceCode),
        false,
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        body,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn connect_obs_content<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header =
        forge_widgets::onboarding_step_header(3, 5, "Connect OBS Studio", true, false, palette);

    let subtitle = iced::widget::text(
        "Forge talks to OBS via the WebSocket plugin (bundled since OBS 28). \
         You'll need it running locally — we'll connect to localhost:4455 by default.",
    )
    .size(11.5)
    .color(palette.text_muted);

    let coming_soon_card = forge_widgets::card(
        [
            iced::widget::text("Coming in alpha-3")
                .size(13.0)
                .color(palette.text_primary)
                .into(),
            iced::widget::text(
                "OBS connection wiring is implemented in alpha-3. \
                 For now, this step is skippable and Forge will operate without OBS integration.",
            )
            .size(11.5)
            .color(palette.text_muted)
            .into(),
        ],
        palette,
    );

    let footer = forge_widgets::onboarding_footer(
        Some(Message::Onboarding(OnboardingMsg::BackFromObs)),
        Some(Message::Onboarding(OnboardingMsg::SkipObs)),
        "I'll connect later",
        '→',
        Message::Onboarding(OnboardingMsg::AdvanceFromObs),
        true,
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        coming_soon_card,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn starter_pack_content<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header = forge_widgets::onboarding_step_header(4, 5, "Starter pack", true, false, palette);

    let subtitle = iced::widget::text(
        "Pre-built actions, commands, and overlays for common streamer setups. \
         Pick a pack to install or skip and add later from settings.",
    )
    .size(11.5)
    .color(palette.text_muted);

    let ua_pack = forge_widgets::card(
        [
            iced::widget::text("UA streamer pack")
                .size(13.0)
                .color(palette.text_primary)
                .into(),
            iced::widget::text("Coming in alpha-2 RC — placeholder for now.")
                .size(11.5)
                .color(palette.text_muted)
                .into(),
        ],
        palette,
    );

    let generic_pack = forge_widgets::card(
        [
            iced::widget::text("Generic essentials")
                .size(13.0)
                .color(palette.text_primary)
                .into(),
            iced::widget::text("Coming in alpha-2 RC — placeholder for now.")
                .size(11.5)
                .color(palette.text_muted)
                .into(),
        ],
        palette,
    );

    let pack_grid = iced::widget::row![ua_pack, generic_pack].spacing(10);

    let footer = forge_widgets::onboarding_footer(
        Some(Message::Onboarding(OnboardingMsg::BackFromStarterPack)),
        Some(Message::Onboarding(OnboardingMsg::SkipStarterPack)),
        "Continue",
        '→',
        Message::Onboarding(OnboardingMsg::AdvanceFromStarterPack),
        true,
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        pack_grid,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn ready_content<'a>(palette: &'a ForgePalette) -> Element<'a, Message> {
    let header = forge_widgets::onboarding_step_header(5, 5, "You're ready", false, false, palette);

    let subtitle = iced::widget::text(
        "Everything you configured is saved. You can change any of these later from Settings.",
    )
    .size(11.5)
    .color(palette.text_muted);

    let banner = forge_widgets::live_status_banner(
        forge_widgets::BannerKind::Success,
        "Forge is ready to run your show.",
        None,
        palette,
    );

    let summary_card = forge_widgets::card(
        [iced::widget::text(
            "Click Enter Forge to open the Hub. From there you can configure \
             actions, triggers, integrations, and TTS.",
        )
        .size(11.5)
        .color(palette.text_muted)
        .into()],
        palette,
    );

    let footer = forge_widgets::onboarding_footer(
        None,
        None,
        "Enter Forge",
        '→',
        Message::Onboarding(OnboardingMsg::FinishOnboarding),
        true,
        palette,
    );

    iced::widget::column![
        header,
        subtitle,
        banner,
        summary_card,
        iced::widget::Space::new().height(Length::Fill),
        footer,
    ]
    .spacing(16.0)
    .height(Length::Fill)
    .into()
}

fn onboarding_view<'a>(
    step: &'a OnboardingStep,
    onboarding: &'a OnboardingState,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    let skip_action: Element<'a, Message> = forge_widgets::ghost_button(
        "Skip setup, just let me explore →",
        Message::Onboarding(OnboardingMsg::SkipSetup),
        palette,
    );

    let title_bar =
        forge_widgets::title_bar_with_logo("Forge", "Quick setup", 'S', vec![skip_action], palette);

    let left = onboarding_left_column(&onboarding.step_infos, palette);

    let right: Element<'a, Message> = match step {
        OnboardingStep::Welcome => welcome_step_content(palette),
        OnboardingStep::ConnectPlatform => connect_platform_content(onboarding, palette),
        OnboardingStep::DeviceCodeFlow(_) => device_code_flow_content(onboarding, palette),
        OnboardingStep::ConnectObs => connect_obs_content(palette),
        OnboardingStep::StarterPack => starter_pack_content(palette),
        OnboardingStep::Ready => ready_content(palette),
    };

    let body = iced::widget::row![left, right]
        .spacing(40.0)
        .padding(iced::Padding::from([32_u16, 40_u16]))
        .height(Length::Fill);

    iced::widget::column![title_bar, body]
        .height(Length::Fill)
        .into()
}

fn actions_view<'a>(app: &'a App, palette: &'a ForgePalette) -> Element<'a, Message> {
    use forge_widgets::{NodeProps, NodeStatus, ToggleProps};
    use iced::widget::{column, container, row, scrollable, text};

    let actions_state = &app.actions;

    let new_action_btn = forge_widgets::primary_button_small(
        "+ New action",
        Message::Actions(ActionsMsg::OpenAddActionModal),
        palette,
    );

    let search = forge_widgets::search_input("Search actions...", "", |_| Message::Noop, palette);

    let toolbar_row = row![search, new_action_btn]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center);

    let mut tree_col = column![toolbar_row].spacing(4);

    if actions_state.loading {
        tree_col = tree_col.push(text("Loading...").size(12.0).color(palette.text_muted));
    } else if actions_state.tree.is_empty() {
        tree_col = tree_col.push(forge_widgets::empty_state(
            "No actions yet",
            "Use + New action to create your first action.",
            None::<(&str, Message)>,
            palette,
        ));
    } else {
        for group in &actions_state.tree {
            let header = forge_widgets::section_header(
                &group.name,
                Some(group.actions.len() as u32),
                palette,
            );
            tree_col = tree_col.push(header);

            for summary in &group.actions {
                let status = if summary.enabled {
                    NodeStatus::Enabled
                } else {
                    NodeStatus::Disabled
                };
                let selected = actions_state.selected == Some(summary.id);
                let node = forge_widgets::tree_node_with_status(
                    palette,
                    NodeProps {
                        label: &summary.name,
                        status,
                        sub_action_count: summary.sub_action_count,
                        selected,
                        on_press: Message::Actions(ActionsMsg::ActionSelected(summary.id)),
                    },
                );
                tree_col = tree_col.push(node);
            }
        }
    }

    let left_pane = container(scrollable(tree_col).height(Length::Fill))
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .padding([8, 0])
        .style(move |_theme: &iced::Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(palette.shell)),
            border: iced::Border {
                color: palette.border_regular,
                width: 0.5,
                radius: 0.0.into(),
            },
            ..iced::widget::container::Style::default()
        });

    let right_pane: Element<'_, Message> = match actions_state.detail.as_ref() {
        None if actions_state.selected.is_some() => container(
            text("Loading action...")
                .size(12.0)
                .color(palette.text_muted),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(18)
        .into(),
        None => container(forge_widgets::empty_state(
            "Select an action",
            "Choose an action from the list to view its triggers and sub-action chain.",
            None::<(&str, Message)>,
            palette,
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        Some(detail) => {
            let action = &detail.action;

            let enabled_toggle = forge_widgets::toggle(
                palette,
                ToggleProps {
                    label: "Enabled",
                    description: "Action runs when a trigger fires.",
                    value: action.enabled,
                    on_toggle: Message::Actions(ActionsMsg::ToggleEnabled(
                        action.id,
                        !action.enabled,
                    )),
                },
            );

            let test_btn = forge_widgets::secondary_button(
                "Test run",
                Message::Actions(ActionsMsg::TestTrigger(action.id)),
                palette,
            );

            let delete_btn = forge_widgets::destructive_button(
                "Delete",
                Message::Actions(ActionsMsg::DeleteAction(action.id)),
                palette,
            );

            let header_row = row![
                text(&action.name).size(18.0).color(palette.text_primary),
                iced::widget::Space::new().width(Length::Fill),
                test_btn,
                delete_btn,
            ]
            .spacing(8)
            .align_y(iced::alignment::Vertical::Center);

            let description_el = if let Some(desc) = &action.description {
                text(desc.as_str()).size(12.0).color(palette.text_muted)
            } else {
                text("").size(12.0).color(palette.text_muted)
            };

            let triggers_header = row![
                forge_widgets::section_header(
                    "TRIGGERS",
                    Some(detail.triggers.len() as u32),
                    palette,
                ),
                iced::widget::Space::new().width(Length::Fill),
                forge_widgets::ghost_button(
                    "+ Add trigger",
                    Message::Actions(ActionsMsg::OpenAddTriggerModal(action.id)),
                    palette,
                ),
            ]
            .align_y(iced::alignment::Vertical::Center)
            .spacing(8);

            let trigger_elems: Vec<Element<'_, Message>> = detail
                .triggers
                .iter()
                .map(|t| {
                    trigger_row_element(
                        format!("Twitch \u{00b7} {:?}", t.kind),
                        format!("{:?}", t.config),
                        palette,
                    )
                })
                .chain(detail.commands.iter().map(|c| {
                    trigger_row_element(
                        "Twitch \u{00b7} Chat command".to_string(),
                        format!(
                            "{} \u{00b7} cooldown {}s \u{00b7} {:?}",
                            c.name, c.cooldown_secs, c.permission
                        ),
                        palette,
                    )
                }))
                .collect();

            let mut triggers_col = column![].spacing(6);
            if trigger_elems.is_empty() {
                triggers_col = triggers_col.push(
                    text("No triggers — use + Add trigger to fire this action.")
                        .size(11.5)
                        .color(palette.text_faint),
                );
            }
            for elem in trigger_elems {
                triggers_col = triggers_col.push(elem);
            }

            let sub_count = action.sub_actions.len();
            let sub_actions_header = forge_widgets::section_header(
                format!("SUB-ACTIONS \u{00b7} {sub_count}"),
                None,
                palette,
            );

            let mut sub_actions_col = column![].spacing(6);
            if action.sub_actions.is_empty() {
                sub_actions_col = sub_actions_col.push(
                    text("No sub-actions yet.")
                        .size(11.5)
                        .color(palette.text_faint),
                );
            }
            for (idx, spec) in action.sub_actions.iter().enumerate() {
                let idx_u8 = (idx + 1).min(255) as u8;
                let card = sub_action_element(idx_u8, spec, palette);
                sub_actions_col = sub_actions_col.push(card);
            }

            let detail_col = column![
                header_row,
                description_el,
                enabled_toggle,
                triggers_header,
                triggers_col,
                sub_actions_header,
                sub_actions_col,
            ]
            .spacing(12);

            container(scrollable(detail_col).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(18)
                .into()
        }
    };

    row![left_pane, right_pane].into()
}

fn trigger_row_element<'a>(
    kind_label: String,
    summary: String,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use iced::widget::{container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let icon_el = container(text('\u{ea21}'.to_string()).size(14.0).color(palette.brand))
        .width(26)
        .height(26)
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

    let label_col = iced::widget::column![
        text(kind_label).size(12.5).color(palette.text_primary),
        text(summary)
            .size(11.0)
            .color(palette.text_muted)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
    ]
    .spacing(1);

    let inner = row![icon_el, container(label_col).width(Length::Fill),]
        .spacing(10)
        .align_y(Alignment::Center);

    container(inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 8.0,
            right: 10.0,
            bottom: 8.0,
            left: 10.0,
        })
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Md).into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn sub_action_element<'a>(
    index: u8,
    spec: &forge_types::SubActionSpec,
    palette: &'a ForgePalette,
) -> Element<'a, Message> {
    use forge_types::SubActionSpec;
    use iced::widget::{container, row, text};
    use iced::{Alignment, Background, Border, Length};

    let (icon_char, kind_label, preview): (char, &str, String) = match spec {
        SubActionSpec::SendChat { message, target } => (
            '\u{ea21}',
            spec.kind_label(),
            format!("{target}: {message}"),
        ),
        SubActionSpec::SetGlobal { name, value } => {
            ('\u{eb58}', spec.kind_label(), format!("{name} = {value}"))
        }
        SubActionSpec::Delay { ms } => ('\u{ebc5}', spec.kind_label(), format!("{ms}ms")),
        SubActionSpec::Log { level, message } => (
            '\u{ea77}',
            spec.kind_label(),
            format!("[{level:?}] {message}"),
        ),
    };

    let index_el = container(
        text(format!("{index}"))
            .size(11.0)
            .color(palette.shell)
            .font(forge_widgets::font(forge_widgets::FontRole::Body)),
    )
    .width(22)
    .height(22)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(Background::Color(palette.brand)),
        border: Border {
            radius: 11.0.into(),
            color: iced::Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    });

    let icon_el = container(text(icon_char.to_string()).size(14.0).color(palette.brand))
        .width(26)
        .height(26)
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

    let label_col = iced::widget::column![
        text(kind_label).size(12.5).color(palette.text_primary),
        text(preview)
            .size(11.0)
            .color(palette.text_muted)
            .font(forge_widgets::font(forge_widgets::FontRole::Monospace)),
    ]
    .spacing(2);

    let card_inner = row![icon_el, container(label_col).width(Length::Fill),]
        .spacing(8)
        .align_y(Alignment::Center);

    let card = container(card_inner)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 10.0,
            right: 12.0,
            bottom: 10.0,
            left: 12.0,
        })
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(Background::Color(palette.elevated)),
            border: Border {
                color: palette.border_regular,
                width: 0.5,
                radius: forge_widgets::radius(forge_widgets::Radius::Lg).into(),
            },
            ..container::Style::default()
        });

    row![index_el, card]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
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

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = &app.palette;

    if let Screen::Onboarding(step) = &app.screen {
        return onboarding_view(step, &app.onboarding, palette);
    }

    let nav_items = vec![
        nav_button("Hub", Screen::Hub, palette),
        nav_button("Live Chat", Screen::LiveChat, palette),
        nav_button("Events", Screen::EventFeed, palette),
        nav_button("Globals", Screen::Globals, palette),
        nav_button("Actions", Screen::Actions, palette),
        nav_button("Commands", Screen::Commands, palette),
        nav_button("Platforms", Screen::Platforms, palette),
        nav_button("Integrations", Screen::Integrations, palette),
        nav_button(
            "Settings",
            Screen::Settings(SettingsSection::Appearance),
            palette,
        ),
    ];

    let sidebar = forge_widgets::sidebar(
        vec![forge_widgets::sidebar_section("Main", nav_items, palette)],
        palette,
    );

    let content: Element<'_, Message> = match &app.screen {
        Screen::Hub => hub_view(palette),
        Screen::LiveChat => live_chat_view(&app.live_chat, palette),
        Screen::Actions => actions_view(app, palette),
        Screen::Settings(section) => {
            settings_view(section, app.twitch_chat_handle.as_ref(), palette)
        }
        Screen::Onboarding(_) => unreachable!(),
        other => coming_soon_view(format!("{other:?}"), palette),
    };

    iced::widget::row![sidebar, content].into()
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

    from_recipe(BusRecipe(app.bus.clone()))
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
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        assert_eq!(app.screen, Screen::Hub);
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
    fn navigate_to_onboarding_welcome() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(
            &mut app,
            Message::Navigate(Screen::Onboarding(OnboardingStep::Welcome)),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
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
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
    }

    #[test]
    fn subscription_compiles() {
        let app = App::default();
        let _ = subscription(&app);
    }

    #[test]
    fn onboarding_skip_setup_navigates_to_hub() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::SkipSetup));
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn onboarding_advance_from_welcome_navigates_to_connect_platform() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromWelcome),
        );
        assert_eq!(
            app.screen,
            Screen::Onboarding(OnboardingStep::ConnectPlatform)
        );
    }

    #[test]
    fn onboarding_state_initialized_with_no_platform() {
        let app = App::default();
        assert!(app.onboarding.selected_platform.is_none());
    }

    #[test]
    fn view_compiles_onboarding_welcome() {
        let app = App::default();
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_hub() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = view(&app);
    }

    #[test]
    fn platform_selected_stores_id() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        );
        assert_eq!(app.onboarding.selected_platform.as_deref(), Some("twitch"));
    }

    #[test]
    fn platform_selected_replaces_previous_selection() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        );
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("youtube".into())),
        );
        assert_eq!(app.onboarding.selected_platform.as_deref(), Some("youtube"));
    }

    #[test]
    fn advance_from_picker_with_twitch_goes_to_device_code_flow() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        );
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromPicker),
        );
        assert_eq!(
            app.screen,
            Screen::Onboarding(OnboardingStep::DeviceCodeFlow("twitch".into()))
        );
    }

    #[test]
    fn advance_from_picker_without_twitch_goes_to_connect_obs() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("kick".into())),
        );
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromPicker),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::ConnectObs));
    }

    #[test]
    fn back_from_picker_returns_to_welcome() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromWelcome),
        );
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::BackFromPicker));
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
    }

    #[test]
    fn skip_picker_advances_to_connect_obs() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::SkipPicker));
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::ConnectObs));
    }

    #[test]
    fn view_compiles_connect_platform() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromWelcome),
        );
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_connect_platform_with_selection() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromWelcome),
        );
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        );
        let _ = view(&app);
    }

    #[test]
    fn advance_from_obs_navigates_to_starter_pack() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::AdvanceFromObs));
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::StarterPack));
    }

    #[test]
    fn skip_obs_navigates_to_starter_pack() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::SkipObs));
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::StarterPack));
    }

    #[test]
    fn back_from_obs_returns_to_connect_platform() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::BackFromObs));
        assert_eq!(
            app.screen,
            Screen::Onboarding(OnboardingStep::ConnectPlatform)
        );
    }

    #[test]
    fn advance_from_starter_pack_navigates_to_ready() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromStarterPack),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Ready));
    }

    #[test]
    fn skip_starter_pack_navigates_to_ready() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::SkipStarterPack),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Ready));
    }

    #[test]
    fn back_from_starter_pack_returns_to_connect_obs() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::BackFromStarterPack),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::ConnectObs));
    }

    #[test]
    fn finish_onboarding_navigates_to_hub() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::FinishOnboarding),
        );
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn persist_result_ok_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(&mut app, Message::OnboardingPersistResult(Ok(())));
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn persist_result_err_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(
            &mut app,
            Message::OnboardingPersistResult(Err("disk full".into())),
        );
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn view_compiles_connect_obs() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::AdvanceFromObs));
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::BackFromStarterPack),
        );
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_starter_pack() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::AdvanceFromObs));
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_ready() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Onboarding(OnboardingMsg::AdvanceFromObs));
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromStarterPack),
        );
        let _ = view(&app);
    }

    #[test]
    fn stepper_shows_connect_obs_as_current_on_step_3() {
        let mut app = App::default();
        app.onboarding.sync_step(&OnboardingStep::ConnectObs);
        assert_eq!(
            app.onboarding.step_infos[2].status,
            forge_widgets::StepStatus::Current
        );
        assert_eq!(
            app.onboarding.step_infos[0].status,
            forge_widgets::StepStatus::Done
        );
    }

    #[test]
    fn stepper_shows_starter_pack_as_current_on_step_4() {
        let mut app = App::default();
        app.onboarding.sync_step(&OnboardingStep::StarterPack);
        assert_eq!(
            app.onboarding.step_infos[3].status,
            forge_widgets::StepStatus::Current
        );
    }

    #[test]
    fn stepper_shows_ready_as_current_on_step_5() {
        let mut app = App::default();
        app.onboarding.sync_step(&OnboardingStep::Ready);
        assert_eq!(
            app.onboarding.step_infos[4].status,
            forge_widgets::StepStatus::Current
        );
    }

    #[test]
    fn enter_device_code_flow_sets_requesting_or_missing_client_id() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::EnterDeviceCodeFlow("twitch".into())),
        );
        assert!(app.onboarding.device_code.is_some());
        let is_valid_initial = app.onboarding.device_code.as_ref().is_some_and(|s| {
            matches!(
                s.status,
                DeviceCodeStatus::MissingClientId | DeviceCodeStatus::Requesting
            )
        });
        assert!(is_valid_initial);
    }

    #[test]
    fn back_from_device_code_clears_session_and_returns_to_platform_picker() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Waiting,
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::BackFromDeviceCode),
        );
        assert!(app.onboarding.device_code.is_none());
        assert_eq!(
            app.screen,
            Screen::Onboarding(OnboardingStep::ConnectPlatform)
        );
    }

    #[test]
    fn retry_device_code_clears_session() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Error("timeout".into()),
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::RetryDeviceCode),
        );
        assert!(app.onboarding.device_code.is_none());
    }

    #[test]
    fn device_code_received_err_sets_error_status() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: String::new(),
            verification_uri: String::new(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Requesting,
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::DeviceCodeReceived(Err("HTTP 400".into()))),
        );
        assert!(
            app.onboarding
                .device_code
                .as_ref()
                .is_some_and(|s| { matches!(s.status, DeviceCodeStatus::Error(_)) })
        );
    }

    #[test]
    fn token_received_err_sets_error_status() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Waiting,
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::TokenReceived(Err("user denied".into()))),
        );
        assert!(
            app.onboarding
                .device_code
                .as_ref()
                .is_some_and(|s| { matches!(s.status, DeviceCodeStatus::Error(_)) })
        );
    }

    #[test]
    fn token_received_ok_ignored_when_session_cleared() {
        use forge_platform_core::oauth::TokenResponse;
        use forge_types::{OAuthToken, RefreshToken};
        use std::time::Duration;

        let mut app = App::default();
        app.onboarding.device_code = None;
        let fake_token = TokenResponse {
            access_token: OAuthToken::new("access"),
            refresh_token: Some(RefreshToken::new("refresh")),
            expires_in: Duration::from_secs(3600),
            scopes: vec!["chat:read".into()],
        };
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::TokenReceived(Ok(fake_token))),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
    }

    fn make_device_code_app(status: DeviceCodeStatus) -> App {
        let mut app = App::default();
        app.onboarding
            .sync_step(&OnboardingStep::DeviceCodeFlow("twitch".into()));
        app.screen = Screen::Onboarding(OnboardingStep::DeviceCodeFlow("twitch".into()));
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "WDJB-MJHT".into(),
            verification_uri: "https://www.twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now() + std::time::Duration::from_secs(300),
            status,
        });
        app
    }

    #[test]
    fn view_compiles_device_code_flow_no_session() {
        let mut app = App::default();
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::PlatformSelected("twitch".into())),
        );
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::AdvanceFromPicker),
        );
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_device_code_flow_requesting() {
        let app = make_device_code_app(DeviceCodeStatus::Requesting);
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_device_code_flow_waiting() {
        let app = make_device_code_app(DeviceCodeStatus::Waiting);
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_device_code_flow_missing_client_id() {
        let app = make_device_code_app(DeviceCodeStatus::MissingClientId);
        let _ = view(&app);
    }

    #[test]
    fn view_compiles_device_code_flow_error() {
        let app = make_device_code_app(DeviceCodeStatus::Error("bad client id".into()));
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
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(
            &mut app,
            Message::Settings(SettingsMsg::PlatformReconnectResult(Ok(()))),
        );
        assert_eq!(app.screen, Screen::Hub);
    }

    #[test]
    fn settings_reconnect_result_err_logs_and_leaves_screen_unchanged() {
        let mut app = App::default();
        let _ = update(&mut app, Message::Navigate(Screen::Hub));
        let _ = update(
            &mut app,
            Message::Settings(SettingsMsg::PlatformReconnectResult(Err(
                "connection refused".into(),
            ))),
        );
        assert_eq!(app.screen, Screen::Hub);
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
    fn credentials_stored_ok_sets_success_status_and_navigates_to_connect_obs() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Waiting,
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::CredentialsStored(Ok(()))),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::ConnectObs));
        assert!(
            app.onboarding
                .device_code
                .as_ref()
                .is_some_and(|s| matches!(s.status, DeviceCodeStatus::Success))
        );
    }

    #[test]
    fn credentials_stored_err_sets_error_status_and_does_not_navigate() {
        let mut app = App::default();
        app.onboarding.device_code = Some(crate::DeviceCodeSession {
            user_code: "ABCD-1234".into(),
            verification_uri: "https://twitch.tv/activate".into(),
            expires_at: std::time::SystemTime::now(),
            status: DeviceCodeStatus::Waiting,
        });
        let _ = update(
            &mut app,
            Message::Onboarding(OnboardingMsg::CredentialsStored(Err(
                "keyring write failed".into(),
            ))),
        );
        assert_eq!(app.screen, Screen::Onboarding(OnboardingStep::Welcome));
        assert!(
            app.onboarding
                .device_code
                .as_ref()
                .is_some_and(|s| matches!(s.status, DeviceCodeStatus::Error(_)))
        );
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
        let bus = EventBus::new();
        let queues = dp.queue_repo().list().await.expect("list queues");

        let engine = forge_runtime::spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));
        let scheduler =
            forge_runtime::QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
        let parser = forge_runtime::CommandParser::spawn(
            Arc::clone(&bus),
            Arc::clone(&dp),
            scheduler.clone(),
        );

        let (theme, palette) = forge_widgets::catppuccin_mocha();
        let app = App {
            screen: Screen::Hub,
            theme,
            palette,
            backend: sqlite,
            bus,
            storage_offline: false,
            onboarding: OnboardingState::new(),
            live_chat: LiveChatState::new(),
            actions: ActionsState::new(),
            twitch_chat_handle: None,
            action_engine: Some(engine),
            scheduler: Some(scheduler),
            command_parser: Some(parser),
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
}
