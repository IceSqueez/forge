use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use forge_events::Event;
use forge_types::ActionId;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde_json::{Map, Value};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{self, Message};

use super::endpoint::ControlEndpoint;
use super::event::decode_history_entry;
use super::frame::{InboundFrame, Observation, ResponseOutcome, classify};
use super::request::{EventFilter, Request};
use crate::EmulatorError;

#[derive(Debug, Clone, Copy)]
pub struct ClientTimeouts {
    pub connect: Duration,
    pub request: Duration,
}

impl Default for ClientTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            request: Duration::from_secs(5),
        }
    }
}

pub struct ControlClient {
    ledger: Arc<Mutex<Ledger>>,
    outbound: mpsc::UnboundedSender<Message>,
    next_id: AtomicU64,
    request_timeout: Duration,
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

/// Ends once the connection closes or its `ControlClient` is dropped.
pub struct EventStream {
    observations: mpsc::UnboundedReceiver<Observation>,
}

impl EventStream {
    pub async fn next(&mut self) -> Option<Observation> {
        self.observations.recv().await
    }
}

#[derive(Default)]
struct Ledger {
    closed: bool,
    pending: VecDeque<PendingRequest>,
}

struct PendingRequest {
    id: String,
    /// `None` once the caller gave up; the entry stays queued so arrival order still lines up.
    reply: Option<oneshot::Sender<ResponseOutcome>>,
}

impl ControlClient {
    pub async fn connect(
        endpoint: &ControlEndpoint,
        timeouts: ClientTimeouts,
    ) -> Result<(Self, EventStream), EmulatorError> {
        let (socket, _) = tokio::time::timeout(
            timeouts.connect,
            tokio_tungstenite::connect_async(endpoint.url()),
        )
        .await
        .map_err(|_| EmulatorError::ConnectTimeout)?
        .map_err(|e| EmulatorError::Connect {
            reason: e.to_string(),
        })?;
        let (sink, stream) = socket.split();
        let ledger = Arc::new(Mutex::new(Ledger::default()));
        let (outbound, outbound_rx) = mpsc::unbounded_channel();
        let (observations_tx, observations) = mpsc::unbounded_channel();
        let writer = tokio::spawn(write_frames(sink, outbound_rx));
        let reader = tokio::spawn(read_frames(stream, Arc::clone(&ledger), observations_tx));
        let client = Self {
            ledger,
            outbound,
            next_id: AtomicU64::new(1),
            request_timeout: timeouts.request,
            reader,
            writer,
        };
        Ok((client, EventStream { observations }))
    }

    pub async fn authenticate(&self, token: &str) -> Result<(), EmulatorError> {
        match self.send(Request::Auth { token }).await? {
            Ok(_) => Ok(()),
            Err(refusal) => Err(EmulatorError::AuthRefused {
                message: refusal.message,
            }),
        }
    }

    pub async fn subscribe(&self, filters: &[EventFilter]) -> Result<(), EmulatorError> {
        self.call(Request::Subscribe { events: filters })
            .await
            .map(drop)
    }

    pub async fn recent_events(&self, limit: u32) -> Result<Vec<Event>, EmulatorError> {
        let request = Request::GetEvents { limit };
        let method = request.method();
        let unexpected = |reason: String| EmulatorError::UnexpectedResponse {
            request: method,
            reason,
        };
        let mut body = self.call(request).await?;
        let Some(Value::Array(entries)) = body.remove("events") else {
            return Err(unexpected("no `events` array".to_owned()));
        };
        entries
            .into_iter()
            .map(|entry| decode_history_entry(entry).map_err(|e| unexpected(e.to_string())))
            .collect()
    }

    pub async fn forge_version(&self) -> Result<String, EmulatorError> {
        let request = Request::GetInfo;
        let method = request.method();
        let mut body = self.call(request).await?;
        match body.remove("version") {
            Some(Value::String(version)) => Ok(version),
            _ => Err(EmulatorError::UnexpectedResponse {
                request: method,
                reason: "no `version` string".to_owned(),
            }),
        }
    }

