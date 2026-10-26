use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_runtime::{ActionEngineHandle, EventBus};
use forge_storage::paths;
use forge_storage::{
    ActionRepo, CredentialsRepo, GlobalsRepo, OverlayRepo,
    SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT, SettingsRepo, StorageError, UserGlobalsRepo,
    get_bool_setting, reserved_keys, set_bool_setting, set_json_setting, stored_json_setting,
};

const VALID_BIND_ADDRESSES: &[&str] = &["127.0.0.1", "0.0.0.0", "::1", "::"];

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub overlay_root: PathBuf,
    pub auth_required_for_reads: bool,
    pub overlay_cors_any_origin: bool,
    pub lan_bind_enabled: bool,
    pub additional_origins: Vec<String>,
    pub settings: Arc<dyn SettingsRepo>,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub event_bus: Arc<EventBus>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub overlays: Arc<dyn OverlayRepo>,
    pub action_engine: Arc<ActionEngineHandle>,
}

impl ServerConfig {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        credentials: Arc<dyn CredentialsRepo>,
        event_bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        globals: Arc<dyn GlobalsRepo>,
        user_globals: Arc<dyn UserGlobalsRepo>,
        overlays: Arc<dyn OverlayRepo>,
        action_engine: Arc<ActionEngineHandle>,
    ) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9515),
            overlay_root: paths::overlays_dir(),
            auth_required_for_reads: false,
            overlay_cors_any_origin: true,
            lan_bind_enabled: false,
            additional_origins: Vec::new(),
            settings,
            credentials,
            event_bus,
            actions,
            globals,
            user_globals,
            overlays,
            action_engine,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub enabled: bool,
    pub bind_address: String,
    pub port: u16,
    pub lan_bind_enabled: bool,
    pub auth_required_for_reads: bool,
    pub http_overlay_require_token: bool,
    pub overlay_cors_any_origin: bool,
    pub overlay_root: Option<String>,
    pub additional_origins: Vec<String>,
    pub additional_origins_unreadable: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "127.0.0.1".to_owned(),
            port: 8081,
            lan_bind_enabled: false,
            auth_required_for_reads: false,
            http_overlay_require_token: SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
            overlay_cors_any_origin: true,
            overlay_root: None,
            additional_origins: Vec::new(),
            additional_origins_unreadable: false,
        }
    }
}

async fn load_additional_origins(
    repo: &dyn SettingsRepo,
) -> Result<(Vec<String>, bool), StorageError> {
    match stored_json_setting::<Vec<String>>(repo, reserved_keys::SERVER_ADDITIONAL_ORIGINS).await {
        Ok(origins) => Ok((origins.unwrap_or_default(), false)),
        Err(StorageError::UndecodableSetting { key }) => {
            tracing::warn!(
                key = %key,
                "stored additional origins cannot be decoded; serving the default origins only"
            );
            Ok((Vec::new(), true))
        }
        Err(e) => Err(e),
    }
}

impl ServerSettings {
    pub async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        let enabled = get_bool_setting(repo, reserved_keys::SERVER_ENABLED, true).await;
        let bind_address = repo
            .get_string(reserved_keys::SERVER_BIND_ADDRESS)
            .await?
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let port = repo
            .get_string(reserved_keys::SERVER_PORT)
            .await?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8081u16);
        let lan_bind_enabled =
            get_bool_setting(repo, reserved_keys::SERVER_LAN_BIND_ENABLED, false).await;
        let auth_required_for_reads =
            get_bool_setting(repo, reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS, false).await;
        let http_overlay_require_token = get_bool_setting(
            repo,
            reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN,
            SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
        )
        .await;
        let overlay_cors_any_origin =
            get_bool_setting(repo, reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN, true).await;
        let overlay_root = repo.get_string(reserved_keys::SERVER_OVERLAY_ROOT).await?;
        let (additional_origins, additional_origins_unreadable) =
            load_additional_origins(repo).await?;
        Ok(Self {
            enabled,
            bind_address,
            port,
            lan_bind_enabled,
            auth_required_for_reads,
            http_overlay_require_token,
            overlay_cors_any_origin,
            overlay_root,
            additional_origins,
            additional_origins_unreadable,
        })
    }

    pub async fn save_enabled(repo: &dyn SettingsRepo, enabled: bool) -> Result<(), StorageError> {
        set_bool_setting(repo, reserved_keys::SERVER_ENABLED, enabled).await
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
        repo.set_string(reserved_keys::SERVER_BIND_ADDRESS, addr)
            .await
    }

    pub async fn save_port(repo: &dyn SettingsRepo, port: u16) -> Result<(), StorageError> {
        repo.set_string(reserved_keys::SERVER_PORT, &port.to_string())
            .await
    }

    pub async fn save_lan_bind_enabled(
        repo: &dyn SettingsRepo,
        enabled: bool,
    ) -> Result<(), StorageError> {
        set_bool_setting(repo, reserved_keys::SERVER_LAN_BIND_ENABLED, enabled).await
    }

    pub async fn save_auth_required_for_reads(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        set_bool_setting(
            repo,
            reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS,
            required,
        )
        .await
    }

    pub async fn save_http_overlay_require_token(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        set_bool_setting(
            repo,
            reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN,
            required,
        )
        .await
    }

    pub async fn save_overlay_cors_any_origin(
        repo: &dyn SettingsRepo,
        allow: bool,
    ) -> Result<(), StorageError> {
        set_bool_setting(repo, reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN, allow).await
    }

    pub async fn save_overlay_root(
        repo: &dyn SettingsRepo,
        path: &str,
    ) -> Result<(), StorageError> {
        repo.set_string(reserved_keys::SERVER_OVERLAY_ROOT, path)
            .await
    }

    pub async fn save_additional_origins(
        repo: &dyn SettingsRepo,
        origins: &[String],
    ) -> Result<(), StorageError> {
        set_json_setting(repo, reserved_keys::SERVER_ADDITIONAL_ORIGINS, &origins).await
    }
}

