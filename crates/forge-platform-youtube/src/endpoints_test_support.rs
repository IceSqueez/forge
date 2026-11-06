use crate::endpoint_surfaces::{YOUTUBE_DATA_API, YOUTUBE_GRPC, YOUTUBE_OAUTH, YOUTUBE_UPLOAD_API};
pub(crate) use forge_events::test_support::CapturingPublisher;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};

use forge_events::EventPublisher;
use forge_platform_core::PlatformEndpoints;

use forge_net::{RateLimiter, TokenBucketRateLimiter};

use crate::account::oauth_client::{
    GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, GoogleOAuthClient,
    google_oauth_gateway,
};
use crate::api::data_api::YouTubeDataApi;
use crate::api::quota_state::{SharedQuota, today_pacific};

pub(crate) const UNREACHABLE_HTTP: &str = "http://127.0.0.1:0";
const UNREACHABLE_WS: &str = "ws://127.0.0.1:0";
const WS_SURFACE_MARKER: &str = "_WS_";
pub(crate) const OAUTH_CLIENT_ID: &str = "test-client-id";
pub(crate) const OAUTH_CLIENT_SECRET: &str = "test-client-secret-value";

fn endpoints_with(data_api_base: Option<&str>) -> PlatformEndpoints {
    endpoints_at(data_api_base, None, None)
}

