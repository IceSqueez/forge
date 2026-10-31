#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_monobank::{
    DEFAULT_HISTORY_LOOKBACK, DEFAULT_WINDOW_OVERLAP, MONOBANK_CREDENTIAL_ID, MonobankConfig,
    MonobankProvider, MonobankRateLimits, MonobankTiming, PollStatus, monobank_gateways,
};
use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
use forge_platform_core::{BuiltinControl, DonationProvider, DonationStream, PlatformError};
use forge_storage::CredentialsRepo;
use forge_types::Donation;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub(crate) const TOKEN: &str = "uMono-test-token-7f3a9c";
pub(crate) const OTHER_TOKEN: &str = "uMono-other-token-41be02";
pub(crate) const JAR: &str = "jarAAA111";
pub(crate) const OTHER_JAR: &str = "jarBBB222";
pub(crate) const UAH: i64 = 980;
pub(crate) const WAIT: Duration = Duration::from_secs(30);
pub(crate) const TICK: Duration = Duration::from_millis(10);
pub(crate) use forge_storage::test_support::MemoryCredentials;

pub(crate) fn creds_with(token: &str, jar: Option<&str>) -> Arc<MemoryCredentials> {
    Arc::new(MemoryCredentials::holding(
        MONOBANK_CREDENTIAL_ID,
        &json!({ "token": token, "jar_id": jar }).to_string(),
    ))
}

pub(crate) fn stored_bundle(creds: &MemoryCredentials) -> Option<Value> {
    creds
        .stored(MONOBANK_CREDENTIAL_ID)
        .map(|bundle| serde_json::from_str(&bundle).unwrap())
}

pub(crate) const LATER_ARRIVAL_ID: &str = "LATER-ARRIVAL";
const REFERENCE_POLLS: usize = 8;
pub(crate) const CLIENT_INFO: &str = "/personal/client-info";
pub(crate) const STATEMENT_PREFIX: &str = "/personal/statement/";

pub(crate) struct ScriptedLimiter {
    outcome: RateLimitOutcome,
    acquires: Mutex<usize>,
    throttles: Mutex<Vec<Duration>>,
}

impl ScriptedLimiter {
    pub(crate) fn answering(outcome: RateLimitOutcome) -> Arc<Self> {
        Arc::new(Self {
            outcome,
            acquires: Mutex::new(0),
            throttles: Mutex::new(Vec::new()),
        })
    }

    pub(crate) fn open() -> Arc<Self> {
        Self::answering(RateLimitOutcome::Granted)
    }

    pub(crate) fn throttles(&self) -> Vec<Duration> {
        self.throttles.lock().unwrap().clone()
    }

    pub(crate) async fn wait_for_acquires(&self, count: usize) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while *self.acquires.lock().unwrap() < count {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {count} budget checks"
            );
            tokio::time::sleep(TICK).await;
        }
    }
}

#[async_trait]
impl RateLimiter for ScriptedLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        *self.acquires.lock().unwrap() += 1;
        Ok(self.outcome)
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.throttles.lock().unwrap().push(retry_after);
    }
}

pub(crate) struct Limits {
    pub client_info: Arc<ScriptedLimiter>,
    pub statement: Arc<ScriptedLimiter>,
}

impl Limits {
    pub(crate) fn open() -> Self {
        Self {
            client_info: ScriptedLimiter::open(),
            statement: ScriptedLimiter::open(),
        }
    }

    pub(crate) fn shared(&self) -> MonobankRateLimits {
        MonobankRateLimits {
            client_info: self.client_info.clone(),
            statement: self.statement.clone(),
        }
    }
}

pub(crate) fn config(base_url: &str) -> MonobankConfig {
    MonobankConfig {
        base_url: base_url.to_owned(),
        history_lookback: DEFAULT_HISTORY_LOOKBACK,
        window_overlap: DEFAULT_WINDOW_OVERLAP,
    }
}

pub(crate) fn provider_on(
    base_url: &str,
    creds: Arc<dyn CredentialsRepo>,
    limits: MonobankRateLimits,
) -> MonobankProvider {
    let config = config(base_url);
    let gateways = monobank_gateways(&config, limits).unwrap();
    MonobankProvider::new(config, creds, gateways)
}

pub(crate) fn ticking_timing() -> MonobankTiming {
    MonobankTiming {
        statement_call_interval: TICK,
        error_backoff_base: TICK,
        error_backoff_cap: TICK,
    }
}

pub(crate) fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

pub(crate) fn jar(id: &str) -> Value {
    json!({
        "id": id,
        "sendId": format!("send-{id}"),
        "title": format!("title {id}"),
        "currencyCode": UAH,
        "balance": 0,
        "goal": 1_000_000,
    })
}

