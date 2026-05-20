use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::{Mutex, watch};

use crate::auth::AuthState;
use crate::server::AppState;
use crate::{ServerError, server};

struct HandleInner {
    join: Option<tokio::task::JoinHandle<Result<(), ServerError>>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    state: AppState,
    bind_addr: SocketAddr,
}

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
        let mut guard = self.inner.lock().await;

        if guard.shutdown_tx.is_none() {
            return Ok(());
        }

        if let Some(tx) = guard.shutdown_tx.take() {
            let _ = tx.send(true);
        }

        guard.state.bus_adapter.broadcast_close().await;

        let drain = async {
            if let Some(join) = guard.join.take() {
                let _ = join.await;
            }
        };

        let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;

        guard.join = None;

        Ok(())
    }

    pub async fn restart(&self) -> Result<(), ServerError> {
        let (state, bind_addr) = {
            let guard = self.inner.lock().await;
            (guard.state.clone(), guard.bind_addr)
        };

        self.stop().await?;

        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| ServerError::Bind {
                addr: bind_addr.to_string(),
                reason: e.to_string(),
            })?;

        let (join, shutdown_tx) = server::serve_on_with_shutdown(listener, state.clone());

        let mut guard = self.inner.lock().await;
        guard.join = Some(join);
        guard.shutdown_tx = Some(shutdown_tx);
        guard.state = state;
        guard.bind_addr = bind_addr;

        Ok(())
    }

    pub async fn auth_state(&self) -> Arc<AuthState> {
        Arc::clone(&self.inner.lock().await.state.auth)
    }

    pub async fn bind_addr(&self) -> SocketAddr {
        self.inner.lock().await.bind_addr
    }

    pub async fn server_info(&self) -> Arc<crate::server_info::ServerInfo> {
        Arc::clone(&self.inner.lock().await.state.server_info)
    }

    pub async fn bus_adapter(&self) -> Arc<crate::bus_adapter::BusAdapter> {
        Arc::clone(&self.inner.lock().await.state.bus_adapter)
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