fn endpoints_at(
    data_api_base: Option<&str>,
    oauth_base: Option<&str>,
    grpc_base: Option<&str>,
) -> PlatformEndpoints {
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        PlatformEndpoints::resolve(crate::endpoint_surfaces::ENDPOINT_SURFACES, |variable| {
            crate::endpoint_surfaces::ENDPOINT_SURFACES
                .iter()
                .copied()
                .find(|surface| surface.env_var() == variable)
                .map(|surface| {
                    let rest_base = match surface {
                        YOUTUBE_DATA_API | YOUTUBE_UPLOAD_API => {
                            data_api_base.unwrap_or(UNREACHABLE_HTTP)
                        }
                        YOUTUBE_OAUTH => oauth_base.unwrap_or(UNREACHABLE_HTTP),
                        YOUTUBE_GRPC => grpc_base.unwrap_or(UNREACHABLE_HTTP),
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

pub(crate) fn data_api_endpoints(base: &str) -> PlatformEndpoints {
    endpoints_with(Some(base))
}

pub(crate) fn data_api_with(
    base: &str,
    quota: SharedQuota,
    bus: Arc<dyn EventPublisher>,
) -> YouTubeDataApi {
    YouTubeDataApi::new(&data_api_endpoints(base), quota, bus).unwrap()
}

pub(crate) fn data_api_streaming(
    rest_base: &str,
    grpc_base: &str,
    quota: SharedQuota,
    bus: Arc<dyn EventPublisher>,
) -> YouTubeDataApi {
    YouTubeDataApi::new(
        &endpoints_at(Some(rest_base), None, Some(grpc_base)),
        quota,
        bus,
    )
    .unwrap()
}

pub(crate) fn data_api_charging(base: &str, quota: SharedQuota) -> YouTubeDataApi {
    data_api_with(
        base,
        quota,
        Arc::new(forge_events::test_support::NullPublisher),
    )
}

pub(crate) fn data_api_at(base: &str) -> YouTubeDataApi {
    data_api_charging(base, SharedQuota::default())
}

pub(crate) fn unreachable_data_api() -> YouTubeDataApi {
    data_api_at(UNREACHABLE_HTTP)
}

pub(crate) fn oauth_client_limited_by(
    base: &str,
    limiter: Arc<dyn RateLimiter>,
) -> GoogleOAuthClient {
    let gateway = google_oauth_gateway(&endpoints_at(None, Some(base), None), limiter).unwrap();
    GoogleOAuthClient::new(
        gateway,
        OAUTH_CLIENT_ID.to_owned(),
        forge_types::Secret::new(OAUTH_CLIENT_SECRET),
    )
}

pub(crate) fn oauth_client_at(base: &str) -> GoogleOAuthClient {
    oauth_client_limited_by(
        base,
        Arc::new(TokenBucketRateLimiter::new(
            GOOGLE_OAUTH_BUDGET_CAPACITY,
            GOOGLE_OAUTH_BUDGET_WINDOW,
        )),
    )
}

pub(crate) fn unreachable_oauth_client() -> GoogleOAuthClient {
    oauth_client_at(UNREACHABLE_HTTP)
}

pub(crate) async fn used_today(quota: &SharedQuota) -> u32 {
    quota.lock().await.used_on(today_pacific())
}

pub(crate) async fn exhaust_quota(quota: &SharedQuota) {
    let mut state = quota.lock().await;
    state.last_reset_date = today_pacific();
    state.used_today = u32::MAX;
}

const WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(1);
pub(crate) const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);
const HEADER_END: &[u8] = b"\r\n\r\n";
const READ_CHUNK: usize = 4096;

#[derive(Clone)]
pub(crate) struct StubReply {
    status: u16,
    body: String,
    headers: Vec<(String, String)>,
    held: bool,
    gate: Option<Arc<ReplyGate>>,
}

#[derive(Default)]
pub(crate) struct ReplyGate {
    arrived: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl ReplyGate {
    pub(crate) async fn arrival(&self) {
        self.arrived.notified().await;
    }

    pub(crate) fn open(&self) {
        self.release.notify_one();
    }
}

impl StubReply {
    pub(crate) fn json(status: u16, body: &serde_json::Value) -> Self {
        Self {
            status,
            body: body.to_string(),
            headers: Vec::new(),
            held: false,
            gate: None,
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

    pub(crate) fn gated_by(mut self, gate: &Arc<ReplyGate>) -> Self {
        self.gate = Some(Arc::clone(gate));
        self
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
            gate: None,
        }
    }
}

#[derive(Default)]
struct StubRoutes {
    queued: std::collections::HashMap<String, std::collections::VecDeque<StubReply>>,
    standing: std::collections::HashMap<String, StubReply>,
    answered: Vec<String>,
    arrived_at: Vec<tokio::time::Instant>,
}

impl StubRoutes {
    fn reply_for(&mut self, path: &str) -> StubReply {
        self.answered.push(path.to_owned());
        self.arrived_at.push(tokio::time::Instant::now());
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
        self.request_counter(path)()
    }

    pub(crate) fn request_counter(&self, path: &str) -> impl Fn() -> usize + Send + Sync + 'static {
        let routes = Arc::clone(&self.routes);
        let path = path.to_owned();
        move || {
            routes
                .lock()
                .unwrap()
                .answered
                .iter()
                .filter(|answered| **answered == path)
                .count()
        }
    }

    pub(crate) fn answered_paths(&self) -> Vec<String> {
        self.routes.lock().unwrap().answered.clone()
    }

    pub(crate) fn arrivals_of(&self, path: &str) -> Vec<tokio::time::Instant> {
        let routes = self.routes.lock().unwrap();
        routes
            .answered
            .iter()
            .zip(&routes.arrived_at)
            .filter(|(answered, _)| answered.as_str() == path)
            .map(|(_, at)| *at)
            .collect()
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
    let head_text = String::from_utf8_lossy(&head).into_owned();
    let header_len = head
        .windows(HEADER_END.len())
        .position(|window| window == HEADER_END)
        .unwrap_or_default()
        + HEADER_END.len();
    let body_len = head_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or_default();
    while head.len() < header_len + body_len {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => head.extend_from_slice(&chunk[..read]),
        }
    }
    let request_line = head_text;
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let path = target.split('?').next().unwrap_or_default().to_owned();
    let reply = routes.lock().unwrap().reply_for(&path);
    if reply.held {
        std::future::pending::<()>().await;
    }
    if let Some(gate) = &reply.gate {
        gate.arrived.notify_one();
        gate.release.notified().await;
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

pub(crate) async fn wait_until(mut reached: impl FnMut() -> bool) -> bool {
    tokio::time::timeout(HANG_GUARD, async {
        while !reached() {
            tokio::time::sleep(WAIT_STEP).await;
        }
    })
    .await
    .is_ok()
}

const GRPC_CONTENT_TYPE: &str = "application/grpc";
const GRPC_STATUS: &str = "grpc-status";
const GRPC_MESSAGE: &str = "grpc-message";
const GRPC_DETAILS: &str = "grpc-status-details-bin";
const GRPC_FRAME_PREFIX: usize = 5;
const UNCOMPRESSED_FRAME: u8 = 0;
const ERROR_INFO_TYPE_URL: &str = "type.googleapis.com/google.rpc.ErrorInfo";
const RETRY_INFO_TYPE_URL: &str = "type.googleapis.com/google.rpc.RetryInfo";
pub(crate) const GRPC_OK: &str = "0";

pub(crate) type GrpcPairs = Vec<(&'static str, String)>;

#[derive(Clone)]
pub(crate) enum GrpcStep {
    Head {
        status: u16,
        headers: GrpcPairs,
        end: bool,
    },
    Data(Vec<u8>),
    Trailers(GrpcPairs),
    Hold,
}

#[derive(Debug, Clone)]
pub(crate) struct GrpcSeen {
    pub(crate) path: String,
    pub(crate) headers: http::HeaderMap,
    pub(crate) body: Vec<u8>,
}

impl GrpcSeen {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    pub(crate) fn message(&self) -> &[u8] {
        self.body.get(GRPC_FRAME_PREFIX..).unwrap_or_default()
    }
}

type GrpcScript = Arc<dyn Fn(&GrpcSeen) -> Vec<GrpcStep> + Send + Sync>;

pub(crate) struct GrpcStub {
    address: std::net::SocketAddr,
    seen: Arc<Mutex<Vec<GrpcSeen>>>,
}

impl GrpcStub {
    pub(crate) async fn serving(steps: Vec<GrpcStep>) -> Self {
        Self::scripted(move |_| steps.clone()).await
    }

    pub(crate) async fn scripted(
        script: impl Fn(&GrpcSeen) -> Vec<GrpcStep> + Send + Sync + 'static,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let script: GrpcScript = Arc::new(script);
        let recorded = Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve_grpc(
                    socket,
                    Arc::clone(&script),
                    Arc::clone(&recorded),
                ));
            }
        });
        Self { address, seen }
    }

    pub(crate) fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(crate) fn seen(&self) -> Vec<GrpcSeen> {
        self.seen.lock().unwrap().clone()
    }

    pub(crate) fn seen_counter(&self) -> impl Fn() -> usize + Send + Sync + 'static {
        let seen = Arc::clone(&self.seen);
        move || seen.lock().unwrap().len()
    }
}

async fn serve_grpc(
    socket: tokio::net::TcpStream,
    script: GrpcScript,
    seen: Arc<Mutex<Vec<GrpcSeen>>>,
) {
    let Ok(mut connection) = h2::server::handshake(socket).await else {
        return;
    };
    while let Some(Ok((request, respond))) = connection.accept().await {
        tokio::spawn(answer_grpc(
            request,
            respond,
            Arc::clone(&script),
            Arc::clone(&seen),
        ));
    }
}

async fn answer_grpc(
    request: http::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<bytes::Bytes>,
    script: GrpcScript,
    seen: Arc<Mutex<Vec<GrpcSeen>>>,
) {
    let (head, mut body) = request.into_parts();
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = body.data().await {
        body.flow_control().release_capacity(chunk.len()).ok();
        bytes.extend_from_slice(&chunk);
    }
    let recorded = GrpcSeen {
        path: head.uri.path().to_owned(),
        headers: head.headers,
        body: bytes,
    };
    let steps = script(&recorded);
    seen.lock().unwrap().push(recorded);
    let mut send = None;
    for step in steps {
        match step {
            GrpcStep::Head {
                status,
                headers,
                end,
            } => {
                let mut response = http::Response::builder().status(status);
                for (name, value) in headers {
                    response = response.header(name, value);
                }
                match respond.send_response(response.body(()).unwrap(), end) {
                    Ok(stream) => send = Some(stream),
                    Err(_) => return,
                }
            }
            GrpcStep::Data(data) => {
                let Some(stream) = send.as_mut() else { return };
                if stream.send_data(bytes::Bytes::from(data), false).is_err() {
                    return;
                }
            }
            GrpcStep::Trailers(pairs) => {
                let Some(stream) = send.as_mut() else { return };
                let mut map = http::HeaderMap::new();
                for (name, value) in pairs {
                    map.append(
                        http::HeaderName::from_static(name),
                        http::HeaderValue::from_str(&value).unwrap(),
                    );
                }
                if stream.send_trailers(map).is_err() {
                    return;
                }
            }
            GrpcStep::Hold => std::future::pending::<()>().await,
        }
    }
}

pub(crate) fn grpc_frame(message: &[u8]) -> GrpcStep {
    let mut framed = vec![UNCOMPRESSED_FRAME];
    framed.extend_from_slice(&u32::try_from(message.len()).unwrap().to_be_bytes());
    framed.extend_from_slice(message);
    GrpcStep::Data(framed)
}

pub(crate) fn grpc_head() -> GrpcStep {
    GrpcStep::Head {
        status: reqwest::StatusCode::OK.as_u16(),
        headers: vec![("content-type", GRPC_CONTENT_TYPE.to_owned())],
        end: false,
    }
}

pub(crate) fn grpc_ok_trailers() -> GrpcStep {
    GrpcStep::Trailers(vec![(GRPC_STATUS, GRPC_OK.to_owned())])
}

pub(crate) fn grpc_trailers_only(pairs: GrpcPairs) -> GrpcStep {
    let mut headers = vec![("content-type", GRPC_CONTENT_TYPE.to_owned())];
    headers.extend(pairs);
    GrpcStep::Head {
        status: reqwest::StatusCode::OK.as_u16(),
        headers,
        end: true,
    }
}

pub(crate) fn grpc_status(code: &str) -> GrpcPairs {
    vec![(GRPC_STATUS, code.to_owned())]
}

pub(crate) fn grpc_status_with_message(code: &str, message: &str) -> GrpcPairs {
    let mut pairs = grpc_status(code);
    pairs.push((GRPC_MESSAGE, message.to_owned()));
    pairs
}

fn status_details(code: &str, details: &[prost_types::Any]) -> GrpcPairs {
    use base64::Engine;
    let mut status = Vec::new();
    prost::encoding::int32::encode(1, &code.parse::<i32>().unwrap(), &mut status);
    for detail in details {
        prost::encoding::message::encode(3, detail, &mut status);
    }
    let mut pairs = grpc_status(code);
    pairs.push((
        GRPC_DETAILS,
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(status),
    ));
    pairs
}

pub(crate) fn grpc_status_with_reason(code: &str, reason: &str) -> GrpcPairs {
    let mut info = Vec::new();
    prost::encoding::string::encode(1, &reason.to_owned(), &mut info);
    status_details(
        code,
        &[prost_types::Any {
            type_url: ERROR_INFO_TYPE_URL.to_owned(),
            value: info,
        }],
    )
}

pub(crate) fn grpc_status_with_retry(code: &str, retry: std::time::Duration) -> GrpcPairs {
    let mut info = Vec::new();
    let delay = prost_types::Duration {
        seconds: i64::try_from(retry.as_secs()).unwrap(),
        nanos: 0,
    };
    prost::encoding::message::encode(1, &delay, &mut info);
    status_details(
        code,
        &[prost_types::Any {
            type_url: RETRY_INFO_TYPE_URL.to_owned(),
            value: info,
        }],
    )
}
