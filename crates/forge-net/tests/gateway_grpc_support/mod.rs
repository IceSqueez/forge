#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use bytes::Bytes;
use forge_net::{GatewayFailure, GrpcItem, GrpcStream, TransportKind};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Response, Version};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

pub(crate) const GRPC: &str = "application/grpc";
pub(crate) const CONTENT_TYPE: &str = "content-type";
pub(crate) const GRPC_STATUS: &str = "grpc-status";
pub(crate) const GRPC_MESSAGE: &str = "grpc-message";
pub(crate) const GRPC_DETAILS: &str = "grpc-status-details-bin";
pub(crate) const STATUS_OK: &str = "0";
pub(crate) const HTTP_OK: u16 = 200;
pub(crate) const UNCOMPRESSED: u8 = 0;
pub(crate) const ERROR_INFO_URL: &str = "type.googleapis.com/google.rpc.ErrorInfo";
pub(crate) const RETRY_INFO_URL: &str = "type.googleapis.com/google.rpc.RetryInfo";
const WIRE_VARINT: u64 = 0;
const WIRE_LENGTH_DELIMITED: u64 = 2;
const VARINT_PAYLOAD: u64 = 0x7f;
const VARINT_CONTINUES: u8 = 0x80;

pub(crate) type Pairs = Vec<(&'static str, String)>;

#[derive(Clone)]
pub(crate) enum Step {
    Head {
        status: u16,
        headers: Pairs,
        end: bool,
    },
    Data(Vec<u8>),
    Trailers(Pairs),
    Finish,
    Hold,
}

#[derive(Debug, Clone)]
pub(crate) struct Seen {
    pub method: Method,
    pub path: String,
    pub version: Version,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Seen {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

type Script = Arc<dyn Fn(&Seen) -> Vec<Step> + Send + Sync>;

pub(crate) struct H2Server {
    address: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    arrived: Arc<Notify>,
}

impl H2Server {
    pub(crate) async fn serving(steps: Vec<Step>) -> Self {
        Self::scripted(move |_| steps.clone()).await
    }

    pub(crate) async fn scripted(
        script: impl Fn(&Seen) -> Vec<Step> + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let arrived = Arc::new(Notify::new());
        let script: Script = Arc::new(script);
        let (recorded, signal) = (Arc::clone(&seen), Arc::clone(&arrived));
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve(
                    socket,
                    Arc::clone(&script),
                    Arc::clone(&recorded),
                    Arc::clone(&signal),
                ));
            }
        });
        Self {
            address,
            seen,
            arrived,
        }
    }

    pub(crate) fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    pub(crate) async fn request_arrived(&self) {
        self.arrived.notified().await;
    }
}

async fn serve(
    socket: TcpStream,
    script: Script,
    seen: Arc<Mutex<Vec<Seen>>>,
    arrived: Arc<Notify>,
) {
    let Ok(mut connection) = h2::server::handshake(socket).await else {
        return;
    };
    while let Some(Ok((request, respond))) = connection.accept().await {
        tokio::spawn(answer(
            request,
            respond,
            Arc::clone(&script),
            Arc::clone(&seen),
            Arc::clone(&arrived),
        ));
    }
}

async fn answer(
    request: http::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<Bytes>,
    script: Script,
    seen: Arc<Mutex<Vec<Seen>>>,
    arrived: Arc<Notify>,
) {
    let (head, mut body) = request.into_parts();
    let mut bytes = Vec::new();
    while let Some(Ok(chunk)) = body.data().await {
        body.flow_control().release_capacity(chunk.len()).ok();
        bytes.extend_from_slice(&chunk);
    }
    let recorded = Seen {
        method: head.method,
        path: head.uri.path().to_owned(),
        version: head.version,
        headers: head.headers,
        body: bytes,
    };
    let steps = script(&recorded);
    seen.lock().unwrap().push(recorded);
    arrived.notify_one();
    let mut send = None;
    for step in steps {
        match step {
            Step::Head {
                status,
                headers,
                end,
            } => {
                let mut response = Response::builder().status(status);
                for (name, value) in headers {
                    response = response.header(name, value);
                }
                match respond.send_response(response.body(()).unwrap(), end) {
                    Ok(stream) => send = Some(stream),
                    Err(_) => return,
                }
            }
            Step::Data(bytes) => {
                let stream = send.as_mut().unwrap();
                if stream.send_data(Bytes::from(bytes), false).is_err() {
                    return;
                }
            }
            Step::Trailers(pairs) => {
                let stream = send.as_mut().unwrap();
                if stream.send_trailers(header_map(pairs)).is_err() {
                    return;
                }
            }
            Step::Finish => {
                let stream = send.as_mut().unwrap();
                if stream.send_data(Bytes::new(), true).is_err() {
                    return;
                }
            }
            Step::Hold => std::future::pending::<()>().await,
        }
    }
}

