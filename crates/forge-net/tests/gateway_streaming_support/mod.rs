#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_net::{
    BodyProgress, BodySink, Gateway, GatewayFailure, GatewayRequest, StreamFailure,
    StreamedResponse, TransportKind,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const HEADER_END: &[u8] = b"\r\n\r\n";
const READ_CHUNK: usize = 4096;
const CHUNKED_END: &[u8] = b"0\r\n\r\n";
pub(crate) const UNLIMITED: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SinkRefused {
    pub chunk: usize,
}

#[derive(Clone, Default)]
pub(crate) struct MemorySink {
    bytes: Arc<Mutex<Vec<u8>>>,
    accepted_chunks: usize,
    refuse_chunk: Option<usize>,
    delivered: Option<Arc<Notify>>,
    last_accepted: Arc<Mutex<Option<tokio::time::Instant>>>,
}

impl MemorySink {
    pub(crate) fn refusing_chunk(index: usize) -> Self {
        Self {
            refuse_chunk: Some(index),
            ..Self::default()
        }
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }

    pub(crate) fn len(&self) -> u64 {
        u64::try_from(self.bytes.lock().unwrap().len()).unwrap()
    }

    pub(crate) fn last_accepted_at(&self) -> Option<tokio::time::Instant> {
        *self.last_accepted.lock().unwrap()
    }
}

impl BodySink for MemorySink {
    type Error = SinkRefused;

    async fn accept(&mut self, chunk: &[u8]) -> Result<(), SinkRefused> {
        let index = self.accepted_chunks;
        if self.refuse_chunk == Some(index) {
            return Err(SinkRefused { chunk: index });
        }
        self.accepted_chunks += 1;
        self.bytes.lock().unwrap().extend_from_slice(chunk);
        if let Some(delivered) = &self.delivered {
            delivered.notify_one();
        }
        *self.last_accepted.lock().unwrap() = Some(tokio::time::Instant::now());
        Ok(())
    }
}

pub(crate) type StreamOutcome = Result<StreamedResponse, StreamFailure<SinkRefused>>;

pub(crate) struct Streamed {
    pub outcome: StreamOutcome,
    pub sink: MemorySink,
    pub progress: Vec<BodyProgress>,
}

impl Streamed {
    pub(crate) fn failure(&self) -> GatewayFailure {
        match &self.outcome {
            Err(StreamFailure::Gateway(failure)) => failure.clone(),
            other => panic!("expected a gateway failure, got {other:?}"),
        }
    }

    pub(crate) fn response(&self) -> &StreamedResponse {
        match &self.outcome {
            Ok(response) => response,
            Err(failure) => panic!("expected a streamed response, got {failure:?}"),
        }
    }
}

pub(crate) async fn stream(gateway: &Gateway, request: GatewayRequest, cap: u64) -> Streamed {
    stream_with(
        gateway,
        request,
        cap,
        MemorySink::default(),
        &CancellationToken::new(),
    )
    .await
}

pub(crate) async fn stream_into(
    gateway: &Gateway,
    request: GatewayRequest,
    cap: u64,
    sink: MemorySink,
) -> Streamed {
    stream_with(gateway, request, cap, sink, &CancellationToken::new()).await
}

pub(crate) async fn stream_with(
    gateway: &Gateway,
    request: GatewayRequest,
    cap: u64,
    mut sink: MemorySink,
    cancel: &CancellationToken,
) -> Streamed {
    let mut progress = Vec::new();
    let outcome = gateway
        .send_streaming(request, cap, &mut sink, |step| progress.push(step), cancel)
        .await;
    Streamed {
        outcome,
        sink,
        progress,
    }
}

