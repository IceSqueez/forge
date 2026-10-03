use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Json, Response};
use forge_platform_core::EndpointSurface;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::config::{FakeMonobankConfig, FakeTransaction};
use crate::EmulatorError;

pub const UNKNOWN_TOKEN_MESSAGE: &str = "Unknown 'X-Token'";
pub const TOO_MANY_REQUESTS_MESSAGE: &str = "Too many requests";
pub const MAX_STATEMENT_ITEMS: usize = 500;
pub const MAX_STATEMENT_WINDOW_SECS: i64 = 31 * 24 * 3_600 + 3_600;

const TOKEN_HEADER: &str = "x-token";
const CLIENT_INFO_PATH: &str = "/personal/client-info";
const STATEMENT_PREFIX: &str = "/personal/statement/";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonobankEndpoint {
    ClientInfo,
    Statement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedMonobankRequest {
    pub path: String,
    pub endpoint: Option<MonobankEndpoint>,
    pub token_accepted: bool,
    pub status: u16,
}

struct Inner {
    config: FakeMonobankConfig,
    redirect_to: Option<String>,
    last_client_info: Option<Instant>,
    last_statement: Option<Instant>,
    requests: Vec<RecordedMonobankRequest>,
}

struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<usize>,
}

impl Shared {
    fn mutate<T>(&self, change: impl FnOnce(&mut Inner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let result = change(&mut inner);
        let recorded = inner.requests.len();
        drop(inner);
        self.changes.send_replace(recorded);
        result
    }

    fn read<T>(&self, look: impl FnOnce(&Inner) -> T) -> T {
        look(&self.inner.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

pub struct FakeMonobank {
    shared: Arc<Shared>,
    base_url: String,
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl FakeMonobank {
    pub async fn start(config: FakeMonobankConfig) -> Result<Self, EmulatorError> {
        if config.token.trim().is_empty() {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: "the fake monobank token is blank".to_owned(),
            });
        }
        if let Some((jar, _)) = config
            .transactions
            .iter()
            .find(|(jar, _)| !config.jars.iter().any(|known| &known.id == jar))
        {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: format!("transaction for unknown jar `{jar}`"),
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| EmulatorError::FakeBind {
                reason: e.to_string(),
            })?;
        let address: SocketAddr = listener.local_addr().map_err(|e| EmulatorError::FakeBind {
            reason: e.to_string(),
        })?;
        let (changes, _) = watch::channel(0);
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                config,
                redirect_to: None,
                last_client_info: None,
                last_statement: None,
                requests: Vec::new(),
            }),
            changes,
        });
        let (shutdown, mut shutdown_rx) = watch::channel(false);
        let router = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.changed().await;
                })
                .await;
        });
        Ok(Self {
            shared,
            base_url: format!("http://{address}"),
            shutdown,
            task: Some(task),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn endpoint_override(&self) -> (&'static str, String) {
        (
            EndpointSurface::MonobankApi.env_var(),
            self.base_url.clone(),
        )
    }

    pub fn top_up(&self, jar: &str, transaction: FakeTransaction) {
        self.shared.mutate(|inner| {
            inner
                .config
                .transactions
                .push((jar.to_owned(), transaction));
        });
    }

    pub fn redirect_all_to(&self, origin: Option<&str>) {
        self.shared
            .mutate(|inner| inner.redirect_to = origin.map(str::to_owned));
    }

    pub fn requests(&self) -> Vec<RecordedMonobankRequest> {
        self.shared.read(|inner| inner.requests.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&[RecordedMonobankRequest]) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        let mut changes = self.shared.changes.subscribe();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(found) = self.shared.read(|inner| probe(&inner.requests)) {
                return Ok(found);
            }
            match tokio::time::timeout_at(deadline, changes.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    return Err(EmulatorError::WaitTimeout {
                        what: what.to_owned(),
                    });
                }
            }
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(mut task) = self.task.take()
            && tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
                .await
                .is_err()
        {
            task.abort();
        }
    }
}

impl Drop for FakeMonobank {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

enum Answer {
    Json(StatusCode, Value),
    Redirect(String),
}

impl Answer {
    fn status(&self) -> StatusCode {
        match self {
            Self::Json(status, _) => *status,
            Self::Redirect(_) => StatusCode::FOUND,
        }
    }

