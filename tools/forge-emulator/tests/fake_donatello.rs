#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_donatello::{DONATELLO_CREDENTIAL_ID, DonatelloConfig, DonatelloProvider};
use forge_emulator::donatello::{
    DonatesFault, DonatesOrder, FAKE_DONATELLO_TOKEN, FakeDonatello, FakeDonatelloConfig,
    FakeDonation, PROFILE_INCOMPLETE_MESSAGE, UNAUTHORIZED_MESSAGE,
};
use forge_emulator::scenario::DonatelloGift;
use forge_platform_core::{DonationProvider, DonationStream, PlatformEndpoints, PlatformError};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::{Donation, DonationOrigin};
use futures_util::StreamExt;
use serde_json::{Value, json};
use time::OffsetDateTime;

const WAIT: Duration = Duration::from_secs(5);

fn donation(index: usize) -> FakeDonation {
    FakeDonation::new(
        &format!("DN-{index:03}"),
        &format!("viewer {index}"),
        "50",
        &format!("2026-10-03 {:02}:{:02}:00", 10 + index / 60, index % 60),
    )
}

async fn fake_with(config: FakeDonatelloConfig) -> FakeDonatello {
    FakeDonatello::start(config).await.unwrap()
}

async fn get(fake: &FakeDonatello, path: &str, token: Option<&str>) -> reqwest::Response {
    let mut request = reqwest::Client::new().get(format!("{}{path}", fake.base_url()));
    if let Some(token) = token {
        request = request.header("X-Token", token);
    }
    request.send().await.unwrap()
}

async fn page(fake: &FakeDonatello, page: u64, size: u64) -> Value {
    get(
        fake,
        &format!("/donates?page={page}&size={size}"),
        Some(FAKE_DONATELLO_TOKEN),
    )
    .await
    .json()
    .await
    .unwrap()
}

fn ids(page: &Value) -> Vec<String> {
    page["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["pubId"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn me_answers_the_account_for_the_configured_token() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;

    let response = get(&fake, "/me", Some(FAKE_DONATELLO_TOKEN)).await;
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();

    assert_eq!(status, 200);
    assert_eq!(body["nickname"], "fake-streamer");
}

#[tokio::test]
async fn wrong_or_missing_token_is_refused_with_the_documented_body() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;

    for (path, token) in [
        ("/me", None),
        ("/me", Some("other-token")),
        ("/donates?page=0&size=20", Some("other-token")),
    ] {
        let response = get(&fake, path, token).await;
        let status = response.status().as_u16();
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            (status, body),
            (
                401,
                json!({ "success": false, "message": UNAUTHORIZED_MESSAGE })
            ),
            "{path} {token:?}"
        );
    }
}

#[tokio::test]
async fn incomplete_profile_answers_not_found_on_me() {
    let fake = fake_with(FakeDonatelloConfig {
        profile_complete: false,
        ..FakeDonatelloConfig::default()
    })
    .await;

    let response = get(&fake, "/me", Some(FAKE_DONATELLO_TOKEN)).await;
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();

    assert_eq!(
        (status, body["message"].as_str()),
        (404, Some(PROFILE_INCOMPLETE_MESSAGE))
    );
}

#[tokio::test]
async fn donates_pages_carry_the_list_metadata() {
    let fake = fake_with(FakeDonatelloConfig {
        donations: (0..45).map(donation).collect(),
        ..FakeDonatelloConfig::default()
    })
    .await;

    let first = page(&fake, 0, 20).await;
    let last = page(&fake, 2, 20).await;

    assert_eq!(
        [
            &first["pages"],
            &first["total"],
            &first["first"],
            &first["last"]
        ],
        [&json!(3), &json!(45), &json!(true), &json!(false)]
    );
    assert_eq!(
        [&last["first"], &last["last"], &json!(ids(&last).len())],
        [&json!(false), &json!(true), &json!(5)]
    );
}

#[tokio::test]
async fn donates_follow_the_configured_list_order() {
    for (order, first_id) in [
        (DonatesOrder::NewestFirst, "DN-044"),
        (DonatesOrder::OldestFirst, "DN-000"),
    ] {
        let fake = fake_with(FakeDonatelloConfig {
            order,
            donations: (0..45).map(donation).collect(),
            ..FakeDonatelloConfig::default()
        })
        .await;

        let listed = ids(&page(&fake, 0, 20).await);

        assert_eq!(
            listed.first().map(String::as_str),
            Some(first_id),
            "{order:?}"
        );
    }
}

#[tokio::test]
async fn injected_donation_appears_on_the_next_read() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;
    let before = page(&fake, 0, 20).await;

    fake.donate(donation(7).with_message("hello"));
    let after = page(&fake, 0, 20).await;

    assert_eq!(before["total"], 0);
    assert_eq!(after["content"][0]["message"], "hello");
}

