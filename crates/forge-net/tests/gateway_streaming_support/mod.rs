#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_net::{
    BodyProgress, BodySink, Gateway, GatewayFailure, GatewayRequest, StreamFailure,
    StreamedResponse, TransportKind,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;

const HEADER_END: &[u8] = b"\r\n\r\n";
const READ_CHUNK: usize = 4096;
const CHUNKED_END: &[u8] = b"0\r\n\r\n";
pub const UNLIMITED: u64 = u64::MAX;
pub const FRAME_GAP: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkRefused {
    pub chunk: usize,
}

#[derive(Clone, Default)]
pub struct MemorySink {
    bytes: Arc<Mutex<Vec<u8>>>,
    accepted_chunks: usize,
    refuse_chunk: Option<usize>,
}

impl MemorySink {
    pub fn refusing_chunk(index: usize) -> Self {
        Self {
            refuse_chunk: Some(index),
            ..Self::default()
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }

    pub fn len(&self) -> u64 {
        u64::try_from(self.bytes.lock().unwrap().len()).unwrap()
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
        Ok(())
    }
}

pub type StreamOutcome = Result<StreamedResponse, StreamFailure<SinkRefused>>;

pub struct Streamed {
    pub outcome: StreamOutcome,
    pub sink: MemorySink,
    pub progress: Vec<BodyProgress>,
}

impl Streamed {
    pub fn failure(&self) -> GatewayFailure {
        match &self.outcome {
            Err(StreamFailure::Gateway(failure)) => failure.clone(),
            other => panic!("expected a gateway failure, got {other:?}"),
        }
    }

    pub fn response(&self) -> &StreamedResponse {
        match &self.outcome {
            Ok(response) => response,
            Err(failure) => panic!("expected a streamed response, got {failure:?}"),
        }
    }
}

pub async fn stream(gateway: &Gateway, request: GatewayRequest, cap: u64) -> Streamed {
    stream_with(
        gateway,
        request,
        cap,
        MemorySink::default(),
        &CancellationToken::new(),
    )
    .await
}

pub async fn stream_with(
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

pub fn spawn_stream(
    gateway: &Gateway,
    request: GatewayRequest,
    sink: MemorySink,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<Streamed> {
    let gateway = gateway.clone();
    tokio::spawn(async move { stream_with(&gateway, request, UNLIMITED, sink, &cancel).await })
}

pub fn transport_kind(failure: &GatewayFailure) -> TransportKind {
    match failure {
        GatewayFailure::Transport(transport) => transport.kind(),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

#[derive(Clone)]
pub enum Step {
    Send(Vec<u8>),
    Wait(Duration),
    Hold,
}

pub fn chunked_head(status: u16) -> Step {
    Step::Send(
        format!(
            "HTTP/1.1 {status} Stub\r\nx-asset: build-7\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )
}

pub fn sized_head(length: usize) -> Step {
    Step::Send(
        format!(
            "HTTP/1.1 200 Stub\r\nx-asset: build-7\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )
}

pub fn redirect_to(location: &str) -> Vec<Step> {
    vec![Step::Send(
        format!(
            "HTTP/1.1 302 Stub\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )
        .into_bytes(),
    )]
}

pub fn chunk_frame(bytes: &[u8]) -> Step {
    let mut frame = format!("{:x}\r\n", bytes.len()).into_bytes();
    frame.extend_from_slice(bytes);
    frame.extend_from_slice(b"\r\n");
    Step::Send(frame)
}

pub fn chunked_end() -> Step {
    Step::Send(CHUNKED_END.to_vec())
}

pub fn chunked_body(chunks: &[&[u8]], gap: Duration) -> Vec<Step> {
    let mut steps = vec![chunked_head(200)];
    for (index, bytes) in chunks.iter().enumerate() {
        if index > 0 {
            steps.push(Step::Wait(gap));
        }
        steps.push(chunk_frame(bytes));
    }
    steps.push(chunked_end());
    steps
}

pub fn sized_body(pieces: &[&[u8]]) -> Vec<Step> {
    let length = pieces.iter().map(|piece| piece.len()).sum();
    let mut steps = vec![sized_head(length)];
    for (index, piece) in pieces.iter().enumerate() {
        if index > 0 {
            steps.push(Step::Wait(FRAME_GAP));
        }
        steps.push(Step::Send(piece.to_vec()));
    }
    steps
}

pub struct StubServer {
    address: SocketAddr,
    heads: Arc<Mutex<Vec<String>>>,
}

impl StubServer {
    pub async fn serving(steps: Vec<Step>) -> Self {
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

    pub fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn heads(&self) -> Vec<String> {
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
            Step::Hold => std::future::pending::<()>().await,
        }
    }
    connection.shutdown().await.ok();
}

pub async fn spin_until(mut reached: impl FnMut() -> bool, what: &str) {
    for _ in 0..1_000_000 {
        if reached() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("{what}");
}
