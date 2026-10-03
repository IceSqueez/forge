#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_donatello::{
    DONATELLO_CREDENTIAL_ID, DonatelloConfig, DonatelloProvider, MIN_POLL_INTERVAL, PollStatus,
};
use forge_platform_core::{
    DonationProvider, DonationStream, PlatformError, RateLimitOutcome, RateLimiter,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Donation;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TOKEN: &str = "dn-test-token-7f3a9c";
pub const OTHER_TOKEN: &str = "dn-other-token-41be02";
pub const WAIT: Duration = Duration::from_secs(5);
pub const QUIET: Duration = Duration::from_millis(50);

#[derive(Default)]
pub struct MemCreds {
    entries: Mutex<HashMap<String, String>>,
}

impl MemCreds {
    pub fn with_token(token: &str) -> Arc<Self> {
        let creds = Self::default();
        creds.entries.lock().unwrap().insert(
            DONATELLO_CREDENTIAL_ID.to_owned(),
            json!({ "token": token }).to_string(),
        );
        Arc::new(creds)
    }

    pub fn stored(&self) -> Option<String> {
        self.entries
            .lock()
            .unwrap()
            .get(DONATELLO_CREDENTIAL_ID)
            .cloned()
    }
}

#[async_trait]
impl CredentialsRepo for MemCreds {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.entries
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), plaintext_bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.entries.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.entries.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .keys()
            .map(CredentialId::new)
            .collect())
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

pub struct ScriptedLimiter {
    exhausted: bool,
    throttles: Mutex<Vec<Duration>>,
}

impl ScriptedLimiter {
    pub fn open() -> Arc<Self> {
        Arc::new(Self {
            exhausted: false,
            throttles: Mutex::new(Vec::new()),
        })
    }

    pub fn exhausted() -> Arc<Self> {
        Arc::new(Self {
            exhausted: true,
            throttles: Mutex::new(Vec::new()),
        })
    }

    pub fn throttles(&self) -> Vec<Duration> {
        self.throttles.lock().unwrap().clone()
    }
}

#[async_trait]
impl RateLimiter for ScriptedLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(if self.exhausted {
            RateLimitOutcome::Exhausted
        } else {
            RateLimitOutcome::Granted
        })
    }

    fn remaining(&self) -> u32 {
        u32::from(!self.exhausted)
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.throttles.lock().unwrap().push(retry_after);
    }
}

pub fn config(base_url: &str) -> DonatelloConfig {
    DonatelloConfig {
        base_url: base_url.to_owned(),
        poll_interval: MIN_POLL_INTERVAL,
        request_timeout: WAIT,
    }
}

pub fn donate(id: &str, created_at: &str) -> Value {
    json!({
        "pubId": id,
        "clientName": format!("donor {id}"),
        "message": format!("message {id}"),
        "amount": "100",
        "currency": "UAH",
        "goal": "",
        "isPublished": true,
        "createdAt": created_at,
    })
}

pub fn donates(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            let minute = index % 60;
            let hour = index / 60;
            donate(
                &format!("D-{index:03}"),
                &format!("2026-07-01 {:02}:{minute:02}:00", 10 + hour),
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    NewestFirst,
    OldestFirst,
}

struct ListState {
    oldest_first: Vec<Value>,
    order: Order,
    requested_pages: Vec<u64>,
    faults: VecDeque<ResponseTemplate>,
}

#[derive(Clone)]
pub struct DonatesList {
    state: Arc<Mutex<ListState>>,
}

impl DonatesList {
    fn new(order: Order, initial: Vec<Value>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ListState {
                oldest_first: initial,
                order,
                requested_pages: Vec::new(),
                faults: VecDeque::new(),
            })),
        }
    }

    pub fn add(&self, item: Value) {
        self.state.lock().unwrap().oldest_first.push(item);
    }

    pub fn fail_next(&self, response: ResponseTemplate) {
        self.state.lock().unwrap().faults.push_back(response);
    }

    pub fn requested_pages(&self) -> Vec<u64> {
        self.state.lock().unwrap().requested_pages.clone()
    }

    pub fn forget_requests(&self) {
        self.state.lock().unwrap().requested_pages.clear();
    }
}

