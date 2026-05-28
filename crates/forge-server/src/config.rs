use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_platform_core::paths;
use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::{
    ActionRepo, CredentialsRepo, GlobalsRepo, SettingsRepo, StorageError, UserGlobalsRepo,
    reserved_keys,
};

const VALID_BIND_ADDRESSES: &[&str] = &["127.0.0.1", "0.0.0.0", "::1", "::"];

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub overlay_root: PathBuf,
    pub auth_required_for_reads: bool,
    pub http_overlay_require_token: bool,
    pub overlay_cors_any_origin: bool,
    pub lan_bind_enabled: bool,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub event_bus: Arc<EventBus>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub action_engine: Arc<ActionEngineHandle>,
}

impl ServerConfig {
    pub fn new(
        credentials: Arc<dyn CredentialsRepo>,
        event_bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        globals: Arc<dyn GlobalsRepo>,
        user_globals: Arc<dyn UserGlobalsRepo>,
        action_engine: Arc<ActionEngineHandle>,
    ) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9515),
            overlay_root: paths::data_dir().join("overlays"),
            auth_required_for_reads: false,
            http_overlay_require_token: false,
            overlay_cors_any_origin: true,
            lan_bind_enabled: false,
            credentials,
            event_bus,
            actions,
            globals,
            user_globals,
            action_engine,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub bind_address: String,
    pub port: u16,
    pub lan_bind_enabled: bool,
    pub auth_required_for_reads: bool,
    pub http_overlay_require_token: bool,
    pub overlay_cors_any_origin: bool,
    pub overlay_root: Option<String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_owned(),
            port: 8081,
            lan_bind_enabled: false,
            auth_required_for_reads: false,
            http_overlay_require_token: false,
            overlay_cors_any_origin: true,
            overlay_root: None,
        }
    }
}

impl ServerSettings {
    pub async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        let bind_address = repo
            .get_string(reserved_keys::SERVER_BIND_ADDRESS_KEY)
            .await?
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let port = repo
            .get_string(reserved_keys::SERVER_PORT_KEY)
            .await?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8081u16);
        let lan_bind_enabled = repo
            .get_string(reserved_keys::SERVER_LAN_BIND_ENABLED_KEY)
            .await?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false);
        let auth_required_for_reads = repo
            .get_string(reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS_KEY)
            .await?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false);
        let http_overlay_require_token = repo
            .get_string(reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY)
            .await?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false);
        let overlay_cors_any_origin = repo
            .get_string(reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY)
            .await?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true);
        let overlay_root = repo
            .get_string(reserved_keys::SERVER_OVERLAY_ROOT_KEY)
            .await?;
        Ok(Self {
            bind_address,
            port,
            lan_bind_enabled,
            auth_required_for_reads,
            http_overlay_require_token,
            overlay_cors_any_origin,
            overlay_root,
        })
    }

    pub async fn save_bind_address(
        repo: &dyn SettingsRepo,
        addr: &str,
    ) -> Result<(), StorageError> {
        if !VALID_BIND_ADDRESSES.contains(&addr) {
            return Err(StorageError::ValidationFailed {
                field: "server.bind_address".to_owned(),
                reason: format!("must be one of: {}", VALID_BIND_ADDRESSES.join(", ")),
            });
        }
        repo.set_string(reserved_keys::SERVER_BIND_ADDRESS_KEY, addr)
            .await
    }

    pub async fn save_port(repo: &dyn SettingsRepo, port: u16) -> Result<(), StorageError> {
        repo.set_string(reserved_keys::SERVER_PORT_KEY, &port.to_string())
            .await
    }

    pub async fn save_lan_bind_enabled(
        repo: &dyn SettingsRepo,
        enabled: bool,
    ) -> Result<(), StorageError> {
        repo.set_string(
            reserved_keys::SERVER_LAN_BIND_ENABLED_KEY,
            if enabled { "true" } else { "false" },
        )
        .await
    }

    pub async fn save_auth_required_for_reads(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        repo.set_string(
            reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS_KEY,
            if required { "true" } else { "false" },
        )
        .await
    }

    pub async fn save_http_overlay_require_token(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        repo.set_string(
            reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY,
            if required { "true" } else { "false" },
        )
        .await
    }

    pub async fn save_overlay_cors_any_origin(
        repo: &dyn SettingsRepo,
        allow: bool,
    ) -> Result<(), StorageError> {
        repo.set_string(
            reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY,
            if allow { "true" } else { "false" },
        )
        .await
    }

    pub async fn save_overlay_root(
        repo: &dyn SettingsRepo,
        path: &str,
    ) -> Result<(), StorageError> {
        repo.set_string(reserved_keys::SERVER_OVERLAY_ROOT_KEY, path)
            .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_storage::StorageError;

    use super::*;

    struct MapRepo(Mutex<HashMap<String, String>>);

    impl MapRepo {
        fn empty() -> Self {
            Self(Mutex::new(HashMap::new()))
        }
    }

    #[async_trait]
    impl SettingsRepo for MapRepo {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }

        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(key).is_some())
        }

        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn defaults_when_no_keys_stored() {
        let repo = MapRepo::empty();
        let s = ServerSettings::load(&repo).await.unwrap();
        assert_eq!(s.bind_address, "127.0.0.1");
        assert_eq!(s.port, 8081);
        assert!(!s.lan_bind_enabled);
        assert!(!s.auth_required_for_reads);
        assert!(!s.http_overlay_require_token);
        assert!(s.overlay_cors_any_origin);
        assert!(s.overlay_root.is_none());
    }

    #[tokio::test]
    async fn load_roundtrip_all_fields() {
        let repo = MapRepo::empty();
        ServerSettings::save_bind_address(&repo, "0.0.0.0")
            .await
            .unwrap();
        ServerSettings::save_port(&repo, 9000).await.unwrap();
        ServerSettings::save_lan_bind_enabled(&repo, true)
            .await
            .unwrap();
        ServerSettings::save_auth_required_for_reads(&repo, true)
            .await
            .unwrap();
        ServerSettings::save_http_overlay_require_token(&repo, true)
            .await
            .unwrap();
        ServerSettings::save_overlay_cors_any_origin(&repo, false)
            .await
            .unwrap();
        ServerSettings::save_overlay_root(&repo, "/overlays")
            .await
            .unwrap();

        let s = ServerSettings::load(&repo).await.unwrap();
        assert_eq!(s.bind_address, "0.0.0.0");
        assert_eq!(s.port, 9000);
        assert!(s.lan_bind_enabled);
        assert!(s.auth_required_for_reads);
        assert!(s.http_overlay_require_token);
        assert!(!s.overlay_cors_any_origin);
        assert_eq!(s.overlay_root.as_deref(), Some("/overlays"));
    }

    #[tokio::test]
    async fn save_bind_address_rejects_non_allowlisted() {
        let repo = MapRepo::empty();
        let err = ServerSettings::save_bind_address(&repo, "192.168.1.100")
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::ValidationFailed { .. }));
    }

    #[tokio::test]
    async fn save_bind_address_accepts_all_valid_values() {
        let repo = MapRepo::empty();
        for addr in ["127.0.0.1", "0.0.0.0", "::1", "::"] {
            ServerSettings::save_bind_address(&repo, addr)
                .await
                .unwrap();
            let s = ServerSettings::load(&repo).await.unwrap();
            assert_eq!(s.bind_address, addr);
        }
    }
}