    fn refusal(status: StatusCode, message: &str) -> Self {
        Self::Json(status, json!({ "errorDescription": message }))
    }
}

async fn handle(State(shared): State<Arc<Shared>>, uri: Uri, headers: HeaderMap) -> Response {
    let presented = headers
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let path = uri.path().to_owned();
    let endpoint = endpoint_of(&path);
    let answer = shared.mutate(|inner| {
        let token_accepted = presented.as_deref() == Some(inner.config.token.as_str());
        let answer = answer(inner, &path, endpoint, token_accepted);
        inner.requests.push(RecordedMonobankRequest {
            path: path.clone(),
            endpoint,
            token_accepted,
            status: answer.status().as_u16(),
        });
        answer
    });
    match answer {
        Answer::Json(status, body) => (status, Json(body)).into_response(),
        Answer::Redirect(location) => {
            (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
        }
    }
}

fn endpoint_of(path: &str) -> Option<MonobankEndpoint> {
    if path == CLIENT_INFO_PATH {
        Some(MonobankEndpoint::ClientInfo)
    } else if path.starts_with(STATEMENT_PREFIX) {
        Some(MonobankEndpoint::Statement)
    } else {
        None
    }
}

fn answer(
    inner: &mut Inner,
    path: &str,
    endpoint: Option<MonobankEndpoint>,
    token_accepted: bool,
) -> Answer {
    if let Some(origin) = &inner.redirect_to {
        return Answer::Redirect(format!("{origin}{path}"));
    }
    let Some(endpoint) = endpoint else {
        return Answer::refusal(StatusCode::NOT_FOUND, "Unknown method");
    };
    if !token_accepted {
        return Answer::refusal(StatusCode::FORBIDDEN, UNKNOWN_TOKEN_MESSAGE);
    }
    let window = inner.config.call_window;
    let last = match endpoint {
        MonobankEndpoint::ClientInfo => &mut inner.last_client_info,
        MonobankEndpoint::Statement => &mut inner.last_statement,
    };
    let now = Instant::now();
    if last.is_some_and(|at| now.saturating_duration_since(at) < window) {
        return Answer::refusal(StatusCode::TOO_MANY_REQUESTS, TOO_MANY_REQUESTS_MESSAGE);
    }
    *last = Some(now);
    match endpoint {
        MonobankEndpoint::ClientInfo => client_info(&inner.config),
        MonobankEndpoint::Statement => statement(&inner.config, path),
    }
}

fn client_info(config: &FakeMonobankConfig) -> Answer {
    Answer::Json(
        StatusCode::OK,
        json!({
            "clientId": "fake-client",
            "name": config.client_name,
            "webHookUrl": "",
            "permissions": "psfj",
            "accounts": [],
            "jars": config.jars.iter().map(super::config::FakeJar::wire).collect::<Vec<_>>(),
        }),
    )
}

fn statement(config: &FakeMonobankConfig, path: &str) -> Answer {
    let parts: Vec<&str> = path
        .trim_start_matches(STATEMENT_PREFIX)
        .split('/')
        .collect();
    let [account, from, to] = parts.as_slice() else {
        return Answer::refusal(StatusCode::BAD_REQUEST, "Invalid statement path");
    };
    let (Ok(from), Ok(to)) = (from.parse::<i64>(), to.parse::<i64>()) else {
        return Answer::refusal(StatusCode::BAD_REQUEST, "Invalid time range");
    };
    if to < from || to - from > MAX_STATEMENT_WINDOW_SECS {
        return Answer::refusal(
            StatusCode::BAD_REQUEST,
            "Period must be no more than 31 days",
        );
    }
    if !config.jars.iter().any(|jar| jar.id == *account) {
        return Answer::refusal(StatusCode::BAD_REQUEST, "Invalid account");
    }
    let mut listed: Vec<&FakeTransaction> = config
        .transactions
        .iter()
        .filter(|(jar, item)| jar == account && (from..=to).contains(&item.time))
        .map(|(_, item)| item)
        .collect();
    listed.sort_by_key(|item| std::cmp::Reverse(item.time));
    Answer::Json(
        StatusCode::OK,
        Value::Array(
            listed
                .into_iter()
                .take(MAX_STATEMENT_ITEMS)
                .map(FakeTransaction::wire)
                .collect(),
        ),
    )
}