    /// Yields the execution id forge publishes as the cause of the run's `action.start`.
    pub async fn do_action(
        &self,
        action: ActionId,
        args: &Map<String, Value>,
    ) -> Result<String, EmulatorError> {
        let request = Request::DoAction {
            action_id: action.to_string(),
            args,
        };
        let method = request.method();
        let mut body = self.call(request).await?;
        match body.remove("execution_id") {
            Some(Value::String(execution_id)) => Ok(execution_id),
            _ => Err(EmulatorError::UnexpectedResponse {
                request: method,
                reason: "no `execution_id` string".to_owned(),
            }),
        }
    }

    pub async fn set_global(
        &self,
        name: &str,
        value: &Value,
        persisted: bool,
    ) -> Result<(), EmulatorError> {
        self.call(Request::SetGlobal {
            name,
            value,
            persisted,
        })
        .await
        .map(drop)
    }

    async fn call(&self, request: Request<'_>) -> Result<Map<String, Value>, EmulatorError> {
        let method = request.method();
        self.send(request)
            .await?
            .map_err(|refusal| EmulatorError::Refused {
                request: method,
                code: refusal.code,
                message: refusal.message,
            })
    }

    async fn send(&self, request: Request<'_>) -> Result<ResponseOutcome, EmulatorError> {
        let method = request.method();
        let id = format!("emu-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (reply, response) = oneshot::channel();
        {
            let mut ledger = lock(&self.ledger);
            if ledger.closed {
                return Err(EmulatorError::ConnectionClosed);
            }
            let frame = Message::Text(request.to_frame(&id).into());
            if self.outbound.send(frame).is_err() {
                return Err(EmulatorError::ConnectionClosed);
            }
            ledger.pending.push_back(PendingRequest {
                id: id.clone(),
                reply: Some(reply),
            });
        }
        match tokio::time::timeout(self.request_timeout, response).await {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(_)) => Err(EmulatorError::ConnectionClosed),
            Err(_) => {
                if let Some(entry) = lock(&self.ledger)
                    .pending
                    .iter_mut()
                    .find(|entry| entry.id == id)
                {
                    entry.reply = None;
                }
                Err(EmulatorError::RequestTimeout { request: method })
            }
        }
    }
}

impl Drop for ControlClient {
    fn drop(&mut self) {
        self.reader.abort();
        self.writer.abort();
    }
}

fn lock(ledger: &Mutex<Ledger>) -> MutexGuard<'_, Ledger> {
    ledger.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn write_frames<S>(mut sink: S, mut outbound: mpsc::UnboundedReceiver<Message>)
where
    S: Sink<Message> + Unpin,
{
    while let Some(message) = outbound.recv().await {
        if sink.send(message).await.is_err() {
            break;
        }
    }
}

async fn read_frames<S>(
    mut stream: S,
    ledger: Arc<Mutex<Ledger>>,
    observations: mpsc::UnboundedSender<Observation>,
) where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    while let Some(Ok(message)) = stream.next().await {
        let inbound = match message {
            Message::Text(text) => classify(text.as_str()),
            Message::Close(_) => break,
            Message::Binary(bytes) => InboundFrame::Observation(Observation::Undecodable {
                frame: format!("<{} binary bytes>", bytes.len()),
                reason: "binary frame".to_owned(),
            }),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        };
        match inbound {
            InboundFrame::Response { id, outcome } => route_response(&ledger, id, outcome),
            InboundFrame::Observation(observation) => {
                let _ = observations.send(observation);
            }
        }
    }
    let mut ledger = lock(&ledger);
    ledger.closed = true;
    ledger.pending.clear();
}

fn route_response(ledger: &Mutex<Ledger>, id: Option<String>, outcome: ResponseOutcome) {
    let mut ledger = lock(ledger);
    let entry = match id {
        Some(id) => ledger
            .pending
            .iter()
            .position(|entry| entry.id == id)
            .and_then(|index| ledger.pending.remove(index)),
        // The server answers frames strictly in arrival order, so an answer it could not
        // correlate belongs to the oldest request still queued.
        None => ledger.pending.pop_front(),
    };
    if let Some(reply) = entry.and_then(|entry| entry.reply) {
        let _ = reply.send(outcome);
    }
}
