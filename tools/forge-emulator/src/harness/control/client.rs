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
    abandoned: usize,
}

struct PendingRequest {
    id: String,
    reply: oneshot::Sender<ResponseOutcome>,
}

const UNATTRIBUTABLE_RESPONSE: &str = "a response without an id cannot be attributed to a request";

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
        let writer = tokio::spawn(write_frames(sink, outbound_rx, Arc::clone(&ledger)));
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
                reply,
            });
        }
        match tokio::time::timeout(self.request_timeout, response).await {
            Ok(Ok(outcome)) => Ok(outcome),
            Ok(Err(_)) => Err(EmulatorError::ConnectionClosed),
            Err(_) => {
                let mut ledger = lock(&self.ledger);
                if let Some(index) = ledger.pending.iter().position(|entry| entry.id == id) {
                    ledger.pending.remove(index);
                    ledger.abandoned += 1;
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

async fn write_frames<S>(
    mut sink: S,
    mut outbound: mpsc::UnboundedReceiver<Message>,
    ledger: Arc<Mutex<Ledger>>,
) where
    S: Sink<Message> + Unpin,
{
    while let Some(message) = outbound.recv().await {
        if sink.send(message).await.is_err() {
            break;
        }
    }
    close(&ledger);
}

async fn read_frames<S>(
    mut stream: S,
    ledger: Arc<Mutex<Ledger>>,
    observations: mpsc::UnboundedSender<Observation>,
) where
    S: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => break,
            Message::Binary(bytes) => {
                let _ = observations.send(Observation::Undecodable {
                    frame: format!("<{} binary bytes>", bytes.len()),
                    reason: "binary frame".to_owned(),
                });
                continue;
            }
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        };
        let observation = match classify(text.as_str()) {
            InboundFrame::Response { id, outcome } => match route_response(&ledger, id, outcome) {
                Routing::Settled => continue,
                Routing::Unattributable => Observation::Undecodable {
                    frame: text.as_str().to_owned(),
                    reason: UNATTRIBUTABLE_RESPONSE.to_owned(),
                },
            },
            InboundFrame::Observation(observation) => observation,
        };
        let _ = observations.send(observation);
    }
    close(&ledger);
}

fn close(ledger: &Mutex<Ledger>) {
    let mut ledger = lock(ledger);
    ledger.closed = true;
    ledger.pending.clear();
}

enum Routing {
    Settled,
    Unattributable,
}

fn route_response(ledger: &Mutex<Ledger>, id: Option<String>, outcome: ResponseOutcome) -> Routing {
    let mut ledger = lock(ledger);
    let entry = match id {
        Some(id) => {
            let position = ledger.pending.iter().position(|entry| entry.id == id);
            match position {
                Some(index) => ledger.pending.remove(index),
                None => {
                    ledger.abandoned = ledger.abandoned.saturating_sub(1);
                    None
                }
            }
        }
        None if ledger.abandoned == 0 && ledger.pending.len() == 1 => ledger.pending.pop_front(),
        None => {
            ledger.abandoned = ledger.abandoned.saturating_sub(1);
            return Routing::Unattributable;
        }
    };
    if let Some(entry) = entry {
        let _ = entry.reply.send(outcome);
    }
    Routing::Settled
}

#[cfg(test)]
mod tests {
    use futures_util::sink;

    use super::*;

    #[tokio::test]
    async fn a_failed_write_closes_the_ledger_and_releases_waiting_requests() {
        let ledger = Arc::new(Mutex::new(Ledger::default()));
        let (reply, response) = oneshot::channel();
        lock(&ledger).pending.push_back(PendingRequest {
            id: "emu-1".to_owned(),
            reply,
        });
        let (outbound, outbound_rx) = mpsc::unbounded_channel();
        let failing = Box::pin(sink::unfold((), |(), _: Message| async {
            Err::<(), ()>(())
        }));

        outbound.send(Message::Text("frame".into())).unwrap();
        write_frames(failing, outbound_rx, Arc::clone(&ledger)).await;

        assert!(lock(&ledger).closed);
        assert!(response.await.is_err());
    }
}
