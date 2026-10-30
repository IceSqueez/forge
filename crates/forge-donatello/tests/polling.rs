mod support;

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use forge_donatello::PollPhase;
use forge_platform_core::{BuiltinControl, DonationProvider, PlatformError};
use forge_types::{Donation, DonationOrigin};
use serde_json::json;
use support::{Harness, MemCreds, OTHER_TOKEN, Order, donate, donates};
use tokio_stream::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
async fn a_scan_cut_short_by_a_page_error_counts_what_it_found_so_the_retry_does_not_over_walk() {
    let mut h = Harness::start(Order::OldestFirst, donates(45)).await;
    h.donations(45).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    let baseline_poll = h.provider.poll_status().last_poll_at;
    for index in 0..16 {
        h.list.add(donate(
            &format!("NEW-{index:02}"),
            &format!("2026-07-02 10:{index:02}:00"),
        ));
    }
    h.list.forget_requests();
    h.list.fail_after(2, ResponseTemplate::new(503));

    h.wake_poller();
    let partial = h.next().await.unwrap();
    let failure = h.next().await;
    assert!(
        matches!(failure, Err(PlatformError::Http { status: 503, .. })),
        "{failure:?}"
    );
    assert_eq!(h.list.requested_pages(), vec![0, 3, 2]);
    h.list.forget_requests();
    h.wake_poller();
    let rest = h.donations(15).await;
    h.wait_for("completed retry", |s| s.last_poll_at != baseline_poll)
        .await;

    assert_eq!(partial.donation_id, "NEW-15");
    assert_eq!(rest.len(), 15);
    assert_eq!(h.list.requested_pages(), vec![0, 3, 2]);
}

async fn after_baseline(order: Order, existing: usize) -> Harness {
    let mut h = Harness::start(order, donates(existing)).await;
    h.donations(existing).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.list.forget_requests();
    h
}

fn add_new(h: &Harness, count: usize) {
    for index in 0..count {
        h.list.add(donate(
            &format!("NEW-{index:02}"),
            &format!("2026-07-02 10:{index:02}:00"),
        ));
    }
}

fn new_live_ids(count: usize) -> HashSet<(String, DonationOrigin)> {
    (0..count)
        .map(|index| (format!("NEW-{index:02}"), DonationOrigin::Live))
        .collect()
}

fn ids_and_origins(reported: &[Donation]) -> HashSet<(String, DonationOrigin)> {
    reported
        .iter()
        .map(|d| (d.donation_id.clone(), d.origin))
        .collect()
}

#[tokio::test]
async fn newest_first_list_walks_forward_until_every_new_donation_is_found() {
    let mut h = after_baseline(Order::NewestFirst, 45).await;

    add_new(&h, 25);
    h.poll_now().await;
    let reported = h.donations(25).await;

    assert_eq!(ids_and_origins(&reported), new_live_ids(25));
    assert_eq!(h.list.requested_pages(), vec![0, 1]);
}

#[tokio::test]
async fn a_newest_first_walk_cut_short_resumes_past_the_pages_it_already_reported() {
    let mut h = after_baseline(Order::NewestFirst, 45).await;
    let baseline_poll = h.provider.poll_status().last_poll_at;
    add_new(&h, 45);
    h.list.fail_after(2, ResponseTemplate::new(503));

    h.wake_poller();
    let mut reported = h.donations(40).await;
    let failure = h.next().await;
    assert!(
        matches!(failure, Err(PlatformError::Http { status: 503, .. })),
        "{failure:?}"
    );
    assert_eq!(h.list.requested_pages(), vec![0, 1, 2]);
    h.list.forget_requests();
    h.wake_poller();
    reported.extend(h.donations(5).await);
    h.wait_for("completed retry", |s| s.last_poll_at != baseline_poll)
        .await;

    assert_eq!(ids_and_origins(&reported), new_live_ids(45));
    assert_eq!(h.list.requested_pages(), vec![0, 1, 2]);
}

