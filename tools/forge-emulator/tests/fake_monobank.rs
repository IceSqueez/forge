#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_emulator::monobank::{
    FAKE_MONOBANK_TOKEN, FakeJar, FakeMonobank, FakeMonobankConfig, FakeTransaction,
    MAX_STATEMENT_ITEMS, MAX_STATEMENT_WINDOW_SECS, MonobankEndpoint, TOO_MANY_REQUESTS_MESSAGE,
    UNKNOWN_TOKEN_MESSAGE,
};
use forge_monobank::{MonobankConfig, MonobankError, MonobankProvider, MonobankRateLimits};
use forge_platform_core::{
    DonationProvider, DonationStream, PlatformEndpoints, PlatformError, TokenBucketRateLimiter,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::{Donation, DonationOrigin, Donor};
use futures_util::StreamExt;
use serde_json::{Value, json};
use time::OffsetDateTime;

const WAIT: Duration = Duration::from_secs(5);
const JAR: &str = "fakeJar001";
const OTHER_JAR: &str = "fakeJar002";
const T0: i64 = 1_790_000_000;

fn jars() -> Vec<FakeJar> {
    vec![
        FakeJar::new(JAR, "На стрім").with_goal(1_000_000),
        FakeJar::new(OTHER_JAR, "Інша банка"),
    ]
}

fn unlimited() -> FakeMonobankConfig {
    FakeMonobankConfig {
        jars: jars(),
        call_window: Duration::ZERO,
        ..FakeMonobankConfig::default()
    }
}

fn top_up(index: i64) -> (String, FakeTransaction) {
    (
        JAR.to_owned(),
        FakeTransaction::top_up(&format!("TX-{index:04}"), T0 + index, 100, "Олена"),
    )
}

async fn fake_with(config: FakeMonobankConfig) -> FakeMonobank {
    FakeMonobank::start(config).await.unwrap()
}

async fn get(fake: &FakeMonobank, path: &str, token: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(format!("{}{path}", fake.base_url()));
    if let Some(token) = token {
        request = request.header("X-Token", token);
    }
    request.send().await.unwrap()
}

async fn statement(fake: &FakeMonobank, jar: &str, from: i64, to: i64) -> (u16, Value) {
    let response = get(
        fake,
        &format!("/personal/statement/{jar}/{from}/{to}"),
        Some(FAKE_MONOBANK_TOKEN),
    )
    .await;
    let status = response.status().as_u16();
    (status, response.json().await.unwrap())
}

fn ids(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn client_info_lists_the_configured_jars() {
    let fake = fake_with(unlimited()).await;

    let response = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();

    assert_eq!(status, 200);
    assert_eq!(
        body["jars"][0],
        json!({
            "id": JAR,
            "sendId": format!("jar/{JAR}"),
            "title": "На стрім",
            "description": "",
            "currencyCode": 980,
            "balance": 0,
            "goal": 1_000_000,
        })
    );
}

#[tokio::test]
async fn wrong_or_missing_token_is_refused_with_forbidden() {
    let fake = fake_with(unlimited()).await;

    for (path, token) in [
        ("/personal/client-info", None),
        ("/personal/client-info", Some("other-token")),
        (
            &format!("/personal/statement/{JAR}/{T0}/{}", T0 + 60) as &str,
            Some("other-token"),
        ),
    ] {
        let response = get(&fake, path, token).await;
        let status = response.status().as_u16();
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            (status, body),
            (403, json!({ "errorDescription": UNKNOWN_TOKEN_MESSAGE })),
            "{path} {token:?}"
        );
    }
}

#[tokio::test]
async fn statement_lists_the_window_newest_first_with_inclusive_bounds() {
    let fake = fake_with(FakeMonobankConfig {
        transactions: (0..10).map(top_up).collect(),
        ..unlimited()
    })
    .await;

    let (status, items) = statement(&fake, JAR, T0 + 2, T0 + 5).await;

    assert_eq!(status, 200);
    assert_eq!(ids(&items), ["TX-0005", "TX-0004", "TX-0003", "TX-0002"]);
}

#[tokio::test]
async fn statement_returns_at_most_the_newest_page_limit() {
    let total = i64::try_from(MAX_STATEMENT_ITEMS).unwrap() + 20;
    let fake = fake_with(FakeMonobankConfig {
        transactions: (0..total).map(top_up).collect(),
        ..unlimited()
    })
    .await;

    let (_, items) = statement(&fake, JAR, T0, T0 + total).await;
    let listed = ids(&items);

    assert_eq!(listed.len(), MAX_STATEMENT_ITEMS);
    assert_eq!(listed.last().map(String::as_str), Some("TX-0020"));
}

#[tokio::test]
async fn statement_only_lists_the_requested_jar() {
    let mut transactions: Vec<_> = (0..3).map(top_up).collect();
    transactions.push((
        OTHER_JAR.to_owned(),
        FakeTransaction::top_up("ELSEWHERE", T0 + 1, 100, "Петро"),
    ));
    let fake = fake_with(FakeMonobankConfig {
        transactions,
        ..unlimited()
    })
    .await;

    let (_, items) = statement(&fake, OTHER_JAR, T0, T0 + 10).await;

    assert_eq!(ids(&items), ["ELSEWHERE"]);
}

#[tokio::test]
async fn ids_are_stable_across_reads() {
    let fake = fake_with(FakeMonobankConfig {
        transactions: (0..5).map(top_up).collect(),
        ..unlimited()
    })
    .await;

    let first = ids(&statement(&fake, JAR, T0, T0 + 10).await.1);
    let second = ids(&statement(&fake, JAR, T0, T0 + 10).await.1);

    assert_eq!(first, second);
}

#[tokio::test]
async fn statement_refuses_unknown_jars_and_invalid_windows() {
    let fake = fake_with(unlimited()).await;

    for (jar, from, to) in [
        ("missing", T0, T0 + 60),
        (JAR, T0 + 60, T0),
        (JAR, T0, T0 + MAX_STATEMENT_WINDOW_SECS + 1),
    ] {
        let (status, body) = statement(&fake, jar, from, to).await;
        assert_eq!(status, 400, "{jar} {from} {to}");
        assert!(body["errorDescription"].is_string(), "{body}");
    }
}

#[tokio::test]
async fn longest_allowed_window_is_served() {
    let fake = fake_with(unlimited()).await;

    let (status, _) = statement(&fake, JAR, T0, T0 + MAX_STATEMENT_WINDOW_SECS).await;

    assert_eq!(status, 200);
}

#[tokio::test]
async fn transaction_variants_are_served_as_configured() {
    let fake = fake_with(FakeMonobankConfig {
        transactions: vec![
            (
                JAR.to_owned(),
                FakeTransaction::top_up("WITH", T0 + 3, 100, "Олена").with_comment("дякую"),
            ),
            (
                JAR.to_owned(),
                FakeTransaction::top_up("WITHOUT", T0 + 2, 100, "Олена"),
            ),
            (
                JAR.to_owned(),
                FakeTransaction::outgoing("OUT", T0 + 1, 500),
            ),
            (
                JAR.to_owned(),
                FakeTransaction::top_up("RUB", T0, 100, "Іван").with_currency(643),
            ),
        ],
        ..unlimited()
    })
    .await;

    let (_, items) = statement(&fake, JAR, T0, T0 + 10).await;

    assert_eq!(items[0]["comment"], "дякую");
    assert!(items[1].get("comment").is_none(), "{}", items[1]);
    assert_eq!(items[2]["amount"], -500);
    assert_eq!(items[3]["currencyCode"], 643);
}

#[tokio::test]
async fn each_endpoint_allows_one_call_per_window() {
    let fake = fake_with(FakeMonobankConfig {
        jars: jars(),
        ..FakeMonobankConfig::default()
    })
    .await;
    let statement_path = format!("/personal/statement/{JAR}/{T0}/{}", T0 + 60);

    let first = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;
    let other_endpoint = get(&fake, &statement_path, Some(FAKE_MONOBANK_TOKEN)).await;
    let again = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;
    let again_status = again.status().as_u16();
    let again_body: Value = again.json().await.unwrap();

    assert_eq!(
        (first.status().as_u16(), other_endpoint.status().as_u16()),
        (200, 200)
    );
    assert_eq!(
        (again_status, again_body),
        (
            429,
            json!({ "errorDescription": TOO_MANY_REQUESTS_MESSAGE })
        )
    );
}

#[tokio::test]
async fn endpoint_answers_again_once_its_window_has_passed() {
    let fake = fake_with(FakeMonobankConfig {
        jars: jars(),
        ..FakeMonobankConfig::default()
    })
    .await;
    let first = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;

    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::time::resume();
    let later = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;

    assert_eq!(
        (first.status().as_u16(), later.status().as_u16()),
        (200, 200)
    );
}

#[tokio::test]
async fn redirect_points_every_call_at_the_foreign_origin() {
    let fake = fake_with(unlimited()).await;
    fake.redirect_all_to(Some("http://203.0.113.7"));

    let response = get(&fake, "/personal/client-info", Some(FAKE_MONOBANK_TOKEN)).await;

    assert_eq!(response.status().as_u16(), 302);
    assert_eq!(
        response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("http://203.0.113.7/personal/client-info")
    );
}

#[tokio::test]
async fn invalid_configurations_are_refused() {
    for config in [
        FakeMonobankConfig {
            token: " ".to_owned(),
            ..unlimited()
        },
        FakeMonobankConfig {
            transactions: vec![(
                "unknown".to_owned(),
                FakeTransaction::top_up("TX", T0, 1, "x"),
            )],
            ..unlimited()
        },
    ] {
        assert!(FakeMonobank::start(config).await.is_err());
    }
}

#[derive(Default)]
struct MemCreds(Mutex<HashMap<String, String>>);

#[async_trait]
impl CredentialsRepo for MemCreds {
    async fn store(&self, id: &CredentialId, bundle: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), bundle.to_owned());
        Ok(())
    }
    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }
    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }
    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }
    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }
    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

