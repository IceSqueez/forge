#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_donatello::{
    DONATELLO_CREDENTIAL_ID, DonatelloConfig, DonatelloProvider, DonatelloTiming,
    MAX_POLL_INTERVAL, PollStatus, donatello_gateway,
};
use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
use forge_platform_core::{DonationProvider, DonationStream, PlatformError};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Donation;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub(crate) const TOKEN: &str = "dn-test-token-7f3a9c";
pub(crate) const OTHER_TOKEN: &str = "dn-other-token-41be02";
pub(crate) const WAIT: Duration = Duration::from_secs(30);
pub(crate) const TICK: Duration = Duration::from_millis(10);
pub(crate) const LATER_ARRIVAL_ID: &str = "LATER-ARRIVAL";
const LATER_ARRIVAL_CREATED_AT: &str = "2026-08-01 10:00:00";
const REFERENCE_POLLS: usize = 8;

#[derive(Default)]
pub(crate) struct MemCreds {
    entries: Mutex<HashMap<String, String>>,
}

impl MemCreds {
    pub(crate) fn with_token(token: &str) -> Arc<Self> {
        let creds = Self::default();
        creds.entries.lock().unwrap().insert(
            DONATELLO_CREDENTIAL_ID.to_owned(),
            json!({ "token": token }).to_string(),
        );
        Arc::new(creds)
    }