#[tokio::test]
async fn a_shrunken_list_with_an_arrival_still_reports_the_arrival() {
    for (order, walk) in [
        (Order::NewestFirst, vec![0]),
        (Order::OldestFirst, vec![0, 2]),
    ] {
        let mut h = after_baseline(order, 45).await;

        h.list.remove("D-010");
        h.list.remove("D-011");
        add_new(&h, 1);
        h.poll_now().await;
        let reported = h.next().await.unwrap();

        assert_eq!(
            (reported.donation_id.as_str(), reported.origin),
            ("NEW-00", DonationOrigin::Live),
            "{order:?}"
        );
        assert_eq!(h.list.requested_pages(), walk, "{order:?}");
    }
}

#[tokio::test]
async fn a_shrunken_list_without_an_arrival_stops_after_one_page_of_known_donations() {
    for (order, walk) in [
        (Order::NewestFirst, vec![0, 1]),
        (Order::OldestFirst, vec![0, 2]),
    ] {
        let mut h = after_baseline(order, 45).await;

        h.list.remove("D-010");
        h.poll_now().await;

        assert_eq!(h.list.requested_pages(), walk, "{order:?}");
        assert!(h.stays_quiet().await, "{order:?}");
    }
}

#[tokio::test]
async fn a_removal_and_an_arrival_with_an_unchanged_total_still_report_the_arrival() {
    for (order, walk) in [
        (Order::NewestFirst, vec![0]),
        (Order::OldestFirst, vec![0, 2]),
    ] {
        let mut h = after_baseline(order, 45).await;

        h.list.remove("D-010");
        add_new(&h, 1);
        h.poll_now().await;
        let reported = h.next().await.unwrap();

        assert_eq!(
            (reported.donation_id.as_str(), reported.origin),
            ("NEW-00", DonationOrigin::Live),
            "{order:?}"
        );
        assert_eq!(h.list.requested_pages(), walk, "{order:?}");
    }
}

#[tokio::test]
async fn a_walk_finding_nothing_on_an_unchanged_oldest_first_list_stops_after_one_page() {
    let mut h = after_baseline(Order::OldestFirst, 45).await;

    h.poll_now().await;

    assert_eq!(h.list.requested_pages(), vec![0, 2]);
    assert!(h.stays_quiet().await);
}

#[tokio::test]
async fn a_walk_expecting_arrivals_that_are_all_restored_donations_stops_after_one_page() {
    let existing = donates(45);
    let mut h = after_baseline(Order::OldestFirst, 45).await;
    h.list.remove("D-010");
    h.list.remove("D-011");
    h.poll_now().await;
    h.list.forget_requests();

    h.list.add(existing[10].clone());
    h.list.add(existing[11].clone());
    h.poll_now().await;

    assert_eq!(h.list.requested_pages(), vec![0, 2]);
    assert!(h.stays_quiet().await);
}

#[tokio::test]
async fn a_scan_after_a_shrink_counts_from_the_smaller_total() {
    let mut h = after_baseline(Order::NewestFirst, 45).await;
    h.list.remove("D-010");
    h.poll_now().await;
    h.list.forget_requests();

    h.poll_now().await;

    assert_eq!(h.list.requested_pages(), vec![0]);
}

