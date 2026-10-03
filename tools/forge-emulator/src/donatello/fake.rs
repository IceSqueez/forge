use std::collections::{HashMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Json, Response};
use forge_platform_core::EndpointSurface;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::config::{DonatesOrder, FakeDonatelloConfig, FakeDonation};
use crate::EmulatorError;

pub const UNAUTHORIZED_MESSAGE: &str = "Помилка авторизації";
pub const PROFILE_INCOMPLETE_MESSAGE: &str = "Неповні налаштування профілю";
pub const DEFAULT_PAGE_SIZE: u64 = 20;
pub const MAX_PAGE_SIZE: u64 = 100;

const TOKEN_HEADER: &str = "x-token";
const ME_PATH: &str = "/api/v1/me";
pub const DONATES_PATH: &str = "/api/v1/donates";
const API_PREFIX: &str = "/api/v1";
const PAGE_PARAMETER: &str = "page";
const SIZE_PARAMETER: &str = "size";
const MALFORMED_BODY: &str = "{\"content\": [ {\"pubId\": ";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DonatesFault {
    TooManyRequests { retry_after_secs: Option<u32> },
    ServerError { status: u16 },
    MalformedJson,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedDonatelloRequest {
    pub path: String,
    pub page: Option<u64>,
    pub size: Option<u64>,
    pub token_accepted: bool,
    pub status: u16,
}

struct Inner {
    config: FakeDonatelloConfig,
    donations: Vec<Value>,
    faults: VecDeque<DonatesFault>,
    requests: Vec<RecordedDonatelloRequest>,
}

struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<usize>,
}

impl Shared {
    fn mutate<T>(&self, change: impl FnOnce(&mut Inner) -> T) -> T {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let result = change(&mut inner);
        let recorded = inner.requests.len();
        drop(inner);
        self.changes.send_replace(recorded);
        result
    }

    fn read<T>(&self, look: impl FnOnce(&Inner) -> T) -> T {
        look(&self.inner.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

pub struct FakeDonatello {
    shared: Arc<Shared>,
    base_url: String,
    shutdown: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl FakeDonatello {
    pub async fn start(config: FakeDonatelloConfig) -> Result<Self, EmulatorError> {
        if config.token.trim().is_empty() {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: "the fake Donatello token is blank".to_owned(),
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| EmulatorError::FakeBind {
                reason: e.to_string(),
            })?;
        let address: SocketAddr = listener.local_addr().map_err(|e| EmulatorError::FakeBind {
            reason: e.to_string(),
        })?;
        let donations = config.donations.iter().map(FakeDonation::wire).collect();
        let (changes, _) = watch::channel(0);
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                config,
                donations,
                faults: VecDeque::new(),
                requests: Vec::new(),
            }),
            changes,
        });
        let (shutdown, mut shutdown_rx) = watch::channel(false);
        let router = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.changed().await;
                })
                .await;
        });
        Ok(Self {
            shared,
            base_url: format!("http://{address}{API_PREFIX}"),
            shutdown,
            task: Some(task),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn endpoint_override(&self) -> (&'static str, String) {
        (
            EndpointSurface::DonatelloApi.env_var(),
            self.base_url.clone(),
        )
    }

    pub fn donate(&self, donation: FakeDonation) {
        self.shared
            .mutate(|inner| inner.donations.push(donation.wire()));
    }

    pub fn fail_next_donates(&self, fault: DonatesFault) {
        self.shared.mutate(|inner| inner.faults.push_back(fault));
    }

    pub fn set_profile_complete(&self, complete: bool) {
        self.shared
            .mutate(|inner| inner.config.profile_complete = complete);
    }

    pub fn set_order(&self, order: DonatesOrder) {
        self.shared.mutate(|inner| inner.config.order = order);
    }

    pub fn requests(&self) -> Vec<RecordedDonatelloRequest> {
        self.shared.read(|inner| inner.requests.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&[RecordedDonatelloRequest]) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        let mut changes = self.shared.changes.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(found) = self.shared.read(|inner| probe(&inner.requests)) {
                return Ok(found);
            }
            match tokio::time::timeout_at(deadline, changes.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    return Err(EmulatorError::WaitTimeout {
                        what: what.to_owned(),
                    });
                }
            }
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        if let Some(mut task) = self.task.take()
            && tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
                .await
                .is_err()
        {
            task.abort();
        }
    }
}

impl Drop for FakeDonatello {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

enum Body {
    Json(Value),
    Raw(&'static str),
    Empty,
}

struct Answer {
    status: StatusCode,
    body: Body,
    retry_after_secs: Option<u32>,
}

impl Answer {
    fn json(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            body: Body::Json(body),
            retry_after_secs: None,
        }
    }

