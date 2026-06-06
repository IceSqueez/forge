use std::sync::Arc;

use forge_events::EventPublisher;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, QuickActions, SectionIcon,
};
use forge_platform_twitch::{SubscriptionTracker, TwitchChat, TwitchIntegrationBundle};
use forge_runtime::EventBus;
use forge_storage::{CredentialId, CredentialsRepo};
use iced::Task;

use crate::app::App;
use crate::builtin_detail::BuiltinDetailState;
use crate::message::{
    DiscordClientRef, HotkeyClientRef, Message, MidiClientRef, ObsClientRef, ServerSubsystemMsg,
    TwitchBootBundle, VTubeClientRef,
};
use crate::server_screen::ServerStatus;

pub async fn load_twitch_credential(
    creds: Arc<dyn CredentialsRepo>,
) -> Result<Option<TwitchBootBundle>, String> {
    let Some(client_id) = forge_platform_twitch::client_id() else {
        return Ok(None);
    };
    let stored = forge_platform_twitch::credentials::load(&*creds)
        .await
        .map_err(|e| e.to_string())?;
    Ok(stored.map(|s| TwitchBootBundle {
        access_token: s.access_token,
        client_id,
        user_id: s.user_id,
        login: s.login,
        expires_at: s.expires_at,
    }))
}

pub async fn load_obs_and_connect(
    creds: Arc<dyn CredentialsRepo>,
    bus: Arc<EventBus>,
) -> Result<ObsClientRef, String> {
    let publisher: Arc<dyn EventPublisher> = bus;
    let client = forge_obs::credentials::load_and_connect(&*creds, publisher)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ObsClientRef::new(client))
}

pub(crate) fn handle_twitch_boot_result(
    app: &mut App,
    result: Result<Option<TwitchBootBundle>, String>,
) -> Task<Message> {
    match result {
        Ok(Some(bundle)) => {
            let login = if bundle.login.is_empty() {
                None
            } else {
                Some(bundle.login.clone())
            };
            let tracker = SubscriptionTracker::default();
            let chat = TwitchChat::new(
                bundle.access_token,
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
    }
}

pub(crate) fn handle_obs_boot_result(
    app: &mut App,
    result: Result<ObsClientRef, String>,
) -> Task<Message> {
    match result {
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
    }
}

pub(crate) fn handle_vtube_boot_result(
    app: &mut App,
    result: Result<VTubeClientRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let client = handle.into_arc();
            let id = BuiltinId::new("vtube");
            let icon = SectionIcon::new("mood-smile");
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
            app.rt.vtube_client = Some(client);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "VTube Studio boot connection failed");
            Task::none()
        }
    }
}

pub(crate) fn handle_discord_boot_result(
    app: &mut App,
    result: Result<DiscordClientRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let client = handle.into_arc();
            let id = BuiltinId::new("discord");
            let icon = SectionIcon::new("brand-discord");
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
            app.rt.discord_client = Some(client);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "Discord boot setup failed");
            Task::none()
        }
    }
}

pub(crate) fn handle_midi_boot_result(
    app: &mut App,
    result: Result<MidiClientRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let client = handle.into_arc();
            let id = BuiltinId::new("midi");
            let icon = SectionIcon::new("piano");
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
            app.rt.midi_client = Some(client);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "MIDI boot setup failed");
            Task::none()
        }
    }
}

pub(crate) fn handle_hotkey_boot_result(
    app: &mut App,
    result: Result<HotkeyClientRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let client = handle.into_arc();
            let id = BuiltinId::new("hotkey");
            let icon = SectionIcon::new("keyboard");
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
            app.rt.hotkey_client = Some(client);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "hotkey boot setup failed");
            Task::none()
        }
    }
}

pub(crate) fn handle_server_boot_result(
    app: &mut App,
    result: Result<crate::server_subsystem::ServerBootSnapshot, String>,
) -> Task<Message> {
    match result {
        Ok(snapshot) => {
            app.ui.server_screen.bind_address = snapshot.bind_address;
            app.ui.server_screen.bearer_token = snapshot.bearer_token;
            app.ui.server_screen.server_status = ServerStatus::Running;
        }
        Err(e) => {
            tracing::warn!(error = %e, "server boot failed");
            app.ui.server_screen.server_status = ServerStatus::Error(e);
        }
    }
    Task::none()
}

pub(crate) fn handle_server_restart_result(
    app: &mut App,
    result: Result<(), String>,
) -> Task<Message> {
    match result {
        Ok(()) => {
            app.ui.server_screen.server_status = ServerStatus::Running;
        }
        Err(e) => {
            tracing::warn!(error = %e, "server restart failed");
            app.ui.server_screen.server_status = ServerStatus::Error(e);
        }
    }
    Task::none()
}

pub(crate) fn handle_server_stop_result(
    app: &mut App,
    result: Result<(), String>,
) -> Task<Message> {
    match result {
        Ok(()) => {
            app.ui.server_screen.server_status = ServerStatus::Stopped;
            app.ui.server_screen.connected_clients.clear();
        }
        Err(e) => {
            tracing::warn!(error = %e, "server stop failed");
            app.ui.server_screen.server_status = ServerStatus::Error(e);
        }
    }
    Task::none()
}

pub(crate) fn handle_server_token_rotated(
    app: &mut App,
    result: Result<String, String>,
) -> Task<Message> {
    match result {
        Ok(token) => {
            app.ui.server_screen.bearer_token = token;
        }
        Err(e) => {
            tracing::warn!(error = %e, "token regeneration failed");
            app.ui.server_screen.server_status = ServerStatus::Error(e);
        }
    }
    Task::none()
}

pub(crate) fn handle_server_restart_command(app: &App) -> Task<Message> {
    let subsystem = Arc::clone(&app.rt.server_subsystem);
    Task::perform(
        async move { subsystem.restart().await.map_err(|e| e.to_string()) },
        |r| Message::ServerSubsystem(ServerSubsystemMsg::RestartResult(r)),
    )
}

pub(crate) fn handle_server_stop_command(app: &App) -> Task<Message> {
    let subsystem = Arc::clone(&app.rt.server_subsystem);
    Task::perform(
        async move { subsystem.stop().await.map_err(|e| e.to_string()) },
        |r| Message::ServerSubsystem(ServerSubsystemMsg::StopResult(r)),
    )
}

pub(crate) fn handle_twitch_reauth_requested(app: &mut App) -> Task<Message> {
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

pub(crate) fn handle_server_regenerate_token(app: &App) -> Task<Message> {
    let subsystem = Arc::clone(&app.rt.server_subsystem);
    Task::perform(
        async move {
            subsystem
                .regenerate_token()
                .await
                .map_err(|e| e.to_string())
        },
        |r| Message::ServerSubsystem(ServerSubsystemMsg::TokenRotated(r)),
    )
}
