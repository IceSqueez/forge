use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use forge_runtime::{ActionEngineHandle, EventHistory};
use forge_storage::{
    ActionRepo, CredentialsRepo, GlobalsRepo, OverlayRepo, SettingsRepo, StorageError,
    UserGlobalsRepo, read_setting, write_setting,
};
use forge_types::PlatformId;

use crate::DeliveryHub;
use crate::settings::{self, StoredOrigins};
use forge_storage::settings::core_settings;

pub const LOCALHOST_BIND_ADDRESS: &str = "127.0.0.1";
pub const LAN_BIND_ADDRESS: &str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 8081;

const VALID_BIND_ADDRESSES: &[&str] = &[LOCALHOST_BIND_ADDRESS, LAN_BIND_ADDRESS, "::1", "::"];

pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub overlay_root: PathBuf,
    pub auth_required_for_reads: bool,
    pub overlay_cors_any_origin: bool,
    pub lan_bind_enabled: bool,
    pub additional_origins: Vec<String>,
    pub available_platforms: BTreeSet<PlatformId>,
    pub settings: Arc<dyn SettingsRepo>,
    pub credentials: Arc<dyn CredentialsRepo>,
    pub event_bus: Arc<dyn EventHistory>,
    pub actions: Arc<dyn ActionRepo>,
    pub globals: Arc<dyn GlobalsRepo>,
    pub user_globals: Arc<dyn UserGlobalsRepo>,
    pub overlays: Arc<dyn OverlayRepo>,
    pub action_engine: Arc<ActionEngineHandle>,
    pub delivery_hub: Option<DeliveryHub>,
}

impl ServerConfig {
    #[expect(
        clippy::too_many_arguments,
        reason = "constructor receives each shared handle it wires"
    )]
    pub fn new(
        settings: Arc<dyn SettingsRepo>,
        credentials: Arc<dyn CredentialsRepo>,
        event_bus: Arc<dyn EventHistory>,
        actions: Arc<dyn ActionRepo>,
        globals: Arc<dyn GlobalsRepo>,
        user_globals: Arc<dyn UserGlobalsRepo>,
        overlays: Arc<dyn OverlayRepo>,
        action_engine: Arc<ActionEngineHandle>,
    ) -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_PORT),
            overlay_root: forge_paths::overlays_dir(),
            auth_required_for_reads: false,
            overlay_cors_any_origin: true,
            lan_bind_enabled: false,
            additional_origins: Vec::new(),
            available_platforms: BTreeSet::new(),
            settings,
            credentials,
            event_bus,
            actions,
            globals,
            user_globals,
            overlays,
            action_engine,
            delivery_hub: None,
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
            enabled: settings::ENABLED.codec().default,
            bind_address: LOCALHOST_BIND_ADDRESS.to_owned(),
            port: DEFAULT_PORT,
            lan_bind_enabled: settings::LAN_BIND_ENABLED.codec().default,
            auth_required_for_reads: settings::AUTH_REQUIRED_FOR_READS.codec().default,
            http_overlay_require_token: settings::HTTP_OVERLAY_REQUIRE_TOKEN.codec().default,
            overlay_cors_any_origin: settings::OVERLAY_CORS_ANY_ORIGIN.codec().default,
            overlay_root: None,
            additional_origins: Vec::new(),
            additional_origins_unreadable: false,
        }
    }
}