impl Respond for DonatesList {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let query: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let page: u64 = query.get("page").and_then(|p| p.parse().ok()).unwrap_or(0);
        let size: u64 = query.get("size").and_then(|s| s.parse().ok()).unwrap_or(20);
        let mut state = self.state.lock().unwrap();
        state.requested_pages.push(page);
        if let Some(fault) = state.faults.pop_front() {
            return fault;
        }
        let mut items = state.oldest_first.clone();
        if state.order == Order::NewestFirst {
            items.reverse();
        }
        let total = items.len() as u64;
        let pages = total.div_ceil(size);
        let content: Vec<Value> = items
            .into_iter()
            .skip((page * size) as usize)
            .take(size as usize)
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({
            "content": content,
            "page": page,
            "size": size,
            "pages": pages,
            "first": page == 0,
            "last": page + 1 >= pages,
            "total": total,
        }))
    }
}

pub async fn mount_me(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "nickname": "streamer",
            "pubId": "U-1",
            "page": "https://donatello.to/streamer",
            "isActive": true,
            "isPublic": true,
            "donates": { "totalAmount": "0", "totalCount": "0" },
            "createdAt": "2025-01-01 00:00:00",
        })))
        .mount(server)
        .await;
}

pub async fn mount_donates(server: &MockServer, list: &DonatesList) {
    Mock::given(method("GET"))
        .and(path("/donates"))
        .respond_with(list.clone())
        .mount(server)
        .await;
}

pub struct Harness {
    pub server: MockServer,
    pub list: DonatesList,
    pub creds: Arc<MemCreds>,
    pub provider: DonatelloProvider,
    pub stream: DonationStream,
    flip: bool,
}

impl Harness {
    pub async fn start(order: Order, initial: Vec<Value>) -> Self {
        let server = MockServer::start().await;
        let list = DonatesList::new(order, initial);
        mount_me(&server).await;
        mount_donates(&server, &list).await;
        Self::with_server(server, list, MemCreds::with_token(TOKEN))
    }

    pub fn with_server(server: MockServer, list: DonatesList, creds: Arc<MemCreds>) -> Self {
        let provider = DonatelloProvider::new(
            config(&server.uri()),
            creds.clone(),
            ScriptedLimiter::open(),
        )
        .unwrap();
        let stream = provider.donations();
        Self {
            server,
            list,
            creds,
            provider,
            stream,
            flip: false,
        }
    }

    pub fn empty_list(order: Order) -> DonatesList {
        DonatesList::new(order, Vec::new())
    }

    pub async fn next(&mut self) -> Result<Donation, PlatformError> {
        tokio::time::timeout(WAIT, self.stream.next())
            .await
            .expect("no item from the donation stream in time")
            .expect("donation stream ended")
    }

    pub async fn donations(&mut self, count: usize) -> Vec<Donation> {
        let mut donations = Vec::with_capacity(count);
        for _ in 0..count {
            donations.push(
                self.next()
                    .await
                    .expect("expected a donation, got an error"),
            );
        }
        donations
    }

    pub async fn stays_quiet(&mut self) -> bool {
        tokio::time::timeout(QUIET, self.stream.next())
            .await
            .is_err()
    }

    pub async fn wait_for(&self, what: &str, probe: impl Fn(&PollStatus) -> bool) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !probe(&self.provider.poll_status()) {
            assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    pub async fn poll_now(&mut self) {
        let before = self.provider.poll_status().last_poll_at;
        self.flip = !self.flip;
        let interval = if self.flip {
            MIN_POLL_INTERVAL + Duration::from_secs(1)
        } else {
            MIN_POLL_INTERVAL
        };
        self.provider.set_poll_interval(interval);
        self.wait_for("next completed poll", |status| {
            status.last_poll_at.is_some() && status.last_poll_at != before
        })
        .await;
    }

    pub async fn me_requests(&self) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/me")
            .count()
    }
}
