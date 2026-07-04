use std::sync::Arc;

use forge_events::EventPublisher;
use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyConfig};
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId, BuiltinStatus, QuickActions,
    SectionIcon,
};
use forge_platform_twitch::{
    ChatSessionConfig, SubscriptionTracker, TwitchChat, TwitchIntegrationBundle,
};
use forge_runtime::EventBus;
use forge_storage::{CredentialsRepo, DataProvider};
use forge_types::Variant;
use iced::Task;

use crate::app::App;
use crate::builtin_detail::BuiltinDetailState;
use crate::message::{
    DiscordClientRef, HotkeyClientRef, KickBundleRef, Message, MidiClientRef, ObsClientRef,
    ServerSubsystemMsg, TwitchBootBundle, VTubeClientRef, YoutubeBundleRef,
};
use crate::server_screen::ServerStatus;

const HOTKEY_PRESSED_KIND: &str = "hotkey.global.pressed";

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

pub async fn load_vtube_and_connect(
    creds: Arc<dyn CredentialsRepo>,
    bus: Arc<EventBus>,
) -> Result<VTubeClientRef, String> {
    let publisher: Arc<dyn EventPublisher> = bus;
    let creds_arc: Arc<dyn CredentialsRepo> = Arc::clone(&creds);
    let client = forge_vtube::credentials::load_and_connect(&*creds, publisher, creds_arc)
        .await
        .map_err(|e| e.to_string())?;
    Ok(VTubeClientRef::new(client))
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

pub async fn load_hotkey_and_register(
    backend: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
) -> HotkeyClientRef {
    let publisher: Arc<dyn EventPublisher> = bus;
    let client = HotkeyClient::new(HotkeyConfig::default(), publisher).await;
    reregister_persisted_hotkeys(&client, &backend).await;
    HotkeyClientRef::new(client)
}

async fn reregister_persisted_hotkeys(client: &Arc<HotkeyClient>, backend: &Arc<dyn DataProvider>) {
    let instances = match backend.trigger_instance_repo().list_all().await {
        Ok(instances) => instances,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load persisted hotkey bindings at boot");
            return;
        }
    };

    for instance in instances {
        if instance.kind_id != HOTKEY_PRESSED_KIND {
            continue;
        }
        let Some(Variant::String(combo_str)) = instance.overrides.get("combo") else {
            continue;
        };
        let combo = match HotkeyCombo::parse(combo_str) {
            Ok(combo) => combo,
            Err(e) => {
                tracing::warn!(combo = %combo_str, error = %e, "persisted hotkey combo failed to parse");
                continue;
            }
        };
        if let Err(e) = client.register(combo).await {
            tracing::warn!(combo = %combo_str, error = %e, "failed to re-register hotkey at boot");
        }
    }
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
            let config = ChatSessionConfig {
                client_id: bundle.client_id,
                broadcaster_id: bundle.user_id.clone(),
                user_id: bundle.user_id,
            };
            let chat = TwitchChat::new(
                bundle.access_token,
                config.client_id.clone(),
                config.broadcaster_id.clone(),
                config.user_id.clone(),
                Arc::clone(&app.rt.bus) as Arc<dyn EventPublisher>,
                Arc::clone(&tracker),
            );
            let handle = chat.start();
            let creds: Arc<dyn CredentialsRepo> =
                Arc::clone(&app.rt.backend) as Arc<dyn CredentialsRepo>;
            let twitch_bundle = TwitchIntegrationBundle::new(
                login.clone(),
                config,
                Arc::clone(&app.rt.bus) as Arc<dyn EventPublisher>,
                creds,
                tracker,
                handle,
            );
            let id = BuiltinId::new("twitch");
            let icon = SectionIcon::new("brand-twitch");
            let status: Arc<dyn BuiltinStatus> = twitch_bundle.clone();
            let health: Arc<dyn BuiltinHealth> = twitch_bundle.clone();
            let content: Arc<dyn BuiltinContent> = twitch_bundle.clone();
            let quick_actions: Arc<dyn QuickActions> = twitch_bundle.clone();
            let control: Option<Arc<dyn BuiltinControl>> =
                Some(twitch_bundle.clone() as Arc<dyn BuiltinControl>);
            app.ui.builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
                control,
            ));
            app.rt.twitch_builtin = Some(twitch_bundle);
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

pub(crate) fn handle_kick_boot_result(
    app: &mut App,
    result: Result<KickBundleRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let bundle = handle.into_arc();
            let id = BuiltinId::new("kick");
            let icon = SectionIcon::new("brand-kick");
            let status: Arc<dyn BuiltinStatus> = bundle.clone();
            let health: Arc<dyn BuiltinHealth> = bundle.clone();
            let content: Arc<dyn BuiltinContent> = bundle.clone();
            let quick_actions: Arc<dyn QuickActions> = bundle.clone();
            let control: Option<Arc<dyn BuiltinControl>> =
                Some(bundle.clone() as Arc<dyn BuiltinControl>);
            app.ui.builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
                control,
            ));
            app.rt.kick_builtin = Some(bundle);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "kick boot setup failed");
            Task::none()
        }
    }
}

pub(crate) fn handle_youtube_boot_result(
    app: &mut App,
    result: Result<YoutubeBundleRef, String>,
) -> Task<Message> {
    match result {
        Ok(handle) => {
            let bundle = handle.into_arc();
            let id = BuiltinId::new("youtube");
            let icon = SectionIcon::new("brand-youtube");
            let status: Arc<dyn BuiltinStatus> = bundle.clone();
            let health: Arc<dyn BuiltinHealth> = bundle.clone();
            let content: Arc<dyn BuiltinContent> = bundle.clone();
            let quick_actions: Arc<dyn QuickActions> = bundle.clone();
            let control: Option<Arc<dyn BuiltinControl>> =
                Some(bundle.clone() as Arc<dyn BuiltinControl>);
            app.ui.builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
                control,
            ));
            app.rt.youtube_builtin = Some(bundle);
            Task::none()
        }
        Err(e) => {
            tracing::warn!(error = %e, "youtube boot setup failed");
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
            let control: Option<Arc<dyn BuiltinControl>> =
                Some(client.clone() as Arc<dyn BuiltinControl>);
            app.ui.builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
                control,
            ));
            app.rt.obs_sink.install(Arc::clone(&client));
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
            let control: Option<Arc<dyn BuiltinControl>> =
                Some(client.clone() as Arc<dyn BuiltinControl>);
            app.ui.builtin_detail = Some(BuiltinDetailState::new(
                id,
                icon,
                status,
                health,
                content,
                quick_actions,
                control,
            ));
            app.rt.vtube_sink.install(Arc::clone(&client));
            app.rt.vtube_client = Some(client);
            Task::none()
        }
        Err(e) => {
            tracing::debug!(error = %e, "VTube Studio not started at boot");
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
                None,
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
                None,
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
                None,
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
            app.ui.server_screen.server_status = if snapshot.started {
                ServerStatus::Running
            } else {
                ServerStatus::Stopped
            };
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