#[cfg(test)]
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
        assert!(
            s.http_overlay_require_token,
            "an unset key must keep overlay files behind the token"
        );
        assert!(s.overlay_cors_any_origin);
        assert!(s.overlay_root.is_none());
        assert!(s.additional_origins.is_empty());
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
        ServerSettings::save_http_overlay_require_token(&repo, false)
            .await
            .unwrap();
        ServerSettings::save_overlay_cors_any_origin(&repo, false)
            .await
            .unwrap();
        ServerSettings::save_overlay_root(&repo, "/overlays")
            .await
            .unwrap();
        ServerSettings::save_additional_origins(
            &repo,
            &["https://overlay.example.com".to_owned(), "null".to_owned()],
        )
        .await
        .unwrap();

        let s = ServerSettings::load(&repo).await.unwrap();
        assert_eq!(s.bind_address, "0.0.0.0");
        assert_eq!(s.port, 9000);
        assert!(s.lan_bind_enabled);
        assert!(s.auth_required_for_reads);
        assert!(!s.http_overlay_require_token);
        assert!(!s.overlay_cors_any_origin);
        assert_eq!(s.overlay_root.as_deref(), Some("/overlays"));
        assert_eq!(
            s.additional_origins,
            ["https://overlay.example.com", "null"]
        );
    }

    struct UnreachableOrigins(MapRepo);

    #[async_trait]
    impl SettingsRepo for UnreachableOrigins {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            if key == reserved_keys::SERVER_ADDITIONAL_ORIGINS {
                return Err(StorageError::Connection {
                    reason: "settings store unreachable".to_owned(),
                });
            }
            self.0.get_string(key).await
        }

        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0.set_string(key, value).await
        }

        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            self.0.delete(key).await
        }

        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            self.0.load_all().await
        }
    }

    #[tokio::test]
    async fn load_flags_undecodable_origins_and_still_serves_the_other_settings() {
        for (stored, origins, unreadable) in [
            (None, Vec::<&str>::new(), false),
            (
                Some(r#"["https://overlay.example.com"]"#),
                vec!["https://overlay.example.com"],
                false,
            ),
            (Some("https://overlay.example.com"), Vec::new(), true),
            (
                Some(r#"{"origin":"https://overlay.example.com"}"#),
                Vec::new(),
                true,
            ),
        ] {
            let repo = MapRepo::empty();
            ServerSettings::save_port(&repo, 9000).await.unwrap();
            if let Some(raw) = stored {
                repo.set_string(reserved_keys::SERVER_ADDITIONAL_ORIGINS, raw)
                    .await
                    .unwrap();
            }

            let s = ServerSettings::load(&repo).await.unwrap();

            assert_eq!(
                (
                    s.additional_origins,
                    s.additional_origins_unreadable,
                    s.port
                ),
                (
                    origins.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                    unreadable,
                    9000
                ),
                "{stored:?}"
            );
        }
    }

    #[tokio::test]
    async fn load_fails_when_the_origins_cannot_be_read_from_the_store() {
        let repo = UnreachableOrigins(MapRepo::empty());

        let loaded = ServerSettings::load(&repo).await;

        assert!(matches!(loaded, Err(StorageError::Connection { .. })));
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
