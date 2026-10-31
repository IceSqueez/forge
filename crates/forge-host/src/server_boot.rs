use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_runtime::{ActionEngineHandle, EventBus, EventHistory};
use forge_server::{
    DeliveryHub, ServerConfig, ServerError, ServerHandle, ServerSettings, start_server,
    stopped_server,
};
use forge_storage::{
    CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, StorageError, UserGlobalsRepo,
};
use forge_types::PlatformId;

use crate::boot_signal;
use crate::boot_warnings::{BootWarning, BootWarnings};

#[derive(Debug)]
pub enum ServerUnavailable {
    Settings(StorageError),
    State(ServerError),
}

pub(crate) struct ServerBoot {
    pub(crate) handle: Option<ServerHandle>,
    pub(crate) unavailable: Option<ServerUnavailable>,
}

impl ServerBoot {
    fn running(handle: ServerHandle) -> Self {
        Self {
            handle: Some(handle),
            unavailable: None,
        }
    }

    fn unavailable(reason: ServerUnavailable) -> Self {
        Self {
            handle: None,
            unavailable: Some(reason),
        }
    }
}

pub(crate) struct ServerInputs<'a> {
    pub(crate) backend: &'a Arc<dyn DataProvider>,
    pub(crate) bus: &'a Arc<EventBus>,
    pub(crate) action_engine: &'a ActionEngineHandle,
    pub(crate) available_platforms: BTreeSet<PlatformId>,
    pub(crate) delivery_hub: &'a DeliveryHub,
}

pub(crate) async fn build_server(
    inputs: ServerInputs<'_>,
    warnings: &mut BootWarnings,
) -> ServerBoot {
    let ServerInputs {
        backend,
        bus,
        action_engine,
        available_platforms,
        delivery_hub,
    } = inputs;
    let settings = match ServerSettings::load(backend.as_ref()).await {
        Ok(settings) => settings,
        Err(e) => {
            boot_signal::server_settings_unavailable(&e);
            return ServerBoot::unavailable(ServerUnavailable::Settings(e));
        }
    };
    let bind_addr = match settings.bind_address.parse::<IpAddr>() {
        Ok(ip) => Some(SocketAddr::new(ip, settings.port)),
        Err(e) => {
            tracing::warn!(
                bind_address = %settings.bind_address,
                error = %e,
                "invalid server bind address"
            );
            None
        }
    };

    let build_config = || {
        let settings_repo: Arc<dyn SettingsRepo> = Arc::clone(backend) as Arc<dyn SettingsRepo>;
        let credentials: Arc<dyn CredentialsRepo> = Arc::clone(backend) as Arc<dyn CredentialsRepo>;
        let globals: Arc<dyn GlobalsRepo> = Arc::clone(backend) as Arc<dyn GlobalsRepo>;
        let user_globals: Arc<dyn UserGlobalsRepo> =
            Arc::clone(backend) as Arc<dyn UserGlobalsRepo>;
        let event_history: Arc<dyn EventHistory> = Arc::clone(bus) as Arc<dyn EventHistory>;
        let mut config = ServerConfig::new(
            settings_repo,
            credentials,
            event_history,
            backend.action_repo(),
            globals,
            user_globals,
            backend.overlay_repo(),
            Arc::new(action_engine.clone()),
        );
        if let Some(bind_addr) = bind_addr {
            config.bind_addr = bind_addr;
        }
        config.auth_required_for_reads = settings.auth_required_for_reads;
        config.lan_bind_enabled = settings.lan_bind_enabled;
        config.overlay_cors_any_origin = settings.overlay_cors_any_origin;
        config.additional_origins = settings.additional_origins.clone();
        config.available_platforms = available_platforms.clone();
        config.delivery_hub = Some(delivery_hub.clone());
        if let Some(root) = settings
            .overlay_root
            .as_ref()
            .filter(|root| !root.is_empty())
        {
            config.overlay_root = PathBuf::from(root);
        }
        config
    };

    match (settings.enabled, bind_addr) {
        (true, Some(addr)) => match start_server(build_config()).await {
            Ok(handle) => {
                boot_signal::server_bound(handle.bind_addr().await);
                return ServerBoot::running(handle);
            }
            Err(e) => boot_signal::server_bind_failed(addr, &e),
        },
        (true, None) => {
            boot_signal::server_address_invalid(&settings.bind_address);
            warnings.record(BootWarning::ServerBindAddress);
        }
        (false, _) => boot_signal::server_disabled(),
    }

    match stopped_server(build_config()).await {
        Ok(handle) => ServerBoot::running(handle),
        Err(e) => {
            tracing::error!(error = %e, "server state unavailable, leaving it off");
            ServerBoot::unavailable(ServerUnavailable::State(e))
        }
    }
}
