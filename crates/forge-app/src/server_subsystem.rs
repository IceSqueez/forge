use std::sync::Arc;

use tokio::sync::RwLock;

use forge_runtime::{ActionEngineHandle, EventBus};
use forge_server::{ServerConfig, ServerError, ServerHandle, ServerSettings, start_server};
use forge_storage::{
    CredentialId, CredentialsRepo, DataProvider, GlobalsRepo, SettingsRepo, UserGlobalsRepo,
    reserved_keys::SERVER_PORT_KEY,
};

const BEARER_CREDENTIAL_ID: &str = "server:bearer";

#[derive(Debug, Clone)]
pub struct ServerBootSnapshot {
    pub bind_address: String,
    pub bearer_token: String,
}

pub struct ServerSubsystem {
    handle: Arc<RwLock<Option<ServerHandle>>>,
    credentials: Arc<dyn CredentialsRepo>,
}

impl ServerSubsystem {
    pub fn new(credentials: Arc<dyn CredentialsRepo>) -> Self {
        Self {
            handle: Arc::new(RwLock::new(None)),
            credentials,
        }
    }

    pub async fn start(&self, config: ServerConfig) -> Result<(), ServerError> {
        if self.handle.read().await.is_some() {
            return Ok(());
        }
        // Bind the server without holding the handle lock so concurrent readers
        // (e.g. the metrics subscription) are not blocked for the bind duration.
        let handle = start_server(config).await?;
        let mut guard = self.handle.write().await;
        if guard.is_some() {
            // Lost a concurrent start race; discard the redundant handle.
            drop(guard);
            handle.stop().await?;
            return Ok(());
        }
        *guard = Some(handle);
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), ServerError> {
        let handle = {
            let mut guard = self.handle.write().await;
            guard.take()
        };
        if let Some(handle) = handle {
            handle.stop().await?;
        }
        Ok(())
    }

    pub async fn restart(&self) -> Result<(), ServerError> {
        let handle = { self.handle.read().await.as_ref().cloned() };
        match handle {
            Some(h) => h.restart().await,
            None => Err(ServerError::AuthInvalid {
                reason: "server is not running".to_owned(),
            }),
        }
    }

    pub async fn regenerate_token(&self) -> Result<String, ServerError> {
        let auth = {
            let guard = self.handle.read().await;
            match guard.as_ref() {
                Some(handle) => handle.auth_state().await,
                None => {
                    return Err(ServerError::AuthInvalid {
                        reason: "server is not running".to_owned(),
                    });
                }
            }
        };
        auth.regenerate(self.credentials.as_ref()).await
    }

    pub async fn is_running(&self) -> bool {
        self.handle.read().await.is_some()
    }

    pub async fn server_info(&self) -> Option<Arc<forge_server::ServerInfo>> {
        let handle = { self.handle.read().await.as_ref().cloned() };
        match handle {
            Some(h) => Some(h.server_info().await),
            None => None,
        }
    }

    pub async fn bus_adapter(&self) -> Option<Arc<forge_server::BusAdapter>> {
        let handle = { self.handle.read().await.as_ref().cloned() };
        match handle {
            Some(h) => Some(h.bus_adapter().await),
            None => None,
        }
    }

    pub async fn overlay_root(&self) -> Option<Arc<std::path::PathBuf>> {
        let handle = { self.handle.read().await.as_ref().cloned() };
        match handle {
            Some(h) => Some(h.overlay_root().await),
            None => None,
        }
    }

    pub async fn bearer_token(&self) -> Result<Option<String>, ServerError> {
        let id = CredentialId::new(BEARER_CREDENTIAL_ID);
        self.credentials
            .load(&id)
            .await
            .map_err(|e| ServerError::Storage(e.to_string()))
    }
}

pub async fn load_server_settings_and_start(
    backend: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
    action_engine: Arc<ActionEngineHandle>,
    subsystem: Arc<ServerSubsystem>,
) -> Result<ServerBootSnapshot, String> {
    let settings: &dyn SettingsRepo = backend.as_ref();
    let snap = ServerSettings::load(settings)
        .await
        .map_err(|e| e.to_string())?;

    let ip: std::net::IpAddr = snap
        .bind_address
        .parse()
        .map_err(|e: std::net::AddrParseError| format!("invalid {SERVER_PORT_KEY}: {e}"))?;
    let bind_addr = std::net::SocketAddr::new(ip, snap.port);

    let actions = backend.action_repo();
    let globals: Arc<dyn GlobalsRepo> = Arc::clone(&backend) as Arc<dyn GlobalsRepo>;
    let user_globals: Arc<dyn UserGlobalsRepo> = Arc::clone(&backend) as Arc<dyn UserGlobalsRepo>;
    let mut config = ServerConfig::new(
        Arc::clone(&subsystem.credentials),
        bus,
        actions,
        globals,
        user_globals,
        action_engine,
    );
    config.bind_addr = bind_addr;
    config.auth_required_for_reads = snap.auth_required_for_reads;
    config.lan_bind_enabled = snap.lan_bind_enabled;
    config.http_overlay_require_token = snap.http_overlay_require_token;
    config.overlay_cors_any_origin = snap.overlay_cors_any_origin;
    if let Some(root) = snap.overlay_root
        && !root.is_empty()
    {
        config.overlay_root = std::path::PathBuf::from(root);
    }

    subsystem.start(config).await.map_err(|e| e.to_string())?;

    let token = subsystem
        .bearer_token()
        .await
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    Ok(ServerBootSnapshot {
        bind_address: bind_addr.to_string(),
        bearer_token: token,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_storage::StorageError;
    use time::OffsetDateTime;

    use super::*;

    struct MemCreds(Mutex<HashMap<String, String>>);

    impl MemCreds {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }
    }

    #[async_trait]
    impl CredentialsRepo for MemCreds {
        async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), bundle.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
        }

        async fn last_refresh(
            &self,
            _id: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn new_subsystem_is_not_running() {
        let creds: Arc<dyn CredentialsRepo> = MemCreds::new();
        let sub = ServerSubsystem::new(creds);
        assert!(!sub.is_running().await);
    }

    #[tokio::test]
    async fn restart_when_not_running_returns_error() {
        let creds: Arc<dyn CredentialsRepo> = MemCreds::new();
        let sub = ServerSubsystem::new(creds);
        let err = sub.restart().await.expect_err("must error");
        assert!(matches!(err, ServerError::AuthInvalid { .. }));
    }

    #[tokio::test]
    async fn regenerate_token_when_not_running_returns_error() {
        let creds: Arc<dyn CredentialsRepo> = MemCreds::new();
        let sub = ServerSubsystem::new(creds);
        let err = sub.regenerate_token().await.expect_err("must error");
        assert!(matches!(err, ServerError::AuthInvalid { .. }));
    }

    #[tokio::test]
    async fn bearer_token_when_credential_missing_returns_none() {
        let creds: Arc<dyn CredentialsRepo> = MemCreds::new();
        let sub = ServerSubsystem::new(creds);
        assert_eq!(sub.bearer_token().await.unwrap(), None);
    }

    #[tokio::test]
    async fn bearer_token_returns_stored_value() {
        let creds_mem = MemCreds::new();
        creds_mem
            .store(&CredentialId::new(BEARER_CREDENTIAL_ID), "tok-abc")
            .await
            .unwrap();
        let creds: Arc<dyn CredentialsRepo> = creds_mem;
        let sub = ServerSubsystem::new(creds);
        assert_eq!(
            sub.bearer_token().await.unwrap().as_deref(),
            Some("tok-abc")
        );
    }
}