#[tokio::test]
async fn each_queued_fault_answers_one_donates_request_then_the_list_returns() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;
    fake.fail_next_donates(DonatesFault::TooManyRequests {
        retry_after_secs: Some(9),
    });
    fake.fail_next_donates(DonatesFault::ServerError { status: 503 });
    fake.fail_next_donates(DonatesFault::MalformedJson);
    let path = "/donates?page=0&size=20";

    let limited = get(&fake, path, Some(FAKE_DONATELLO_TOKEN)).await;
    let failing = get(&fake, path, Some(FAKE_DONATELLO_TOKEN)).await;
    let malformed = get(&fake, path, Some(FAKE_DONATELLO_TOKEN)).await;
    let healthy = get(&fake, path, Some(FAKE_DONATELLO_TOKEN)).await;

    assert_eq!(limited.status().as_u16(), 429);
    assert_eq!(
        limited
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok()),
        Some("9")
    );
    assert_eq!(failing.status().as_u16(), 503);
    assert!(
        serde_json::from_slice::<Value>(&malformed.bytes().await.unwrap()).is_err(),
        "the malformed fault must not parse"
    );
    assert_eq!(healthy.status().as_u16(), 200);
}

#[tokio::test]
async fn non_numeric_paging_is_refused_and_recorded() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;

    let response = get(&fake, "/donates?page=x&size=20", Some(FAKE_DONATELLO_TOKEN)).await;

    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(
        fake.requests()
            .iter()
            .map(|r| (r.page, r.size, r.status))
            .collect::<Vec<_>>(),
        [(None, Some(20), 400)]
    );
}

#[tokio::test]
async fn blank_token_configuration_is_refused() {
    let started = FakeDonatello::start(FakeDonatelloConfig {
        token: " ".to_owned(),
        ..FakeDonatelloConfig::default()
    })
    .await;

    assert!(started.is_err());
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

fn endpoints_addressing(fake: &FakeDonatello) -> PlatformEndpoints {
    let (variable, value) = fake.endpoint_override();
    PlatformEndpoints::resolve(move |name| (name == variable).then(|| OsString::from(&value)))
        .unwrap()
}

async fn provider_against(fake: &FakeDonatello) -> (DonatelloProvider, DonationStream) {
    let creds = Arc::new(MemCreds::default());
    creds
        .store(
            &CredentialId::new(DONATELLO_CREDENTIAL_ID),
            &json!({ "token": FAKE_DONATELLO_TOKEN }).to_string(),
        )
        .await
        .unwrap();
    let provider = DonatelloProvider::new(
        DonatelloConfig::new(&endpoints_addressing(fake)),
        creds,
        forge_donatello::default_rate_limiter(),
    )
    .unwrap();
    let stream = provider.donations();
    (provider, stream)
}

async fn next(stream: &mut DonationStream) -> Result<Donation, PlatformError> {
    tokio::time::timeout(WAIT, stream.next())
        .await
        .expect("no donation in time")
        .expect("stream ended")
}

#[tokio::test]
async fn endpoint_override_points_the_real_provider_at_the_fake() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;

    let config = DonatelloConfig::new(&endpoints_addressing(&fake));

    assert_eq!(config.base_url, fake.base_url());
}

#[tokio::test]
async fn real_provider_reads_history_from_the_fake_then_a_live_donation() {
    let fake = fake_with(FakeDonatelloConfig {
        donations: vec![donation(0)],
        ..FakeDonatelloConfig::default()
    })
    .await;
    let (provider, mut stream) = provider_against(&fake).await;
    let history = next(&mut stream).await.unwrap();

    fake.donate(donation(1).with_message("gl hf"));
    provider.set_poll_interval(Duration::from_secs(11));
    let live = next(&mut stream).await.unwrap();

    assert_eq!(
        (history.donation_id.as_str(), history.origin),
        ("DN-000", DonationOrigin::History)
    );
    assert_eq!(
        (
            live.donation_id.as_str(),
            live.origin,
            live.message.as_deref()
        ),
        ("DN-001", DonationOrigin::Live, Some("gl hf"))
    );
}

#[tokio::test]
async fn real_provider_surfaces_the_fake_rate_limit() {
    let fake = fake_with(FakeDonatelloConfig::default()).await;
    fake.fail_next_donates(DonatesFault::TooManyRequests {
        retry_after_secs: Some(4),
    });

    let (_provider, mut stream) = provider_against(&fake).await;
    let failure = next(&mut stream).await;

    assert!(
        matches!(
            failure,
            Err(PlatformError::RateLimited {
                retry_after_secs: 4
            })
        ),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_scenario_gift_reaches_forge_at_the_instant_it_was_made_in_summer_and_winter() {
    for now in [
        time::macros::datetime!(2026-07-01 12:00:00 UTC),
        time::macros::datetime!(2026-12-01 12:00:00 UTC),
    ] {
        let gift = DonatelloGift {
            id: "gift".to_owned(),
            donor: None,
            amount: "10".to_owned(),
            message: None,
            minutes_ago: 90,
        };
        let fake = fake_with(FakeDonatelloConfig {
            donations: vec![gift.to_fake(now).unwrap()],
            ..FakeDonatelloConfig::default()
        })
        .await;
        let (_provider, mut stream) = provider_against(&fake).await;

        let read = next(&mut stream).await.unwrap();

        assert_eq!(
            read.occurred_at.unix_timestamp(),
            (now - time::Duration::minutes(90)).unix_timestamp(),
            "now {now}"
        );
    }
}