    fn refusal(status: StatusCode, message: &str) -> Self {
        Self::json(status, json!({ "success": false, "message": message }))
    }
}

async fn handle(State(shared): State<Arc<Shared>>, uri: Uri, headers: HeaderMap) -> Response {
    let query: HashMap<String, String> = Query::<HashMap<String, String>>::try_from_uri(&uri)
        .map(|Query(pairs)| pairs)
        .unwrap_or_default();
    let page = query.get(PAGE_PARAMETER).map(|raw| raw.parse::<u64>());
    let size = query.get(SIZE_PARAMETER).map(|raw| raw.parse::<u64>());
    let presented = headers
        .get(TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let answer = shared.mutate(|inner| {
        let token_accepted = presented.as_deref() == Some(inner.config.token.as_str());
        let answer = answer(
            inner,
            uri.path(),
            token_accepted,
            page.clone(),
            size.clone(),
        );
        inner.requests.push(RecordedDonatelloRequest {
            path: uri.path().to_owned(),
            page: page.clone().and_then(Result::ok),
            size: size.clone().and_then(Result::ok),
            token_accepted,
            status: answer.status.as_u16(),
        });
        answer
    });
    respond(answer)
}

type Parsed = Option<Result<u64, std::num::ParseIntError>>;

fn answer(
    inner: &mut Inner,
    path: &str,
    token_accepted: bool,
    page: Parsed,
    size: Parsed,
) -> Answer {
    if !token_accepted {
        return Answer::refusal(StatusCode::UNAUTHORIZED, UNAUTHORIZED_MESSAGE);
    }
    match path {
        ME_PATH => me(&inner.config),
        DONATES_PATH => match inner.faults.pop_front() {
            Some(fault) => faulted(fault),
            None => donates_page(inner, page, size),
        },
        _ => Answer::refusal(StatusCode::NOT_FOUND, "not found"),
    }
}

fn me(config: &FakeDonatelloConfig) -> Answer {
    if !config.profile_complete {
        return Answer::refusal(StatusCode::NOT_FOUND, PROFILE_INCOMPLETE_MESSAGE);
    }
    Answer::json(
        StatusCode::OK,
        json!({
            "nickname": config.nickname,
            "pubId": "U-FAKE-0001",
            "page": format!("https://donatello.to/{}", config.nickname),
            "isActive": true,
            "isPublic": true,
            "donates": { "totalAmount": "0", "totalCount": "0" },
            "createdAt": "2025-01-01 00:00:00",
        }),
    )
}

fn faulted(fault: DonatesFault) -> Answer {
    match fault {
        DonatesFault::TooManyRequests { retry_after_secs } => Answer {
            retry_after_secs,
            ..Answer::refusal(StatusCode::TOO_MANY_REQUESTS, "too many requests")
        },
        DonatesFault::ServerError { status } => Answer {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            body: Body::Empty,
            retry_after_secs: None,
        },
        DonatesFault::MalformedJson => Answer {
            status: StatusCode::OK,
            body: Body::Raw(MALFORMED_BODY),
            retry_after_secs: None,
        },
    }
}

fn donates_page(inner: &Inner, page: Parsed, size: Parsed) -> Answer {
    let (Ok(page), Ok(size)) = (page.unwrap_or(Ok(0)), size.unwrap_or(Ok(DEFAULT_PAGE_SIZE)))
    else {
        return Answer::refusal(StatusCode::BAD_REQUEST, "page and size must be numbers");
    };
    let size = size.clamp(1, MAX_PAGE_SIZE);
    let mut listed = inner.donations.clone();
    if inner.config.order == DonatesOrder::NewestFirst {
        listed.reverse();
    }
    let total = listed.len() as u64;
    let pages = total.div_ceil(size);
    let content: Vec<Value> = listed
        .into_iter()
        .skip(usize::try_from(page.saturating_mul(size)).unwrap_or(usize::MAX))
        .take(usize::try_from(size).unwrap_or(usize::MAX))
        .collect();
    Answer::json(
        StatusCode::OK,
        json!({
            "content": content,
            "page": page,
            "size": size,
            "pages": pages,
            "first": page == 0,
            "last": page.saturating_add(1) >= pages,
            "total": total,
        }),
    )
}

fn respond(answer: Answer) -> Response {
    let mut response = match answer.body {
        Body::Json(body) => (answer.status, Json(body)).into_response(),
        Body::Raw(text) => (
            answer.status,
            [(header::CONTENT_TYPE, "application/json")],
            text,
        )
            .into_response(),
        Body::Empty => answer.status.into_response(),
    };
    if let Some(secs) = answer.retry_after_secs {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from(secs));
    }
    response
}
