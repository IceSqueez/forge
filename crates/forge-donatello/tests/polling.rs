#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use forge_donatello::PollPhase;
use forge_platform_core::{BuiltinControl, PlatformError};
use forge_types::DonationOrigin;
use serde_json::json;
use support::{Harness, MemCreds, OTHER_TOKEN, Order, donate, donates};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn after_long_idle() {
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3600)).await;
    tokio::time::resume();
}

#[tokio::test]
async fn baseline_reports_every_existing_donation_as_history_across_all_pages() {
    let existing = donates(45);
    let mut h = Harness::start(Order::NewestFirst, existing.clone()).await;

    let reported = h.donations(45).await;

    let ids: BTreeSet<String> = reported.iter().map(|d| d.donation_id.clone()).collect();
    let expected: BTreeSet<String> = existing
        .iter()
        .map(|d| d["pubId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, expected);
    assert!(reported.iter().all(|d| d.origin == DonationOrigin::History));
    assert_eq!(h.list.requested_pages(), vec![0, 1, 2]);
}

#[tokio::test]
async fn donation_arriving_between_polls_is_reported_live_exactly_once() {
    let mut h = Harness::start(Order::NewestFirst, donates(3)).await;
    h.donations(3).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    h.list.add(donate("NEW-1", "2026-07-02 10:00:00"));
    h.poll_now().await;
    let first = h.next().await.unwrap();
    h.poll_now().await;
    h.list.add(donate("NEW-2", "2026-07-02 10:05:00"));
    h.poll_now().await;
    let second = h.next().await.unwrap();

    assert_eq!(
        [
            (first.donation_id.as_str(), first.origin),
            (second.donation_id.as_str(), second.origin)
        ],
        [
            ("NEW-1", DonationOrigin::Live),
            ("NEW-2", DonationOrigin::Live)
        ]
    );
}

#[tokio::test]
async fn new_donations_are_reported_in_chronological_order() {
    let mut h = Harness::start(Order::NewestFirst, donates(2)).await;
    h.donations(2).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    h.list.add(donate("EARLY", "2026-07-02 10:00:00"));
    h.list.add(donate("LATE", "2026-07-02 10:00:01"));
    h.poll_now().await;
    let ids: Vec<String> = h
        .donations(2)
        .await
        .into_iter()
        .map(|d| d.donation_id)
        .collect();

    assert_eq!(ids, ["EARLY", "LATE"]);
}

#[tokio::test]
async fn newest_first_list_finds_new_donations_on_the_first_page_alone() {
    let mut h = Harness::start(Order::NewestFirst, donates(45)).await;
    h.donations(45).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.list.forget_requests();

    h.list.add(donate("NEW-1", "2026-07-02 10:00:00"));
    h.poll_now().await;

    assert_eq!(h.next().await.unwrap().donation_id, "NEW-1");
    assert_eq!(h.list.requested_pages(), vec![0]);
}

#[tokio::test]
async fn oldest_first_list_walks_back_from_the_last_page_until_every_new_donation_is_found() {
    for (new_count, walk) in [(1, vec![0, 2]), (16, vec![0, 3, 2])] {
        let mut h = Harness::start(Order::OldestFirst, donates(45)).await;
        h.donations(45).await;
        h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
            .await;
        h.list.forget_requests();

        for index in 0..new_count {
            h.list.add(donate(
                &format!("NEW-{index:02}"),
                &format!("2026-07-02 10:{index:02}:00"),
            ));
        }
        h.poll_now().await;
        let reported = h.donations(new_count).await;

        assert!(
            reported
                .iter()
                .all(|d| d.origin == DonationOrigin::Live && d.donation_id.starts_with("NEW-")),
            "{new_count} new"
        );
        assert_eq!(h.list.requested_pages(), walk, "{new_count} new");
    }
}

#[tokio::test]
async fn unreadable_donations_are_reported_once_without_blocking_the_rest() {
    let mut bad_currency = donate("BAD-CURRENCY", "2026-07-01 10:00:00");
    bad_currency["currency"] = json!("UAHX");
    let mut bad_amount = donate("BAD-AMOUNT", "2026-07-01 10:01:00");
    bad_amount["amount"] = json!(true);
    let mut no_id = donate("ignored", "2026-07-01 10:02:00");
    no_id.as_object_mut().unwrap().remove("pubId");
    let initial = vec![
        bad_currency,
        donate("GOOD", "2026-07-01 10:03:00"),
        bad_amount,
        no_id,
    ];
    let mut h = Harness::start(Order::OldestFirst, initial).await;

    let good = h.next().await.unwrap();
    let rejected = [h.next().await, h.next().await];
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.list.add(donate("AFTER", "2026-07-02 10:00:00"));
    h.poll_now().await;
    let after = h.next().await.unwrap();

    assert_eq!(good.donation_id, "GOOD");
    assert!(
        rejected
            .iter()
            .all(|r| matches!(r, Err(PlatformError::MalformedResponse { .. }))),
        "{rejected:?}"
    );
    assert_eq!(after.donation_id, "AFTER");
}

#[tokio::test]
async fn malformed_page_is_reported_and_polling_retries() {
    let list = Harness::empty_list(Order::NewestFirst);
    list.fail_next(ResponseTemplate::new(200).set_body_string("{\"content\": [ not json"));
    list.add(donate("D-1", "2026-07-01 10:00:00"));
    let server = MockServer::start().await;
    support::mount_me(&server).await;
    support::mount_donates(&server, &list).await;
    let mut h = Harness::with_server(server, list, MemCreds::with_token(support::TOKEN));

    let failure = h.next().await;
    let phase = h.provider.poll_status().phase;

    assert!(
        matches!(failure, Err(PlatformError::MalformedResponse { .. })),
        "{failure:?}"
    );
    assert_eq!(phase, PollPhase::Retrying);
}

#[tokio::test]
async fn server_error_backs_off_then_recovers_on_the_next_timer() {
    let list = Harness::empty_list(Order::NewestFirst);
    list.fail_next(ResponseTemplate::new(503));
    list.add(donate("D-1", "2026-07-01 10:00:00"));
    let server = MockServer::start().await;
    support::mount_me(&server).await;
    support::mount_donates(&server, &list).await;
    let mut h = Harness::with_server(server, list, MemCreds::with_token(support::TOKEN));

    let failure = h.next().await;
    assert!(
        matches!(failure, Err(PlatformError::Http { status: 503, .. })),
        "{failure:?}"
    );
    assert_eq!(h.provider.poll_status().phase, PollPhase::Retrying);

    after_long_idle().await;
    let recovered = h.next().await.unwrap();
    h.wait_for("polling again", |s| s.phase == PollPhase::Polling)
        .await;
    assert_eq!(recovered.donation_id, "D-1");
}

#[tokio::test]
async fn errors_needing_the_user_stop_polling_until_the_user_acts() {
    let cases: [(Option<u16>, PollPhase); 3] = [
        (Some(401), PollPhase::ActionRequired),
        (Some(404), PollPhase::ActionRequired),
        (None, PollPhase::AwaitingToken),
    ];
    for (me_status, phase) in cases {
        let server = MockServer::start().await;
        let creds = match me_status {
            Some(status) => {
                Mock::given(method("GET"))
                    .and(path("/me"))
                    .respond_with(ResponseTemplate::new(status))
                    .mount(&server)
                    .await;
                MemCreds::with_token(support::TOKEN)
            }
            None => Arc::new(MemCreds::default()),
        };
        let list = Harness::empty_list(Order::NewestFirst);
        support::mount_donates(&server, &list).await;
        let mut h = Harness::with_server(server, list, creds);

        let failure = h.next().await;
        let expected_platform_error = match me_status {
            Some(401) => {
                matches!(failure, Err(PlatformError::ReauthRequired { ref platform }) if platform == "donatello")
            }
            _ => matches!(failure, Err(PlatformError::Auth { .. })),
        };
        assert!(expected_platform_error, "{me_status:?}: {failure:?}");
        assert_eq!(h.provider.poll_status().phase, phase, "{me_status:?}");

        after_long_idle().await;
        assert!(h.stays_quiet().await, "{me_status:?} polled again");
        assert_eq!(
            h.me_requests().await,
            usize::from(me_status.is_some()),
            "{me_status:?}"
        );
    }
}

#[tokio::test]
async fn pause_and_resume_keeps_the_seen_donations() {
    let mut h = Harness::start(Order::NewestFirst, donates(2)).await;
    h.donations(2).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert!(h.provider.disconnect().await.is_ok());
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    h.list.add(donate("NEW-1", "2026-07-02 10:00:00"));
    assert!(h.provider.reconnect().await.is_ok());
    let next = h.next().await.unwrap();

    assert_eq!(
        (next.donation_id.as_str(), next.origin),
        ("NEW-1", DonationOrigin::Live)
    );
}

#[tokio::test]
async fn paused_poller_sends_no_requests() {
    let mut h = Harness::start(Order::NewestFirst, donates(1)).await;
    h.donations(1).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    assert!(h.provider.disconnect().await.is_ok());
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    h.list.forget_requests();

    after_long_idle().await;
    tokio::time::sleep(support::QUIET).await;

    assert!(h.list.requested_pages().is_empty());
}

#[tokio::test]
async fn saving_a_different_token_takes_a_fresh_baseline() {
    let mut h = Harness::start(Order::NewestFirst, donates(1)).await;
    h.donations(1).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert!(h.provider.save_token(OTHER_TOKEN).await.is_ok());
    let again = h.next().await.unwrap();

    assert_eq!(
        (again.donation_id.as_str(), again.origin),
        ("D-000", DonationOrigin::History)
    );
}

#[tokio::test]
async fn removing_the_token_moves_the_poller_to_awaiting_a_token() {
    let mut h = Harness::start(Order::NewestFirst, donates(1)).await;
    h.donations(1).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert!(matches!(h.provider.remove_token().await, Ok(true)));
    let failure = h.next().await;

    assert!(
        matches!(failure, Err(PlatformError::Auth { .. })),
        "{failure:?}"
    );
    assert_eq!(h.provider.poll_status().phase, PollPhase::AwaitingToken);
}