pub(crate) fn spawn_stream(
    gateway: &Gateway,
    request: GatewayRequest,
    sink: MemorySink,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Streamed> {
    let gateway = gateway.clone();
    tokio::spawn(async move { stream_with(&gateway, request, UNLIMITED, sink, &cancel).await })
}

pub(crate) fn transport_kind(failure: &GatewayFailure) -> TransportKind {
    match failure {
        GatewayFailure::Transport(transport) => transport.kind(),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

#[derive(Clone)]
pub(crate) enum Step {
    Send(Vec<u8>),
    Wait(Duration),
    Await(Arc<Notify>),
    Hold,
}

#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<Notify>);

impl Gate {
    pub(crate) fn step(&self) -> Step {
        Step::Await(Arc::clone(&self.0))
    }

    pub(crate) fn open(&self) {
        self.0.notify_one();
    }
}

#[derive(Clone, Default)]
pub(crate) struct Pace(Arc<Notify>);

impl Pace {
    pub(crate) fn sink(&self) -> MemorySink {
        MemorySink {
            delivered: Some(Arc::clone(&self.0)),
            ..MemorySink::default()
        }
    }

    pub(crate) fn chunked_body(&self, chunks: &[&[u8]]) -> Vec<Step> {
        let mut steps = vec![chunked_head(200)];
        for (index, bytes) in chunks.iter().enumerate() {
            if index > 0 {
                steps.push(self.next_delivery());
            }
            steps.push(chunk_frame(bytes));
        }
        steps.push(chunked_end());
        steps
    }

    pub(crate) fn sized_body(&self, pieces: &[&[u8]]) -> Vec<Step> {
        let length = pieces.iter().map(|piece| piece.len()).sum();
        let mut steps = vec![sized_head(length)];
        for (index, piece) in pieces.iter().enumerate() {
            if index > 0 {
                steps.push(self.next_delivery());
            }
            steps.push(Step::Send(piece.to_vec()));
        }
        steps
    }

    fn next_delivery(&self) -> Step {
        Step::Await(Arc::clone(&self.0))
    }
}

pub(crate) fn chunked_head(status: u16) -> Step {
    Step::Send(
        format!(
            "HTTP/1.1 {status} Stub\r\nx-asset: build-7\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )
}

pub(crate) fn sized_head(length: usize) -> Step {
    Step::Send(
        format!(
            "HTTP/1.1 200 Stub\r\nx-asset: build-7\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )
}

pub(crate) fn redirect_to(location: &str) -> Vec<Step> {
    vec![Step::Send(
        format!(
            "HTTP/1.1 302 Stub\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )]
}

pub(crate) fn chunk_frame(bytes: &[u8]) -> Step {
    let mut frame = format!("{:x}\r\n", bytes.len()).into_bytes();
    frame.extend_from_slice(bytes);
    frame.extend_from_slice(b"\r\n");
    Step::Send(frame)
}

pub(crate) fn chunked_end() -> Step {
    Step::Send(CHUNKED_END.to_vec())
}

pub(crate) fn sized_body(pieces: &[&[u8]]) -> Vec<Step> {
    let length = pieces.iter().map(|piece| piece.len()).sum();
    let mut steps = vec![sized_head(length)];
    steps.extend(pieces.iter().map(|piece| Step::Send(piece.to_vec())));
    steps
}

pub(crate) struct StubServer {
    address: SocketAddr,
    heads: Arc<Mutex<Vec<String>>>,
}

impl StubServer {
    pub(crate) async fn serving(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let heads = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&heads);
        tokio::spawn(async move {
            while let Ok((connection, _)) = listener.accept().await {
                tokio::spawn(answer(connection, steps.clone(), Arc::clone(&recorded)));
            }
        });
        Self { address, heads }
    }

    pub(crate) fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(crate) fn heads(&self) -> Vec<String> {
        self.heads.lock().unwrap().clone()
    }
}

async fn answer(mut connection: TcpStream, steps: Vec<Step>, heads: Arc<Mutex<Vec<String>>>) {
    let mut head = Vec::new();
    let mut buffer = [0_u8; READ_CHUNK];
    while !head
        .windows(HEADER_END.len())
        .any(|window| window == HEADER_END)
    {
        match connection.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(read) => head.extend_from_slice(&buffer[..read]),
        }
    }
    heads
        .lock()
        .unwrap()
        .push(String::from_utf8_lossy(&head).into_owned());
    for step in steps {
        match step {
            Step::Send(bytes) => {
                if connection.write_all(&bytes).await.is_err() {
                    return;
                }
                if connection.flush().await.is_err() {
                    return;
                }
            }
            Step::Wait(duration) => tokio::time::sleep(duration).await,
            Step::Await(released) => released.notified().await,
            Step::Hold => std::future::pending::<()>().await,
        }
    }
    connection.shutdown().await.ok();
}

pub(crate) async fn spin_until(mut reached: impl FnMut() -> bool, what: &str) {
    for _ in 0..1_000_000 {
        if reached() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("{what}");
}
