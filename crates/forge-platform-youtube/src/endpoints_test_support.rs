#![allow(clippy::unwrap_used, clippy::panic)]

use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use forge_events::{Event, EventPublisher};
use forge_platform_core::{EndpointSurface, PlatformEndpoints};

use crate::data_api::YoutubeDataApi;
use crate::quota_state::{SharedQuota, today_pacific};

const UNREACHABLE_HTTP: &str = "http://127.0.0.1:0";
const UNREACHABLE_WS: &str = "ws://127.0.0.1:0";
const WS_SURFACE_MARKER: &str = "_WS_";

fn endpoints_with(data_api_base: Option<&str>) -> PlatformEndpoints {
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        PlatformEndpoints::resolve(|variable| {
            EndpointSurface::ALL
                .into_iter()
                .find(|surface| surface.env_var() == variable)
                .map(|surface| {
                    let rest_base = match surface {
                        EndpointSurface::YouTubeDataApi | EndpointSurface::YouTubeUploadApi => {
                            data_api_base.unwrap_or(UNREACHABLE_HTTP)
                        }
                        _ => UNREACHABLE_HTTP,
                    };
                    let base = if variable.contains(WS_SURFACE_MARKER) {
                        UNREACHABLE_WS
                    } else {
                        rest_base
                    };
                    OsString::from(base)
                })
        })
        .unwrap()
    })
}

pub(crate) fn unreachable_endpoints() -> PlatformEndpoints {
    endpoints_with(None)
}

pub(crate) fn data_api_endpoints(base: &str) -> PlatformEndpoints {
    endpoints_with(Some(base))
}

pub(crate) fn data_api_with(
    base: &str,
    quota: SharedQuota,
    bus: Arc<dyn EventPublisher>,
) -> YoutubeDataApi {
    YoutubeDataApi::new(&data_api_endpoints(base), quota, bus).unwrap()
}

pub(crate) fn data_api_charging(base: &str, quota: SharedQuota) -> YoutubeDataApi {
    data_api_with(base, quota, Arc::new(RecordingPublisher::default()))
}

pub(crate) fn data_api_at(base: &str) -> YoutubeDataApi {
    data_api_charging(base, SharedQuota::default())
}

pub(crate) fn unreachable_data_api() -> YoutubeDataApi {
    data_api_at(UNREACHABLE_HTTP)
}

pub(crate) async fn used_today(quota: &SharedQuota) -> u32 {
    quota.lock().await.used_on(today_pacific())
}

pub(crate) async fn exhaust_quota(quota: &SharedQuota) {
    let mut state = quota.lock().await;
    state.last_reset_date = today_pacific();
    state.used_today = u32::MAX;
}

#[derive(Default)]
pub(crate) struct RecordingPublisher {
    events: Mutex<Vec<Event>>,
}

impl RecordingPublisher {
    pub(crate) fn events_of_kind(&self, kind: &str) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == kind)
            .cloned()
            .collect()
    }
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

const SETTLE_TURNS: usize = 256;
const DRIVE_STEP: std::time::Duration = std::time::Duration::from_secs(1);
const HEADER_END: &[u8] = b"\r\n\r\n";
const READ_CHUNK: usize = 4096;

#[derive(Clone)]
pub(crate) struct StubReply {
    status: u16,
    body: String,
    headers: Vec<(String, String)>,
    held: bool,
}

impl StubReply {
    pub(crate) fn json(status: u16, body: &serde_json::Value) -> Self {
        Self {
            status,
            body: body.to_string(),
            headers: Vec::new(),
            held: false,
        }
    }

    pub(crate) fn text(status: u16, body: &str) -> Self {
        Self {
            body: body.to_owned(),
            ..Self::status(status)
        }
    }

    pub(crate) fn held() -> Self {
        Self {
            held: true,
            ..Self::status(reqwest::StatusCode::OK.as_u16())
        }
    }

    pub(crate) fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub(crate) fn ok(body: &serde_json::Value) -> Self {
        Self::json(reqwest::StatusCode::OK.as_u16(), body)
    }

    pub(crate) fn status(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
            headers: Vec::new(),
            held: false,
        }
    }
}

#[derive(Default)]
struct StubRoutes {
    queued: std::collections::HashMap<String, std::collections::VecDeque<StubReply>>,
    standing: std::collections::HashMap<String, StubReply>,
    answered: Vec<String>,
}

impl StubRoutes {
    fn reply_for(&mut self, path: &str) -> StubReply {
        self.answered.push(path.to_owned());
        self.queued
            .get_mut(path)
            .and_then(std::collections::VecDeque::pop_front)
            .or_else(|| self.standing.get(path).cloned())
            .unwrap_or_else(|| StubReply::status(reqwest::StatusCode::NOT_FOUND.as_u16()))
    }
}

pub(crate) struct StubServer {
    address: std::net::SocketAddr,
    routes: Arc<Mutex<StubRoutes>>,
}

impl StubServer {
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(Mutex::new(StubRoutes::default()));
        let serving = Arc::clone(&routes);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(answer(stream, Arc::clone(&serving)));
            }
        });
        Self { address, routes }
    }

    pub(crate) fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(crate) fn reply_times(&self, path: &str, reply: StubReply, times: usize) {
        self.routes
            .lock()
            .unwrap()
            .queued
            .entry(path.to_owned())
            .or_default()
            .extend(std::iter::repeat_n(reply, times));
    }

    pub(crate) fn reply_always(&self, path: &str, reply: StubReply) {
        self.routes
            .lock()
            .unwrap()
            .standing
            .insert(path.to_owned(), reply);
    }

    pub(crate) fn request_count(&self, path: &str) -> usize {
        self.routes
            .lock()
            .unwrap()
            .answered
            .iter()
            .filter(|answered| answered.as_str() == path)
            .count()
    }
}

async fn answer(mut stream: tokio::net::TcpStream, routes: Arc<Mutex<StubRoutes>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut head = Vec::new();
    let mut chunk = [0_u8; READ_CHUNK];
    while !head
        .windows(HEADER_END.len())
        .any(|window| window == HEADER_END)
    {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }
    let request_line = String::from_utf8_lossy(&head);
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let path = target.split('?').next().unwrap_or_default().to_owned();
    let reply = routes.lock().unwrap().reply_for(&path);
    if reply.held {
        std::future::pending::<()>().await;
    }
    let extra_headers: String = reply
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let response = format!(
        "HTTP/1.1 {} Stub\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{extra_headers}\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    );
    if stream.write_all(response.as_bytes()).await.is_ok() {
        stream.shutdown().await.ok();
    }
}

pub(crate) async fn settle() {
    for _ in 0..SETTLE_TURNS {
        tokio::task::yield_now().await;
    }
}

pub(crate) async fn drive_until(
    mut reached: impl FnMut() -> bool,
    virtual_budget: std::time::Duration,
) -> bool {
    let mut driven = std::time::Duration::ZERO;
    loop {
        settle().await;
        if reached() {
            return true;
        }
        if driven >= virtual_budget {
            return false;
        }
        tokio::time::advance(DRIVE_STEP).await;
        driven += DRIVE_STEP;
    }
}

const SPIN_SETTLES: usize = 64;

pub(crate) async fn spin_until(mut reached: impl FnMut() -> bool, what: &str) {
    for _ in 0..SPIN_SETTLES {
        if reached() {
            return;
        }
        settle().await;
    }
    panic!("{what}");
}
