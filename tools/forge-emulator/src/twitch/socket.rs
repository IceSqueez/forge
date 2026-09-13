use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{Instant, interval_at};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};

use super::frames;
use super::state::Shared;

const OUTBOX_CAPACITY: usize = 256;
const RECONNECT_QUERY_KEY: &str = "reconnect_from=";

pub(crate) async fn serve(
    listener: TcpListener,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    connections.spawn(connection(stream, Arc::clone(&shared), shutdown.clone()));
                }
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    connections.shutdown().await;
}

async fn connection(stream: TcpStream, shared: Arc<Shared>, mut shutdown: watch::Receiver<bool>) {
    let mut query: Option<String> = None;
    let Ok(mut socket) =
        tokio_tungstenite::accept_hdr_async(stream, QueryCapture(&mut query)).await
    else {
        return;
    };
    let reconnected_from = query.as_deref().and_then(|query| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(RECONNECT_QUERY_KEY))
            .map(str::to_owned)
    });

    let (outbox, mut frames_rx) = mpsc::channel::<String>(OUTBOX_CAPACITY);
    let session = shared.mutate(|inner| inner.open_session(reconnected_from, outbox));
    let keepalive = shared.config().keepalive_interval;
    let keepalive_timeout_seconds = keepalive.as_secs().max(1);

    let welcome = frames::welcome(&session, keepalive_timeout_seconds);
    if socket.send(Message::Text(welcome.into())).await.is_ok() {
        let mut idle = interval_at(Instant::now() + keepalive, keepalive);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                frame = frames_rx.recv() => {
                    let Some(frame) = frame else { break };
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                    idle.reset();
                }
                _ = idle.tick() => {
                    if socket.send(Message::Text(frames::keepalive().into())).await.is_err() {
                        break;
                    }
                }
                incoming = socket.next() => match incoming {
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                    Some(Ok(_)) => {}
                },
            }
        }
    }
    shared.mutate(|inner| inner.close_session(&session.id));
    let _ = socket.close(None).await;
}

struct QueryCapture<'a>(&'a mut Option<String>);

impl Callback for QueryCapture<'_> {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        *self.0 = request.uri().query().map(str::to_owned);
        Ok(response)
    }
}
