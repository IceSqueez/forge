#![allow(dead_code, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener as StdTcpListener, TcpStream as StdTcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, FailureContext, FailureObserver, Gateway, GatewayFailure, GatewayProfile,
    GatewayRequest, GatewayResponse, ThrottleHintFamily,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::MockServer;

pub const PLATFORM_NO_HINT_WAIT: Duration = Duration::from_secs(5);
pub const HEADER_END: &[u8] = b"\r\n\r\n";

pub fn platform() -> GatewayProfile {
    platform_with(ThrottleHintFamily::RetryAfter)
}

pub fn platform_with(family: ThrottleHintFamily) -> GatewayProfile {
    GatewayProfile::platform_api(CallClass::Interactive, family, PLATFORM_NO_HINT_WAIT)
}

#[derive(Default)]
pub struct RecordingObserver {
    seen: Mutex<Vec<(GatewayFailure, FailureContext)>>,
}

impl RecordingObserver {
    pub fn seen(&self) -> Vec<(GatewayFailure, FailureContext)> {
        self.seen.lock().unwrap().clone()
    }
}

impl FailureObserver for RecordingObserver {
    fn on_failure(&self, failure: &GatewayFailure, context: FailureContext) {
        self.seen.lock().unwrap().push((failure.clone(), context));
    }
}

pub struct Harness {
    pub gateway: Gateway,
    pub limiter: Arc<RecordingRateLimiter>,
    pub observer: Arc<RecordingObserver>,
}

impl Harness {
    pub fn new(base: &str, profile: GatewayProfile) -> Self {
        Self::with_limiter(base, profile, RecordingRateLimiter::granting())
    }

    pub fn with_limiter(
        base: &str,
        profile: GatewayProfile,
        limiter: RecordingRateLimiter,
    ) -> Self {
        let limiter = Arc::new(limiter);
        let observer = Arc::new(RecordingObserver::default());
        let gateway = Gateway::new(
            base,
            limiter.clone(),
            profile,
            Some(observer.clone() as Arc<dyn FailureObserver>),
        )
        .unwrap();
        Self {
            gateway,
            limiter,
            observer,
        }
    }

    pub async fn ok(&self, request: GatewayRequest) -> GatewayResponse {
        self.gateway.send(request).await.unwrap()
    }

    pub async fn fail(&self, request: GatewayRequest) -> GatewayFailure {
        self.gateway.send(request).await.unwrap_err()
    }
}

pub async fn received(server: &MockServer) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap_or_default()
}

pub async fn serve_raw_once(response: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !seen.windows(HEADER_END.len()).any(|w| w == HEADER_END) {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                return;
            }
            seen.extend_from_slice(&buffer[..read]);
        }
        stream.write_all(&response).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    format!("http://{address}")
}

pub fn chunked_ok(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
    for chunk in chunks {
        out.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

pub fn silent_listener() -> (StdTcpListener, String) {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, format!("http://{address}"))
}

pub fn accept_request_head(listener: &StdTcpListener) -> StdTcpStream {
    let (mut stream, _) = listener.accept().unwrap();
    let mut seen = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !seen.windows(HEADER_END.len()).any(|w| w == HEADER_END) {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0, "client closed before sending the request head");
        seen.extend_from_slice(&buffer[..read]);
    }
    stream.flush().unwrap();
    stream
}

pub fn refused_base() -> String {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{address}")
}
