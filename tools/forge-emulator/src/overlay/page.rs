use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::ORIGIN;
use tokio_tungstenite::tungstenite::{self, Message};

use super::endpoint::PageEndpoint;
use crate::EmulatorError;

/// The wire values `crates/forge-overlay/assets/shared/runtime-v1.js` uses, so a page opened here
/// is indistinguishable from a browser source to forge.
const AUTH_REQUEST_ID: &str = "1";
const AUTH_REQUEST: &str = "auth";
const CREDENTIAL_FIELD: &str = "overlayCredential";
const CREDENTIAL_KEY: &str = "credential";
const ID_KEY: &str = "id";
const STATUS_KEY: &str = "status";
const ERROR_STATUS: &str = "error";
const ERROR_MESSAGE_POINTER: &str = "/error/message";

#[derive(Debug, Clone)]
pub struct ReceivedFrame {
    pub arrived: Instant,
    pub raw: Value,
}

/// A browser source for one overlay: it fetches `config.json`, presents the credential that
/// document carries, and from then on only records what forge sends it.
pub struct OverlayPage {
    identity: String,
    shared: Arc<Shared>,
    reader: JoinHandle<()>,
}

#[derive(Default)]
struct Shared {
    frames: Mutex<Vec<ReceivedFrame>>,
    changes: watch::Sender<u64>,
}

impl OverlayPage {
    /// Returns only once forge has accepted the credential, so a caller that opens a page and then
    /// looks for content cannot race the handshake.
    pub async fn open(
        endpoint: &PageEndpoint,
        identity: &str,
        timeout: Duration,
    ) -> Result<Self, EmulatorError> {
        let deadline = Instant::now() + timeout;
        let credential = fetch_credential(endpoint, identity, timeout).await?;
        let request = socket_request(endpoint)?;
        let (mut socket, _) =
            tokio::time::timeout_at(deadline, tokio_tungstenite::connect_async(request))
                .await
                .map_err(|_| EmulatorError::ConnectTimeout)?
                .map_err(|e| EmulatorError::Connect {
                    reason: e.to_string(),
                })?;

        let shared = Arc::new(Shared::default());
        let auth = serde_json::json!({
            ID_KEY: AUTH_REQUEST_ID,
            "request": AUTH_REQUEST,
            CREDENTIAL_FIELD: credential,
        })
        .to_string();
        socket
            .send(Message::Text(auth.into()))
            .await
            .map_err(|e| EmulatorError::Connect {
                reason: e.to_string(),
            })?;
        await_acceptance(&mut socket, &shared, identity, deadline).await?;

        let reader = tokio::spawn(read_frames(socket, Arc::clone(&shared)));
        Ok(Self {
            identity: identity.to_owned(),
            shared,
            reader,
        })
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// The index the next recorded frame will take.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Hands over every frame recorded so far and forgets them, so a page left open under
    /// sustained load does not keep its whole history. `len` restarts from zero afterwards.
    pub fn take_frames(&self) -> Vec<ReceivedFrame> {
        std::mem::take(&mut *self.lock())
    }

    pub fn read<R>(&self, f: impl FnOnce(&[ReceivedFrame]) -> R) -> R {
        f(&self.lock())
    }

    /// Returns once `settled` holds or `deadline` passes, whichever is first.
    pub async fn wait_until(
        &self,
        deadline: Instant,
        mut settled: impl FnMut(&[ReceivedFrame]) -> bool,
    ) {
        let mut changes = self.shared.changes.subscribe();
        loop {
            if self.read(&mut settled) || Instant::now() >= deadline {
                return;
            }
            if !matches!(
                tokio::time::timeout_at(deadline, changes.changed()).await,
                Ok(Ok(()))
            ) {
                return;
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<ReceivedFrame>> {
        self.shared
            .frames
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for OverlayPage {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn fetch_credential(
    endpoint: &PageEndpoint,
    identity: &str,
    timeout: Duration,
) -> Result<String, EmulatorError> {
    let unreadable = |reason: String| EmulatorError::OverlayConfigUnreadable {
        identity: identity.to_owned(),
        reason,
    };
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| unreadable(without_url(e)))?;
    let response = client
        .get(endpoint.config_url(identity))
        .header(ORIGIN, endpoint.origin())
        .send()
        .await
        .map_err(|e| unreadable(without_url(e)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(unreadable(format!("answered {status}")));
    }
    let document: Value = response
        .json()
        .await
        .map_err(|e| unreadable(without_url(e)))?;
    document
        .get(CREDENTIAL_KEY)
        .and_then(Value::as_str)
        .filter(|credential| !credential.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| EmulatorError::OverlayCredentialMissing {
            identity: identity.to_owned(),
        })
}

/// Strips the request URL before rendering: a page URL carries the overlay identity, and a
/// rendered reqwest error is the one place a request target reaches a report.
fn without_url(error: reqwest::Error) -> String {
    error.without_url().to_string()
}

fn socket_request(endpoint: &PageEndpoint) -> Result<Request, EmulatorError> {
    let refused = |reason: String| EmulatorError::Connect { reason };
    let mut request = endpoint
        .socket_url()
        .into_client_request()
        .map_err(|e| refused(e.to_string()))?;
    let origin = HeaderValue::from_str(&endpoint.origin()).map_err(|e| refused(e.to_string()))?;
    request.headers_mut().insert(ORIGIN, origin);
    Ok(request)
}

/// Frames arriving before the answer are kept: forge replays retained content from inside the
/// credential check, and no delivery is allowed to go unrecorded.
async fn await_acceptance<S>(
    stream: &mut S,
    shared: &Arc<Shared>,
    identity: &str,
    deadline: Instant,
) -> Result<(), EmulatorError>
where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout_at(deadline, stream.next())
            .await
            .map_err(|_| EmulatorError::ConnectTimeout)?;
        let text = match message {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
            Some(Ok(Message::Binary(_) | Message::Close(_))) | None => {
                return Err(EmulatorError::ConnectionClosed);
            }
            Some(Err(e)) => {
                return Err(EmulatorError::Connect {
                    reason: e.to_string(),
                });
            }
        };
        let Ok(frame) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        if frame.get(ID_KEY).and_then(Value::as_str) != Some(AUTH_REQUEST_ID) {
            record(shared, frame);
            continue;
        }
        if frame.get(STATUS_KEY).and_then(Value::as_str) == Some(ERROR_STATUS) {
            return Err(EmulatorError::OverlayAuthRefused {
                identity: identity.to_owned(),
                message: frame
                    .pointer(ERROR_MESSAGE_POINTER)
                    .and_then(Value::as_str)
                    .unwrap_or("no detail given")
                    .to_owned(),
            });
        }
        return Ok(());
    }
}

async fn read_frames<S>(mut stream: S, shared: Arc<Shared>)
where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Text(text) => {
                let raw = serde_json::from_str::<Value>(text.as_str())
                    .unwrap_or_else(|_| Value::String(text.as_str().to_owned()));
                record(&shared, raw);
            }
            Message::Close(_) => break,
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }
}

/// Stamped under the lock, so a read at any instant sees every frame stamped by then.
fn record(shared: &Arc<Shared>, raw: Value) {
    {
        let mut frames = shared.frames.lock().unwrap_or_else(PoisonError::into_inner);
        frames.push(ReceivedFrame {
            arrived: Instant::now(),
            raw,
        });
    }
    shared.changes.send_modify(|generation| *generation += 1);
}