#[tokio::test]
async fn a_donation_arriving_during_the_baseline_is_history_free_and_reported_live_once() {
    for (order, existing, arrives_after_page) in [
        (Order::OldestFirst, 45, 1),
        (Order::OldestFirst, 40, 0),
        (Order::NewestFirst, 45, 0),
        (Order::NewestFirst, 40, 0),
    ] {
        let arrival = (arrives_after_page, donate("NEW-00", "2026-07-02 10:00:00"));
        let mut h = Harness::start_with_arrival(order, donates(existing), arrival).await;

        let baseline = h.donations(existing).await;
        h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
            .await;
        h.poll_now().await;
        let live = h.next().await.unwrap();

        assert!(
            baseline
                .iter()
                .all(|d| d.origin == DonationOrigin::History && d.donation_id != "NEW-00"),
            "{order:?} {existing}"
        );
        assert_eq!(
            (live.donation_id.as_str(), live.origin),
            ("NEW-00", DonationOrigin::Live),
            "{order:?} {existing}"
        );
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
    let mut h = Harness::ticking_with_server(server, list, MemCreds::with_token(support::TOKEN));

    let failure = h.next().await;
    assert!(
        matches!(failure, Err(PlatformError::Http { status: 503, .. })),
        "{failure:?}"
    );
    assert_eq!(h.provider.poll_status().phase, PollPhase::Retrying);

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
        let mut h = Harness::ticking_with_server(server, list, creds);

        let failure = h.next().await;
        let expected_platform_error = match me_status {
            Some(401) => {
                matches!(failure, Err(PlatformError::ReauthRequired { ref platform }) if platform == "donatello")
            }
            _ => matches!(failure, Err(PlatformError::Auth { .. })),
        };
        assert!(expected_platform_error, "{me_status:?}: {failure:?}");
        assert_eq!(h.provider.poll_status().phase, phase, "{me_status:?}");

        assert!(
            h.stays_quiet_for_several_ticks().await,
            "{me_status:?} polled again"
        );
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
    let mut h = Harness::ticking(Order::NewestFirst, donates(1)).await;
    h.donations(1).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    assert!(h.provider.disconnect().await.is_ok());
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    h.list.forget_requests();

    tokio::time::sleep(support::SEVERAL_TICKS).await;

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

#[tokio::test]
async fn replacements_straddling_a_page_boundary_are_all_reported() {
    let mut h = after_baseline(Order::OldestFirst, 41).await;
    h.list.remove("D-010");
    h.list.remove("D-011");
    add_new(&h, 2);

    h.poll_now().await;
    h.poll_now().await;
    let reported = h.donations(2).await;

    assert_eq!(ids_and_origins(&reported), new_live_ids(2));
}

async fn wait_for_requests(h: &Harness, count: usize) {
    let deadline = tokio::time::Instant::now() + support::WAIT;
    while h.list.requested_pages().len() < count {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {count} page requests"
        );
        tokio::time::sleep(support::TICK).await;
    }
}

#[tokio::test]
async fn pausing_during_a_stalled_page_still_reports_what_the_scan_collected_and_resumes_the_rest()
{
    let mut h = after_baseline(Order::NewestFirst, 45).await;
    add_new(&h, 21);
    h.list.forget_requests();
    h.list
        .fail_after(1, ResponseTemplate::new(200).set_delay(support::WAIT * 10));

    h.wake_poller();
    wait_for_requests(&h, 2).await;
    assert!(h.provider.disconnect().await.is_ok());
    let collected = h.donations(20).await;
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    assert!(h.provider.reconnect().await.is_ok());
    let rest = h.donations(1).await;

    let reported: Vec<Donation> = collected.into_iter().chain(rest).collect();
    assert_eq!(ids_and_origins(&reported), new_live_ids(21));
}

#[tokio::test]
async fn a_second_donation_stream_retires_the_first_poller() {
    let mut h = after_baseline(Order::NewestFirst, 2).await;

    let mut replacement = h.provider.donations();

    assert!(
        tokio::time::timeout(support::WAIT, h.stream.next())
            .await
            .expect("the first stream must close")
            .is_none(),
        "the retired poller must drop its stream"
    );
    for _ in 0..2 {
        let baseline = tokio::time::timeout(support::WAIT, replacement.next())
            .await
            .expect("the replacement stream reports the baseline")
            .expect("the replacement stream stays open")
            .unwrap();
        assert_eq!(baseline.origin, DonationOrigin::History);
    }
}