pub(crate) fn top_up(id: &str, time: i64) -> Value {
    json!({
        "id": id,
        "time": time,
        "description": format!("Від: donor {id}"),
        "mcc": 4829,
        "originalMcc": 4829,
        "amount": 10_000,
        "operationAmount": 10_000,
        "currencyCode": UAH,
        "commissionRate": 0,
        "cashbackAmount": 0,
        "balance": 10_000,
        "hold": false,
        "comment": format!("comment {id}"),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatementCall {
    pub jar: String,
    pub from: i64,
    pub to: i64,
}

struct BankState {
    jars: Vec<Value>,
    items: HashMap<String, Vec<Value>>,
    paths: Vec<String>,
    faults: VecDeque<(String, ResponseTemplate)>,
}

#[derive(Clone)]
pub(crate) struct Bank {
    state: Arc<Mutex<BankState>>,
}

impl Bank {
    pub(crate) fn new(jars: &[&str]) -> Self {
        Self {
            state: Arc::new(Mutex::new(BankState {
                jars: jars.iter().map(|id| jar(id)).collect(),
                items: HashMap::new(),
                paths: Vec::new(),
                faults: VecDeque::new(),
            })),
        }
    }

    pub(crate) fn add(&self, jar: &str, item: Value) {
        self.state
            .lock()
            .unwrap()
            .items
            .entry(jar.to_owned())
            .or_default()
            .push(item);
    }

    pub(crate) fn fail_next(&self, path_prefix: &str, response: ResponseTemplate) {
        self.state
            .lock()
            .unwrap()
            .faults
            .push_back((path_prefix.to_owned(), response));
    }

    pub(crate) fn client_info_calls(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .paths
            .iter()
            .filter(|p| p.as_str() == CLIENT_INFO)
            .count()
    }

    pub(crate) fn statement_calls(&self) -> Vec<StatementCall> {
        self.state
            .lock()
            .unwrap()
            .paths
            .iter()
            .filter_map(|p| p.strip_prefix(STATEMENT_PREFIX))
            .map(|rest| {
                let parts: Vec<&str> = rest.split('/').collect();
                StatementCall {
                    jar: parts[0].to_owned(),
                    from: parts[1].parse().unwrap(),
                    to: parts[2].parse().unwrap(),
                }
            })
            .collect()
    }

    pub(crate) fn request_count(&self) -> usize {
        self.state.lock().unwrap().paths.len()
    }
}

impl Respond for Bank {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let path = request.url.path().to_owned();
        let mut state = self.state.lock().unwrap();
        state.paths.push(path.clone());
        if state
            .faults
            .front()
            .is_some_and(|(prefix, _)| path.starts_with(prefix.as_str()))
        {
            return state.faults.pop_front().unwrap().1;
        }
        if request.headers.get("x-token").is_none_or(|v| {
            v.as_bytes() != TOKEN.as_bytes() && v.as_bytes() != OTHER_TOKEN.as_bytes()
        }) {
            return ResponseTemplate::new(reqwest::StatusCode::FORBIDDEN)
                .set_body_json(json!({ "errorDescription": "Unknown 'X-Token'" }));
        }
        if path == CLIENT_INFO {
            return ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({
                "clientId": "client",
                "name": "Streamer",
                "accounts": [],
                "jars": state.jars,
            }));
        }
        let Some(rest) = path.strip_prefix(STATEMENT_PREFIX) else {
            return ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND);
        };
        let parts: Vec<&str> = rest.split('/').collect();
        let (from, to): (i64, i64) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
        let mut listed: Vec<Value> = state
            .items
            .get(parts[0])
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|item| (from..=to).contains(&item["time"].as_i64().unwrap()))
            .collect();
        listed.sort_by_key(|item| std::cmp::Reverse(item["time"].as_i64().unwrap()));
        listed.truncate(500);
        ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(listed)
    }
}

pub(crate) async fn mount(server: &MockServer, bank: &Bank) {
    Mock::given(wiremock::matchers::any())
        .respond_with(bank.clone())
        .mount(server)
        .await;
}

pub(crate) struct Harness {
    pub server: MockServer,
    pub bank: Bank,
    pub creds: Arc<MemoryCredentials>,
    pub limits: Limits,
    pub provider: MonobankProvider,
    pub stream: DonationStream,
}

impl Harness {
    pub(crate) async fn start(bank: Bank) -> Self {
        Self::with_creds(bank, creds_with(TOKEN, Some(JAR))).await
    }

    pub(crate) async fn with_creds(bank: Bank, creds: Arc<MemoryCredentials>) -> Self {
        Self::with_timing(bank, creds, MonobankTiming::default()).await
    }

    pub(crate) async fn ticking(bank: Bank) -> Self {
        Self::ticking_with_creds(bank, creds_with(TOKEN, Some(JAR))).await
    }

    pub(crate) async fn ticking_with_creds(bank: Bank, creds: Arc<MemoryCredentials>) -> Self {
        Self::with_timing(bank, creds, ticking_timing()).await
    }

    async fn with_timing(
        bank: Bank,
        creds: Arc<MemoryCredentials>,
        timing: MonobankTiming,
    ) -> Self {
        let server = MockServer::start().await;
        mount(&server, &bank).await;
        let limits = Limits::open();
        let provider =
            provider_on(&server.uri(), creds.clone(), limits.shared()).with_timing(timing);
        let stream = provider.donations();
        Self {
            server,
            bank,
            creds,
            limits,
            provider,
            stream,
        }
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
        self.bank.add(JAR, top_up(LATER_ARRIVAL_ID, now_unix()));
        self.poll_now().await;
        let next = self.next().await.expect("the later arrival");
        assert_eq!(next.donation_id, LATER_ARRIVAL_ID);
    }

    pub(crate) async fn outlast_several_ticks() {
        let reference = Self::ticking(Bank::new(&[JAR])).await;
        reference.wait_for_statement_calls(REFERENCE_POLLS).await;
    }

    pub(crate) async fn wait_for(&self, what: &str, probe: impl Fn(&PollStatus) -> bool) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !probe(&self.provider.poll_status()) {
            assert!(tokio::time::Instant::now() < deadline, "timed out: {what}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    pub(crate) async fn wake_poller(&self) {
        assert!(self.provider.reconnect().await.is_ok());
    }

    pub(crate) async fn wait_for_statement_calls(&self, count: usize) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while self.bank.statement_calls().len() < count {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {count} statement calls"
            );
            tokio::time::sleep(TICK).await;
        }
    }

    pub(crate) async fn poll_now(&mut self) {
        let before = self.provider.poll_status().last_poll_at;
        self.wake_poller().await;
        self.wait_for("next completed poll", |status| {
            status.last_poll_at.is_some() && status.last_poll_at != before
        })
        .await;
    }
}
