use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, header};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::EmulatorError;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const BEARER_PREFIX: &str = "Bearer ";

pub(crate) struct Observed<T> {
    state: Mutex<T>,
    revision: watch::Sender<u64>,
}

impl<T> Observed<T> {
    pub(crate) fn new(state: T) -> Self {
        Self {
            state: Mutex::new(state),
            revision: watch::Sender::new(0),
        }
    }

    fn lock(&self) -> MutexGuard<'_, T> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn read<R>(&self, look: impl FnOnce(&T) -> R) -> R {
        look(&self.lock())
    }

    pub(crate) fn mutate<R>(&self, change: impl FnOnce(&mut T) -> R) -> R {
        let result = change(&mut self.lock());
        self.revision.send_modify(|revision| *revision += 1);
        result
    }

    pub(crate) async fn wait_for<R>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&T) -> Option<R>,
    ) -> Result<R, EmulatorError> {
        let mut revisions = self.revision.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(found) = self.read(&mut probe) {
                return Ok(found);
            }
            match tokio::time::timeout_at(deadline, revisions.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    return Err(EmulatorError::WaitTimeout {
                        what: what.to_owned(),
                    });
                }
            }
        }
    }
}

pub(crate) struct ServerTasks {
    stop: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl ServerTasks {
    pub(crate) fn new(stop: watch::Sender<bool>, tasks: Vec<JoinHandle<()>>) -> Self {
        Self { stop, tasks }
    }

    pub(crate) async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        for mut task in std::mem::take(&mut self.tasks) {
            if tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
    }
}

impl Drop for ServerTasks {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub(crate) async fn bind_loopback() -> Result<TcpListener, EmulatorError> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| EmulatorError::FakeBind {
            reason: e.to_string(),
        })
}

pub(crate) fn local_addr(listener: &TcpListener) -> Result<SocketAddr, EmulatorError> {
    listener.local_addr().map_err(|e| EmulatorError::FakeBind {
        reason: e.to_string(),
    })
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
}

pub(crate) fn serve_http(
    listener: TcpListener,
    router: Router,
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let served = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = stop.changed().await;
            })
            .await;
        if let Err(error) = served {
            eprintln!("forge-emulator: a fake http server stopped: {error}");
        }
    })
}
