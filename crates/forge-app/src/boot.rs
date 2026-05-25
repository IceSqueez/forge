use std::sync::Arc;

use forge_events::EventPublisher;
use forge_obs::ObsClient;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, QuickActions, SectionIcon,
};
use forge_platform_twitch::{SubscriptionTracker, TwitchChat, TwitchIntegrationBundle, client_id};
use forge_runtime::EventBus;
use forge_storage::{CredentialId, DataProvider};
use forge_types::OAuthToken;
use iced::Task;

use crate::app::App;
use crate::builtin_detail::BuiltinDetailState;
use crate::message::{Message, ObsClientRef, TwitchBootBundle};
use crate::server_screen::ServerStatus;

pub(crate) async fn reconnect_twitch(
    backend: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
) -> Result<(), String> {
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

    let token = OAuthToken::new(access);
    let tracker = SubscriptionTracker::default();
    TwitchChat::new(token, cid, user_id.clone(), user_id, bus, tracker).start();
    Ok(())
}

pub async fn load_twitch_credential(
    backend: Arc<dyn DataProvider>,
) -> Result<Option<TwitchBootBundle>, String> {
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
    Ok(Some(TwitchBootBundle {
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
                OAuthToken::new(bundle.access_token),
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
        Message::ServerRestartResult,
    )
}

pub(crate) fn handle_server_stop_command(app: &App) -> Task<Message> {
    let subsystem = Arc::clone(&app.rt.server_subsystem);
    Task::perform(
        async move { subsystem.stop().await.map_err(|e| e.to_string()) },
        Message::ServerStopResult,
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
        Message::ServerTokenRotated,
    )
}