impl ServerSettings {
    pub async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        let enabled = read_setting(repo, &settings::ENABLED).await?;
        let bind_address = read_setting(repo, &settings::BIND_ADDRESS).await?;
        let port =
            u16::try_from(read_setting(repo, &settings::PORT).await?).unwrap_or(DEFAULT_PORT);
        let lan_bind_enabled = read_setting(repo, &settings::LAN_BIND_ENABLED).await?;
        let auth_required_for_reads =
            read_setting(repo, &settings::AUTH_REQUIRED_FOR_READS).await?;
        let http_overlay_require_token =
            read_setting(repo, &settings::HTTP_OVERLAY_REQUIRE_TOKEN).await?;
        let overlay_cors_any_origin =
            read_setting(repo, &settings::OVERLAY_CORS_ANY_ORIGIN).await?;
        let overlay_root = read_setting(repo, &core_settings::SERVER_OVERLAY_ROOT).await?;
        let StoredOrigins {
            origins: additional_origins,
            unreadable: additional_origins_unreadable,
        } = read_setting(repo, &settings::ADDITIONAL_ORIGINS).await?;
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
        write_setting(repo, &settings::ENABLED, &enabled).await
    }

    pub async fn save_bind_address(
        repo: &dyn SettingsRepo,
        addr: &str,
    ) -> Result<(), StorageError> {
        if !VALID_BIND_ADDRESSES.contains(&addr) {
            return Err(StorageError::ValidationFailed {
                field: settings::BIND_ADDRESS.name().to_owned(),
                reason: format!("must be one of: {}", VALID_BIND_ADDRESSES.join(", ")),
            });
        }
        write_setting(repo, &settings::BIND_ADDRESS, &addr.to_owned()).await
    }

    pub async fn save_port(repo: &dyn SettingsRepo, port: u16) -> Result<(), StorageError> {
        write_setting(repo, &settings::PORT, &u32::from(port)).await
    }

    pub async fn save_lan_bind_enabled(
        repo: &dyn SettingsRepo,
        enabled: bool,
    ) -> Result<(), StorageError> {
        write_setting(repo, &settings::LAN_BIND_ENABLED, &enabled).await
    }

    pub async fn save_auth_required_for_reads(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        write_setting(repo, &settings::AUTH_REQUIRED_FOR_READS, &required).await
    }

    pub async fn save_http_overlay_require_token(
        repo: &dyn SettingsRepo,
        required: bool,
    ) -> Result<(), StorageError> {
        write_setting(repo, &settings::HTTP_OVERLAY_REQUIRE_TOKEN, &required).await
    }

    pub async fn save_overlay_cors_any_origin(
        repo: &dyn SettingsRepo,
        allow: bool,
    ) -> Result<(), StorageError> {
        write_setting(repo, &settings::OVERLAY_CORS_ANY_ORIGIN, &allow).await
    }

    pub async fn save_overlay_root(
        repo: &dyn SettingsRepo,
        path: &str,
    ) -> Result<(), StorageError> {
        write_setting(
            repo,
            &core_settings::SERVER_OVERLAY_ROOT,
            &Some(path.to_owned()),
        )
        .await
    }

    pub async fn save_additional_origins(
        repo: &dyn SettingsRepo,
        origins: &[String],
    ) -> Result<(), StorageError> {
        let stored = StoredOrigins {
            origins: origins.to_vec(),
            unreadable: false,
        };
        write_setting(repo, &settings::ADDITIONAL_ORIGINS, &stored).await
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::StorageError;
    use forge_storage::test_support::MemorySettings;

    use super::*;

    #[tokio::test]
    async fn defaults_when_no_keys_stored() {
        let repo = MemorySettings::new();
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
        let repo = MemorySettings::new();
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
            let repo = MemorySettings::new();
            ServerSettings::save_port(&repo, 9000).await.unwrap();
            if let Some(raw) = stored {
                repo.seed_raw(&settings::ADDITIONAL_ORIGINS, raw);
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
    async fn load_fails_when_any_stored_setting_cannot_be_read_from_the_store() {
        for key in [
            settings::ENABLED.name(),
            settings::BIND_ADDRESS.name(),
            settings::PORT.name(),
            settings::LAN_BIND_ENABLED.name(),
            settings::AUTH_REQUIRED_FOR_READS.name(),
            settings::HTTP_OVERLAY_REQUIRE_TOKEN.name(),
            settings::OVERLAY_CORS_ANY_ORIGIN.name(),
            core_settings::SERVER_OVERLAY_ROOT.name(),
            settings::ADDITIONAL_ORIGINS.name(),
        ] {
            let repo = MemorySettings::new();
            repo.fail_reads_for(key);

            let loaded = ServerSettings::load(&repo).await;

            assert!(
                matches!(loaded, Err(StorageError::Connection { .. })),
                "{key}"
            );
        }
    }

    #[tokio::test]
    async fn load_clamps_a_stored_port_above_the_port_range_to_the_highest_port() {
        for (stored, expected) in [
            ("65535", u16::MAX),
            ("65536", u16::MAX),
            ("70000", u16::MAX),
        ] {
            let repo = MemorySettings::new();
            repo.seed_raw(&settings::PORT, stored);

            let s = ServerSettings::load(&repo).await.unwrap();

            assert_eq!(s.port, expected, "{stored:?}");
        }
    }

    #[tokio::test]
    async fn load_keeps_a_flag_default_when_its_stored_text_is_not_a_flag() {
        let repo = MemorySettings::new();
        repo.seed_raw(&settings::HTTP_OVERLAY_REQUIRE_TOKEN, "sometimes");

        let s = ServerSettings::load(&repo).await.unwrap();

        assert!(s.http_overlay_require_token);
    }

    #[tokio::test]
    async fn save_bind_address_rejects_non_allowlisted() {
        let repo = MemorySettings::new();
        let err = ServerSettings::save_bind_address(&repo, "192.168.1.100")
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::ValidationFailed { .. }));
    }

    #[tokio::test]
    async fn save_bind_address_accepts_all_valid_values() {
        let repo = MemorySettings::new();
        for addr in ["127.0.0.1", "0.0.0.0", "::1", "::"] {
            ServerSettings::save_bind_address(&repo, addr)
                .await
                .unwrap();
            let s = ServerSettings::load(&repo).await.unwrap();
            assert_eq!(s.bind_address, addr);
        }
    }
}