fn header_map(pairs: Pairs) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            HeaderName::from_static(name),
            HeaderValue::from_str(&value).unwrap(),
        );
    }
    map
}

pub(crate) fn frame(message: &[u8]) -> Vec<u8> {
    frame_with_flag(UNCOMPRESSED, message)
}

pub(crate) fn frame_with_flag(flag: u8, message: &[u8]) -> Vec<u8> {
    let mut framed = vec![flag];
    framed.extend_from_slice(&u32::try_from(message.len()).unwrap().to_be_bytes());
    framed.extend_from_slice(message);
    framed
}

pub(crate) fn grpc_head() -> Step {
    head_with(HTTP_OK, GRPC, Vec::new(), false)
}

pub(crate) fn head_with(status: u16, content_type: &str, extra: Pairs, end: bool) -> Step {
    let mut headers = vec![(CONTENT_TYPE, content_type.to_owned())];
    headers.extend(extra);
    Step::Head {
        status,
        headers,
        end,
    }
}

pub(crate) fn trailers_only(pairs: Pairs) -> Step {
    head_with(HTTP_OK, GRPC, pairs, true)
}

pub(crate) fn status_pairs(code: &str) -> Pairs {
    vec![(GRPC_STATUS, code.to_owned())]
}

pub(crate) fn ok_trailers() -> Step {
    Step::Trailers(status_pairs(STATUS_OK))
}

pub(crate) fn ending_with(code: &str) -> Vec<Step> {
    vec![grpc_head(), Step::Trailers(status_pairs(code))]
}

pub(crate) async fn drain(stream: &mut GrpcStream) -> Vec<GrpcItem> {
    let mut items = Vec::new();
    while let Some(item) = stream.next().await {
        items.push(item);
    }
    items
}

pub(crate) fn end_of(items: &[GrpcItem]) -> Result<(), GatewayFailure> {
    match items.last() {
        Some(GrpcItem::End(end)) => end.clone(),
        other => panic!("expected the stream to end with End, got {other:?}"),
    }
}

pub(crate) fn failure_of(items: &[GrpcItem]) -> GatewayFailure {
    match end_of(items) {
        Err(failure) => failure,
        Ok(()) => panic!("expected a failed End, got {items:?}"),
    }
}

pub(crate) fn kind_of(failure: &GatewayFailure) -> TransportKind {
    match failure {
        GatewayFailure::Transport(transport) => transport.kind(),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

pub(crate) fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let low = u8::try_from(value & VARINT_PAYLOAD).unwrap();
        value >>= 7;
        if value == 0 {
            out.push(low);
            return out;
        }
        out.push(low | VARINT_CONTINUES);
    }
}

pub(crate) fn bytes_field(number: u64, bytes: &[u8]) -> Vec<u8> {
    let mut out = varint((number << 3) | WIRE_LENGTH_DELIMITED);
    out.extend(varint(u64::try_from(bytes.len()).unwrap()));
    out.extend_from_slice(bytes);
    out
}

pub(crate) fn varint_field(number: u64, value: u64) -> Vec<u8> {
    let mut out = varint((number << 3) | WIRE_VARINT);
    out.extend(varint(value));
    out
}

pub(crate) fn any(type_url: &str, value: &[u8]) -> Vec<u8> {
    let mut out = bytes_field(1, type_url.as_bytes());
    out.extend(bytes_field(2, value));
    out
}

pub(crate) fn error_info(reason: &str) -> Vec<u8> {
    any(ERROR_INFO_URL, &bytes_field(1, reason.as_bytes()))
}

pub(crate) fn retry_info(seconds: u64, nanos: u64) -> Vec<u8> {
    let mut duration = varint_field(1, seconds);
    duration.extend(varint_field(2, nanos));
    any(RETRY_INFO_URL, &bytes_field(1, &duration))
}

pub(crate) fn status_proto(message: Option<&str>, details: &[Vec<u8>]) -> Vec<u8> {
    let mut out = varint_field(1, 8);
    if let Some(message) = message {
        out.extend(bytes_field(2, message.as_bytes()));
    }
    for detail in details {
        out.extend(bytes_field(3, detail));
    }
    out
}

pub(crate) fn details_bin(status: &[u8]) -> String {
    STANDARD_NO_PAD.encode(status)
}

pub(crate) fn padded_details_bin(status: &[u8]) -> String {
    STANDARD.encode(status)
}
