use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::paths;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, watch};

use crate::auth::AuthState;
use crate::config::ServerSettings;
use crate::server::AppState;
use crate::{ServerError, server};

struct HandleInner {
    join: Option<tokio::task::JoinHandle<Result<(), ServerError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    state: AppState,
    bind_addr: SocketAddr,
}

#[derive(Clone)]
pub struct ServerHandle {
    inner: Arc<Mutex<HandleInner>>,
}

impl ServerHandle {
    pub(crate) fn new(
        join: tokio::task::JoinHandle<Result<(), ServerError>>,
        shutdown_tx: watch::Sender<bool>,
        state: AppState,
        bind_addr: SocketAddr,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HandleInner {
                join: Some(join),
                shutdown_tx: Some(shutdown_tx),
                state,
                bind_addr,
            })),
        }
    }

    pub async fn stop(&self) -> Result<(), ServerError> {
        let (shutdown_tx, join, bus_adapter) = {
            let mut guard = self.inner.lock().await;

            if guard.shutdown_tx.is_none() {
                return Ok(());
            }

            (
                guard.shutdown_tx.take(),
                guard.join.take(),
                Arc::clone(&guard.state.bus_adapter),
            )
        };

        if let Some(tx) = shutdown_tx {
            let _ = tx.send(true);
        }

        bus_adapter.broadcast_close().await;

        let drain = async {
            if let Some(join) = join {
                let _ = join.await;
            }
        };

        let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;

        Ok(())
    }

    pub async fn restart(&self) -> Result<(), ServerError> {
        let state = {
            let guard = self.inner.lock().await;
            guard.state.clone()
        };

        // Reloads persisted settings for the new bind; shared handles (auth, bus, adapters, repos, engine) are preserved.
        let settings = ServerSettings::load(state.settings.as_ref())
            .await
            .map_err(|e| ServerError::Storage(e.to_string()))?;
        let ip: IpAddr = settings
            .bind_address
            .parse()
            .map_err(|_| ServerError::Bind {
                addr: settings.bind_address.clone(),
                reason: "invalid bind address".to_owned(),
            })?;
        let bind_addr = SocketAddr::new(ip, settings.port);
        let overlay_root = settings
            .overlay_root
            .filter(|root| !root.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| paths::data_dir().join("overlays"));

        // Validated before teardown, so a bad config leaves the current server untouched.
        server::validate_lan_bind(
            &bind_addr,
            settings.lan_bind_enabled,
            state.credentials.as_ref(),
        )
        .await?;

        self.stop().await?;

        let new_state = AppState {
            auth: Arc::clone(&state.auth),
            bus: Arc::clone(&state.bus),
            bus_adapter: Arc::clone(&state.bus_adapter),
            actions: Arc::clone(&state.actions),
            globals: Arc::clone(&state.globals),
            user_globals: Arc::clone(&state.user_globals),
            credentials: Arc::clone(&state.credentials),
            settings: Arc::clone(&state.settings),
            server_info: Arc::clone(&state.server_info),
            action_engine: Arc::clone(&state.action_engine),
            overlay_root: Arc::new(overlay_root),
            http_overlay_require_token: settings.http_overlay_require_token,
            overlay_cors_any_origin: settings.overlay_cors_any_origin,
            bind_addr,
        };

        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: bind_addr.to_string(),
                reason: e.to_string(),
            })?;

        let (join, shutdown_tx) = server::serve_on_with_shutdown(listener, new_state.clone());

        let mut guard = self.inner.lock().await;
        guard.join = Some(join);
        guard.shutdown_tx = Some(shutdown_tx);
        guard.state = new_state;
        guard.bind_addr = bind_addr;

        Ok(())
    }

    pub async fn auth_state(&self) -> Arc<AuthState> {
        Arc::clone(&self.inner.lock().await.state.auth)
    }

    pub async fn bind_addr(&self) -> SocketAddr {
        self.inner.lock().await.bind_addr
    }

    pub async fn overlay_root(&self) -> Arc<std::path::PathBuf> {
        Arc::clone(&self.inner.lock().await.state.overlay_root)
    }

    pub async fn snapshot(&self) -> crate::snapshot::ServerSnapshot {
        let state = self.inner.lock().await.state.clone();
        crate::snapshot::build_server_snapshot(&state.server_info, &state.bus_adapter).await
    }

    pub fn abort(&self) {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut guard = inner.lock().await;
            if let Some(tx) = guard.shutdown_tx.take() {
                let _ = tx.send(true);
            }
            if let Some(join) = guard.join.take() {
                join.abort();
            }
        });
    }

    pub async fn await_shutdown(self) -> Result<(), ServerError> {
        let mut guard = self.inner.lock().await;
        let join = match guard.join.take() {
            Some(j) => j,
            None => return Ok(()),
        };
        drop(guard);
        match join.await {
            Ok(result) => result,
            Err(join_err) if join_err.is_cancelled() => Ok(()),
            Err(join_err) => Err(ServerError::Io(std::io::Error::other(join_err))),
        }
    }
}