    pub(crate) fn stored(&self) -> Option<String> {
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

pub(crate) struct ScriptedLimiter {
    exhausted: bool,
    first_wait: Mutex<Option<Duration>>,
    acquires: Mutex<usize>,
    throttles: Mutex<Vec<Duration>>,
}

impl ScriptedLimiter {
    fn build(exhausted: bool, first_wait: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            exhausted,
            first_wait: Mutex::new(first_wait),
            acquires: Mutex::new(0),
            throttles: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn open() -> Arc<Self> {
        Self::build(false, None)
    }

    pub(crate) fn exhausted() -> Arc<Self> {
        Self::build(true, None)
    }

    pub(crate) fn throttled_once(wait: Duration) -> Arc<Self> {
        Self::build(false, Some(wait))
    }

    pub(crate) fn acquires(&self) -> usize {
        *self.acquires.lock().unwrap()
    }

    pub(crate) fn throttles(&self) -> Vec<Duration> {
        self.throttles.lock().unwrap().clone()
    }
}

#[async_trait]
impl RateLimiter for ScriptedLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        *self.acquires.lock().unwrap() += 1;
        if let Some(wait_for) = self.first_wait.lock().unwrap().take() {
            return Ok(RateLimitOutcome::Throttled { wait_for });
        }
        Ok(if self.exhausted {
            RateLimitOutcome::Exhausted
        } else {
            RateLimitOutcome::Granted
        })
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.throttles.lock().unwrap().push(retry_after);
    }
}

pub(crate) fn config(base_url: &str) -> DonatelloConfig {
    DonatelloConfig {
        base_url: base_url.to_owned(),
        poll_interval: MAX_POLL_INTERVAL,
    }
}

pub(crate) fn provider_on(
    base_url: &str,
    creds: Arc<dyn CredentialsRepo>,
    limiter: Arc<dyn RateLimiter>,
) -> DonatelloProvider {
    let config = config(base_url);
    let gateway = donatello_gateway(&config, limiter).unwrap();
    DonatelloProvider::new(config, creds, gateway)
}

fn ticking_timing() -> DonatelloTiming {
    DonatelloTiming {
        min_poll_interval: TICK,
        error_backoff_base: TICK,
        error_backoff_cap: TICK,
    }
}

fn ticking_provider_on(
    base_url: &str,
    creds: Arc<dyn CredentialsRepo>,
    limiter: Arc<dyn RateLimiter>,
) -> DonatelloProvider {
    let config = DonatelloConfig {
        poll_interval: TICK,
        ..config(base_url)
    };
    let gateway = donatello_gateway(&config, limiter).unwrap();
    DonatelloProvider::new(config, creds, gateway).with_timing(ticking_timing())
}

pub(crate) fn donate(id: &str, created_at: &str) -> Value {
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

pub(crate) fn donates(count: usize) -> Vec<Value> {
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
pub(crate) enum Order {
    NewestFirst,
    OldestFirst,
}

struct ListState {
    oldest_first: Vec<Value>,
    order: Order,
    requested_pages: Vec<u64>,
    faults: VecDeque<ResponseTemplate>,
    delayed_fault: Option<(usize, ResponseTemplate)>,
    arrival_after_page: Option<(u64, Value)>,
}

#[derive(Clone)]
pub(crate) struct DonatesList {
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
                delayed_fault: None,
                arrival_after_page: None,
            })),
        }
    }

    pub(crate) fn add(&self, item: Value) {
        self.state.lock().unwrap().oldest_first.push(item);
    }

    pub(crate) fn add_after_page(&self, page: u64, item: Value) {
        self.state.lock().unwrap().arrival_after_page = Some((page, item));
    }

    pub(crate) fn remove(&self, id: &str) {
        self.state
            .lock()
            .unwrap()
            .oldest_first
            .retain(|item| item["pubId"] != id);
    }

    pub(crate) fn fail_next(&self, response: ResponseTemplate) {
        self.state.lock().unwrap().faults.push_back(response);
    }

    pub(crate) fn fail_after(&self, passing_requests: usize, response: ResponseTemplate) {
        self.state.lock().unwrap().delayed_fault = Some((passing_requests, response));
    }

    pub(crate) fn requested_pages(&self) -> Vec<u64> {
        self.state.lock().unwrap().requested_pages.clone()
    }

    pub(crate) fn forget_requests(&self) {
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
        if let Some((passing, _)) = state.delayed_fault.as_mut() {
            if *passing == 0 {
                if let Some((_, fault)) = state.delayed_fault.take() {
                    return fault;
                }
            } else {
                *passing -= 1;
            }
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
        if state
            .arrival_after_page
            .as_ref()
            .is_some_and(|(after, _)| *after == page)
            && let Some((_, item)) = state.arrival_after_page.take()
        {
            state.oldest_first.push(item);
        }
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

pub(crate) async fn mount_me(server: &MockServer) {
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

pub(crate) async fn mount_donates(server: &MockServer, list: &DonatesList) {
    Mock::given(method("GET"))
        .and(path("/donates"))
        .respond_with(list.clone())
        .mount(server)
        .await;
}

pub(crate) struct Harness {
    pub server: MockServer,
    pub list: DonatesList,
    pub creds: Arc<MemCreds>,
    pub provider: DonatelloProvider,
    pub stream: DonationStream,
    flip: bool,
}

impl Harness {
    pub(crate) async fn start(order: Order, initial: Vec<Value>) -> Self {
        let server = MockServer::start().await;
        let list = DonatesList::new(order, initial);
        mount_me(&server).await;
        mount_donates(&server, &list).await;
        Self::with_server(server, list, MemCreds::with_token(TOKEN))
    }

    pub(crate) async fn start_with_arrival(
        order: Order,
        initial: Vec<Value>,
        (after_page, item): (u64, Value),
    ) -> Self {
        let server = MockServer::start().await;
        let list = DonatesList::new(order, initial);
        list.add_after_page(after_page, item);
        mount_me(&server).await;
        mount_donates(&server, &list).await;
        Self::with_server(server, list, MemCreds::with_token(TOKEN))
    }

    pub(crate) async fn ticking(order: Order, initial: Vec<Value>) -> Self {
        let server = MockServer::start().await;
        let list = DonatesList::new(order, initial);
        mount_me(&server).await;
        mount_donates(&server, &list).await;
        Self::ticking_with_server(server, list, MemCreds::with_token(TOKEN))
    }

    pub(crate) fn with_server(server: MockServer, list: DonatesList, creds: Arc<MemCreds>) -> Self {
        let provider = provider_on(&server.uri(), creds.clone(), ScriptedLimiter::open());
        Self::around(server, list, creds, provider)
    }

    pub(crate) fn ticking_with_server(
        server: MockServer,
        list: DonatesList,
        creds: Arc<MemCreds>,
    ) -> Self {
        let provider = ticking_provider_on(&server.uri(), creds.clone(), ScriptedLimiter::open());
        Self::around(server, list, creds, provider)
    }

    fn around(
        server: MockServer,
        list: DonatesList,
        creds: Arc<MemCreds>,
        provider: DonatelloProvider,
    ) -> Self {
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

    pub(crate) fn empty_list(order: Order) -> DonatesList {
        DonatesList::new(order, Vec::new())
    }

    pub(crate) async fn next(&mut self) -> Result<Donation, PlatformError> {
        tokio::time::timeout(WAIT, self.stream.next())
            .await
            .expect("no item from the donation stream in time")
            .expect("donation stream ended")
    }

    pub(crate) async fn donations(&mut self, count: usize) -> Vec<Donation> {
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

    pub(crate) async fn assert_only_a_later_arrival_is_reported_next(&mut self) {
        self.list
            .add(donate(LATER_ARRIVAL_ID, LATER_ARRIVAL_CREATED_AT));
        self.poll_now().await;
        let next = self.next().await.expect("the later arrival");
        assert_eq!(next.donation_id, LATER_ARRIVAL_ID);
    }

    pub(crate) async fn wait_for_requests(&self, count: usize) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.list.requested_pages().len() < count {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {count} page requests"
            );
            tokio::time::sleep(TICK).await;
        }
    }

    pub(crate) async fn outlast_several_ticks() {
        let reference = Self::ticking(Order::NewestFirst, Vec::new()).await;
        reference.wait_for_requests(REFERENCE_POLLS).await;
    }

    pub(crate) async fn wait_for(&self, what: &str, probe: impl Fn(&PollStatus) -> bool) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !probe(&self.provider.poll_status()) {
            assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    pub(crate) fn wake_poller(&mut self) {
        self.flip = !self.flip;
        let interval = if self.flip {
            MAX_POLL_INTERVAL - Duration::from_secs(1)
        } else {
            MAX_POLL_INTERVAL
        };
        self.provider.set_poll_interval(interval);
    }

    pub(crate) async fn poll_now(&mut self) {
        let before = self.provider.poll_status().last_poll_at;
        self.wake_poller();
        self.wait_for("next completed poll", |status| {
            status.last_poll_at.is_some() && status.last_poll_at != before
        })
        .await;
    }

    pub(crate) async fn me_requests(&self) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/me")
            .count()
    }
}
