#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_monobank::{
    DEFAULT_HISTORY_LOOKBACK, DEFAULT_WINDOW_OVERLAP, MONOBANK_CREDENTIAL_ID, MonobankConfig,
    MonobankProvider, MonobankRateLimits, PollStatus,
};
use forge_platform_core::{
    DonationProvider, DonationStream, PlatformError, RateLimitOutcome, RateLimiter,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Donation;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio_stream::StreamExt;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub const TOKEN: &str = "uMono-test-token-7f3a9c";
pub const OTHER_TOKEN: &str = "uMono-other-token-41be02";
pub const JAR: &str = "jarAAA111";
pub const OTHER_JAR: &str = "jarBBB222";
pub const UAH: i64 = 980;
pub const WAIT: Duration = Duration::from_secs(5);
pub const QUIET: Duration = Duration::from_millis(50);
pub const CLIENT_INFO: &str = "/personal/client-info";
pub const STATEMENT_PREFIX: &str = "/personal/statement/";

#[derive(Default)]
pub struct MemCreds {
    entries: Mutex<HashMap<String, String>>,
}

impl MemCreds {
    pub fn with(token: &str, jar: Option<&str>) -> Arc<Self> {
        let creds = Self::default();
        creds.entries.lock().unwrap().insert(
            MONOBANK_CREDENTIAL_ID.to_owned(),
            json!({ "token": token, "jar_id": jar }).to_string(),
        );
        Arc::new(creds)
    }

    pub fn stored(&self) -> Option<Value> {
        self.entries
            .lock()
            .unwrap()
            .get(MONOBANK_CREDENTIAL_ID)
            .map(|bundle| serde_json::from_str(bundle).unwrap())
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
    outcome: RateLimitOutcome,
    throttles: Mutex<Vec<Duration>>,
}

impl ScriptedLimiter {
    pub fn answering(outcome: RateLimitOutcome) -> Arc<Self> {
        Arc::new(Self {
            outcome,
            throttles: Mutex::new(Vec::new()),
        })
    }

    pub fn open() -> Arc<Self> {
        Self::answering(RateLimitOutcome::Granted)
    }

    pub fn throttles(&self) -> Vec<Duration> {
        self.throttles.lock().unwrap().clone()
    }
}

#[async_trait]
impl RateLimiter for ScriptedLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(self.outcome)
    }

    fn remaining(&self) -> u32 {
        u32::from(self.outcome == RateLimitOutcome::Granted)
    }

    async fn observe_remote_throttle(&self, retry_after: Duration) {
        self.throttles.lock().unwrap().push(retry_after);
    }
}

pub struct Limits {
    pub client_info: Arc<ScriptedLimiter>,
    pub statement: Arc<ScriptedLimiter>,
}

impl Limits {
    pub fn open() -> Self {
        Self {
            client_info: ScriptedLimiter::open(),
            statement: ScriptedLimiter::open(),
        }
    }

    pub fn shared(&self) -> MonobankRateLimits {
        MonobankRateLimits {
            client_info: self.client_info.clone(),
            statement: self.statement.clone(),
        }
    }
}

pub fn config(base_url: &str) -> MonobankConfig {
    MonobankConfig {
        base_url: base_url.to_owned(),
        request_timeout: WAIT,
        history_lookback: DEFAULT_HISTORY_LOOKBACK,
        window_overlap: DEFAULT_WINDOW_OVERLAP,
    }
}

pub fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

pub fn jar(id: &str) -> Value {
    json!({
        "id": id,
        "sendId": format!("send-{id}"),
        "title": format!("title {id}"),
        "currencyCode": UAH,
        "balance": 0,
        "goal": 1_000_000,
    })
}

pub fn top_up(id: &str, time: i64) -> Value {
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
pub struct StatementCall {
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
pub struct Bank {
    state: Arc<Mutex<BankState>>,
}

impl Bank {
    pub fn new(jars: &[&str]) -> Self {
        Self {
            state: Arc::new(Mutex::new(BankState {
                jars: jars.iter().map(|id| jar(id)).collect(),
                items: HashMap::new(),
                paths: Vec::new(),
                faults: VecDeque::new(),
            })),
        }
    }

    pub fn add(&self, jar: &str, item: Value) {
        self.state
            .lock()
            .unwrap()
            .items
            .entry(jar.to_owned())
            .or_default()
            .push(item);
    }

    pub fn fail_next(&self, path_prefix: &str, response: ResponseTemplate) {
        self.state
            .lock()
            .unwrap()
            .faults
            .push_back((path_prefix.to_owned(), response));
    }

    pub fn client_info_calls(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .paths
            .iter()
            .filter(|p| p.as_str() == CLIENT_INFO)
            .count()
    }

    pub fn statement_calls(&self) -> Vec<StatementCall> {
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

    pub fn request_count(&self) -> usize {
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
            return ResponseTemplate::new(403)
                .set_body_json(json!({ "errorDescription": "Unknown 'X-Token'" }));
        }
        if path == CLIENT_INFO {
            return ResponseTemplate::new(200).set_body_json(json!({
                "clientId": "client",
                "name": "Streamer",
                "accounts": [],
                "jars": state.jars,
            }));
        }
        let Some(rest) = path.strip_prefix(STATEMENT_PREFIX) else {
            return ResponseTemplate::new(404);
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
        ResponseTemplate::new(200).set_body_json(listed)
    }
}

pub async fn mount(server: &MockServer, bank: &Bank) {
    Mock::given(wiremock::matchers::any())
        .respond_with(bank.clone())
        .mount(server)
        .await;
}

pub struct Harness {
    pub server: MockServer,
    pub bank: Bank,
    pub creds: Arc<MemCreds>,
    pub limits: Limits,
    pub provider: MonobankProvider,
    pub stream: DonationStream,
}

impl Harness {
    pub async fn start(bank: Bank) -> Self {
        Self::with_creds(bank, MemCreds::with(TOKEN, Some(JAR))).await
    }

    pub async fn with_creds(bank: Bank, creds: Arc<MemCreds>) -> Self {
        let server = MockServer::start().await;
        mount(&server, &bank).await;
        let limits = Limits::open();
        let provider =
            MonobankProvider::new(config(&server.uri()), creds.clone(), limits.shared()).unwrap();
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
        after_long_idle().await;
        self.wait_for("next completed poll", |status| {
            status.last_poll_at.is_some() && status.last_poll_at != before
        })
        .await;
    }
}

pub async fn after_long_idle() {
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3_600)).await;
    tokio::time::resume();
}
