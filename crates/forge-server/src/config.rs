use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_platform_core::paths;
use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::{CredentialsRepo, DataProvider};

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub overlay_root: PathBuf,
    pub auth_required_for_reads: bool,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub event_bus: Arc<EventBus>,
    pub data_provider: Arc<dyn DataProvider>,
    pub action_engine: Arc<ActionEngineHandle>,
}

impl ServerConfig {
    pub fn new(
        credentials: Arc<dyn CredentialsRepo>,
        event_bus: Arc<EventBus>,
        data_provider: Arc<dyn DataProvider>,
        action_engine: Arc<ActionEngineHandle>,
    ) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9515),
            overlay_root: paths::data_dir().join("overlays"),
            auth_required_for_reads: false,
            credentials,
            event_bus,
            data_provider,
            action_engine,
        }
    }
}