fn endpoints_addressing(fake: &FakeMonobank) -> PlatformEndpoints {
    let (variable, value) = fake.endpoint_override();
    PlatformEndpoints::resolve(move |name| (name == variable).then(|| OsString::from(&value)))
        .unwrap()
}

fn generous_limits() -> MonobankRateLimits {
    let bucket = || Arc::new(TokenBucketRateLimiter::new(100, Duration::from_secs(1)));
    MonobankRateLimits {
        client_info: bucket(),
        statement: bucket(),
    }
}

fn provider_against(fake: &FakeMonobank, limits: MonobankRateLimits) -> MonobankProvider {
    MonobankProvider::new(
        MonobankConfig::new(&endpoints_addressing(fake)),
        Arc::new(MemCreds::default()),
        limits,
    )
    .unwrap()
}

async fn next(stream: &mut DonationStream) -> Result<Donation, PlatformError> {
    tokio::time::timeout(WAIT, stream.next())
        .await
        .expect("no donation in time")
        .expect("stream ended")
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[tokio::test]
async fn endpoint_override_points_the_real_provider_at_the_fake() {
    let fake = fake_with(unlimited()).await;

    let config = MonobankConfig::new(&endpoints_addressing(&fake));

    assert_eq!(config.base_url, fake.base_url());
}

#[tokio::test]
async fn real_provider_connects_within_the_banks_call_limit() {
    let fake = fake_with(FakeMonobankConfig {
        jars: jars(),
        ..FakeMonobankConfig::default()
    })
    .await;
    let provider = provider_against(&fake, MonobankRateLimits::official());

    let jars = provider.verify_token(FAKE_MONOBANK_TOKEN).await.unwrap();
    provider.save_token(FAKE_MONOBANK_TOKEN, JAR).await.unwrap();
    let mut stream = provider.donations();
    fake.wait_for("first statement", WAIT, |requests| {
        requests
            .iter()
            .any(|r| r.endpoint == Some(MonobankEndpoint::Statement))
            .then_some(())
    })
    .await
    .unwrap();

    assert_eq!(jars.len(), 2);
    assert!(
        fake.requests().iter().all(|r| r.status == 200),
        "{:?}",
        fake.requests()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), stream.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn real_provider_reads_history_from_the_fake_then_a_live_top_up() {
    let fake = fake_with(FakeMonobankConfig {
        jars: jars(),
        transactions: vec![(
            JAR.to_owned(),
            FakeTransaction::top_up("HIST", now_unix() - 600, 5_000, "Олена"),
        )],
        ..FakeMonobankConfig::default()
    })
    .await;
    let provider = provider_against(&fake, generous_limits());
    provider.save_token(FAKE_MONOBANK_TOKEN, JAR).await.unwrap();
    let mut stream = provider.donations();
    let history = next(&mut stream).await.unwrap();

    fake.top_up(
        JAR,
        FakeTransaction::top_up("LIVE", now_unix(), 2_550, "Петро").with_comment("gl hf"),
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    tokio::time::resume();
    let live = next(&mut stream).await.unwrap();

    assert_eq!(
        (history.donation_id.as_str(), history.origin),
        ("HIST", DonationOrigin::History)
    );
    assert_eq!(
        (
            live.donation_id.as_str(),
            live.origin,
            live.donor.clone(),
            live.message.as_deref(),
            live.amount.micros(),
        ),
        (
            "LIVE",
            DonationOrigin::Live,
            Donor::Named("Петро".to_owned()),
            Some("gl hf"),
            25_500_000,
        )
    );
}

#[tokio::test]
async fn real_provider_skips_outgoing_and_reports_unsupported_currency() {
    let fake = fake_with(FakeMonobankConfig {
        jars: jars(),
        transactions: vec![
            (
                JAR.to_owned(),
                FakeTransaction::outgoing("OUT", now_unix() - 30, 1_000),
            ),
            (
                JAR.to_owned(),
                FakeTransaction::top_up("RUB", now_unix() - 20, 100, "Іван").with_currency(643),
            ),
            (
                JAR.to_owned(),
                FakeTransaction::top_up("PLAIN", now_unix() - 10, 100, "Олена"),
            ),
        ],
        ..FakeMonobankConfig::default()
    })
    .await;
    let provider = provider_against(&fake, generous_limits());
    provider.save_token(FAKE_MONOBANK_TOKEN, JAR).await.unwrap();
    let mut stream = provider.donations();

    let plain = next(&mut stream).await.unwrap();
    let rejected = next(&mut stream).await;

    assert_eq!((plain.donation_id.as_str(), plain.message), ("PLAIN", None));
    assert!(
        matches!(rejected, Err(PlatformError::MalformedResponse { .. })),
        "{rejected:?}"
    );
}

#[tokio::test]
async fn real_provider_reports_a_token_the_fake_does_not_know_as_rejected() {
    let fake = fake_with(unlimited()).await;
    let provider = provider_against(&fake, generous_limits());

    let result = provider.verify_token("not-the-fake-token").await;

    assert!(
        matches!(result, Err(MonobankError::Unauthorized)),
        "{result:?}"
    );
}

#[tokio::test]
async fn real_provider_keeps_the_token_away_from_a_redirect_target() {
    let target = fake_with(unlimited()).await;
    let fake = fake_with(unlimited()).await;
    fake.redirect_all_to(Some(target.base_url()));
    let provider = provider_against(&fake, generous_limits());

    let result = provider.verify_token(FAKE_MONOBANK_TOKEN).await;

    assert!(
        matches!(result, Err(MonobankError::Http { status: 302 })),
        "{result:?}"
    );
    assert!(target.requests().is_empty(), "{:?}", target.requests());
}
