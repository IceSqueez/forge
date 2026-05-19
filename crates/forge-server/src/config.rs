use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_platform_core::paths;
use forge_runtime::EventBus;
use forge_storage::CredentialsRepo;

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub overlay_root: PathBuf,
    pub auth_required_for_reads: bool,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub event_bus: Arc<EventBus>,
}

impl ServerConfig {
    pub fn new(credentials: Arc<dyn CredentialsRepo>, event_bus: Arc<EventBus>) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9515),
            overlay_root: paths::data_dir().join("overlays"),
            auth_required_for_reads: false,
            credentials,
            event_bus,
        }
    }
}
