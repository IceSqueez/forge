#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use forge_monobank::{MonobankProvider, MonobankRateLimits, PollFailure, PollPhase};
use forge_platform_core::{BuiltinControl, DonationProvider, PlatformError, RateLimitOutcome};
use forge_types::DonationOrigin;
use serde_json::json;
use support::{
    Bank, CLIENT_INFO, Harness, JAR, MemCreds, OTHER_JAR, STATEMENT_PREFIX, ScriptedLimiter, TOKEN,
    after_long_idle, now_unix, top_up,
};
use wiremock::ResponseTemplate;

const HOUR: i64 = 3_600;
const OVERLAP: i64 = 300;

#[tokio::test]
async fn first_poll_reports_recent_top_ups_as_history_then_new_ones_live_exactly_once() {
    let bank = Bank::new(&[JAR]);
    bank.add(JAR, top_up("OLD", now_unix() - 600));
    let mut h = Harness::start(bank).await;
    let history = h.next().await.unwrap();
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    h.bank.add(JAR, top_up("NEW", now_unix()));
    h.poll_now().await;
    let live = h.next().await.unwrap();
    h.poll_now().await;

    assert_eq!(
        [
            (history.donation_id.as_str(), history.origin),
            (live.donation_id.as_str(), live.origin)
        ],
        [
            ("OLD", DonationOrigin::History),
            ("NEW", DonationOrigin::Live)
        ]
    );
    assert!(h.stays_quiet().await, "a seen top-up was reported again");
}

#[tokio::test]
async fn top_up_older_than_the_history_horizon_is_not_reported() {
    let bank = Bank::new(&[JAR]);
    bank.add(JAR, top_up("ANCIENT", now_unix() - 2 * HOUR));
    bank.add(JAR, top_up("RECENT", now_unix() - 60));
    let mut h = Harness::start(bank).await;

    let first = h.next().await.unwrap();
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert_eq!(first.donation_id, "RECENT");
    assert!(h.stays_quiet().await);
}

#[tokio::test]
async fn polls_ask_for_the_selected_jar_with_overlapping_windows() {
    let mut h = Harness::start(Bank::new(&[JAR])).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.poll_now().await;

    let calls = h.bank.statement_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.iter().all(|call| call.jar == JAR), "{calls:?}");
    assert_eq!(calls[0].to - calls[0].from, HOUR, "{calls:?}");
    assert_eq!(calls[1].from, calls[0].to - OVERLAP, "{calls:?}");
}

#[tokio::test]
async fn steady_polling_reads_the_jar_list_only_once() {
    let mut h = Harness::start(Bank::new(&[JAR])).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.poll_now().await;
    h.poll_now().await;

    assert_eq!(h.bank.client_info_calls(), 1);
}

#[tokio::test]
async fn full_page_is_backfilled_until_every_top_up_in_the_window_is_reported() {
    let bank = Bank::new(&[JAR]);
    let newest = now_unix() - 5;
    let ids: BTreeSet<String> = (0..620).map(|index| format!("TX-{index:03}")).collect();
    for (index, id) in ids.iter().enumerate() {
        bank.add(JAR, top_up(id, newest - 5 * i64::try_from(index).unwrap()));
    }
    let mut h = Harness::start(bank).await;

    let mut reported = h.donations(500).await;
    h.wait_for("first page", |s| s.last_poll_at.is_some()).await;
    after_long_idle().await;
    reported.extend(h.donations(120).await);

    let calls = h.bank.statement_calls();
    assert_eq!(
        reported
            .iter()
            .map(|d| d.donation_id.clone())
            .collect::<BTreeSet<_>>(),
        ids
    );
    assert!(reported.iter().all(|d| d.origin == DonationOrigin::History));
    assert!(calls[1].to < calls[0].to, "{calls:?}");
    assert_eq!(calls[1].from, calls[0].from, "{calls:?}");
}

#[tokio::test]
async fn outgoing_and_zero_amount_transactions_are_skipped() {
    let bank = Bank::new(&[JAR]);
    let mut outgoing = top_up("OUT", now_unix() - 30);
    outgoing["amount"] = json!(-50_000);
    let mut zero = top_up("ZERO", now_unix() - 20);
    zero["amount"] = json!(0);
    bank.add(JAR, outgoing);
    bank.add(JAR, zero);
    bank.add(JAR, top_up("IN", now_unix() - 10));
    let mut h = Harness::start(bank).await;

    let only = h.next().await.unwrap();
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert_eq!(only.donation_id, "IN");
    assert!(h.stays_quiet().await);
}

#[tokio::test]
async fn unreadable_top_ups_are_reported_once_without_blocking_the_rest() {
    let bank = Bank::new(&[JAR]);
    let mut foreign = top_up("RUB", now_unix() - 30);
    foreign["currencyCode"] = json!(643);
    let mut broken = top_up("BROKEN", now_unix() - 20);
    broken["amount"] = json!("100");
    let mut no_id = top_up("ignored", now_unix() - 15);
    no_id.as_object_mut().unwrap().remove("id");
    bank.add(JAR, foreign);
    bank.add(JAR, broken);
    bank.add(JAR, no_id);
    bank.add(JAR, top_up("GOOD", now_unix() - 10));
    let mut h = Harness::start(bank).await;

    let good = h.next().await.unwrap();
    let rejected = [h.next().await, h.next().await];
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    h.poll_now().await;

    assert_eq!(good.donation_id, "GOOD");
    assert!(
        rejected
            .iter()
            .all(|r| matches!(r, Err(PlatformError::MalformedResponse { .. }))),
        "{rejected:?}"
    );
    assert!(
        h.stays_quiet().await,
        "a rejected top-up was reported again"
    );
}

#[tokio::test]
async fn server_error_is_reported_and_polling_recovers_on_the_next_timer() {
    let bank = Bank::new(&[JAR]);
    bank.fail_next(STATEMENT_PREFIX, ResponseTemplate::new(503));
    bank.add(JAR, top_up("D-1", now_unix() - 10));
    let mut h = Harness::start(bank).await;

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
async fn statement_rate_limit_is_reported_and_cools_only_the_statement_endpoint() {
    let bank = Bank::new(&[JAR]);
    bank.fail_next(
        STATEMENT_PREFIX,
        ResponseTemplate::new(429).insert_header("Retry-After", "90"),
    );
    let mut h = Harness::start(bank).await;

    let failure = h.next().await;

    assert!(
        matches!(
            failure,
            Err(PlatformError::RateLimited {
                retry_after_secs: 90
            })
        ),
        "{failure:?}"
    );
    assert_eq!(
        (
            h.limits.statement.throttles(),
            h.limits.client_info.throttles()
        ),
        (vec![Duration::from_secs(90)], vec![])
    );
}

#[tokio::test]
async fn local_cooldown_waits_silently_without_calling_the_bank() {
    let bank = Bank::new(&[JAR]);
    let server = wiremock::MockServer::start().await;
    support::mount(&server, &bank).await;
    let limits = MonobankRateLimits {
        client_info: ScriptedLimiter::open(),
        statement: ScriptedLimiter::answering(RateLimitOutcome::Throttled {
            wait_for: Duration::from_secs(30),
        }),
    };
    let provider = MonobankProvider::new(
        support::config(&server.uri()),
        MemCreds::with(TOKEN, Some(JAR)),
        limits,
    )
    .unwrap();
    let mut stream = provider.donations();

    let quiet = tokio::time::timeout(support::QUIET, tokio_stream::StreamExt::next(&mut stream))
        .await
        .is_err();

    assert!(quiet, "a local cooldown surfaced as a stream item");
    assert!(bank.statement_calls().is_empty());
    assert_eq!(provider.poll_status().last_error, None);
}

#[tokio::test]
async fn errors_needing_the_user_stop_polling_until_the_user_acts() {
    struct Case {
        name: &'static str,
        creds: Arc<MemCreds>,
        jars: &'static [&'static str],
        reject_token: bool,
        failure: PollFailure,
        phase: PollPhase,
    }
    let cases = [
        Case {
            name: "no token",
            creds: Arc::default(),
            jars: &[JAR],
            reject_token: false,
            failure: PollFailure::MissingToken,
            phase: PollPhase::AwaitingToken,
        },
        Case {
            name: "no jar",
            creds: MemCreds::with(TOKEN, None),
            jars: &[JAR],
            reject_token: false,
            failure: PollFailure::JarNotSelected,
            phase: PollPhase::ActionRequired,
        },
        Case {
            name: "jar gone",
            creds: MemCreds::with(TOKEN, Some(OTHER_JAR)),
            jars: &[JAR],
            reject_token: false,
            failure: PollFailure::JarMissing,
            phase: PollPhase::ActionRequired,
        },
        Case {
            name: "token rejected",
            creds: MemCreds::with(TOKEN, Some(JAR)),
            jars: &[JAR],
            reject_token: true,
            failure: PollFailure::TokenRejected,
            phase: PollPhase::ActionRequired,
        },
    ];
    for case in cases {
        let bank = Bank::new(case.jars);
        if case.reject_token {
            bank.fail_next(CLIENT_INFO, ResponseTemplate::new(403));
        }
        let mut h = Harness::with_creds(bank, case.creds).await;

        let failure = h.next().await;
        let expected_platform_error = if case.reject_token {
            matches!(failure, Err(PlatformError::ReauthRequired { ref platform }) if platform == "monobank")
        } else {
            matches!(failure, Err(PlatformError::Auth { .. }))
        };
        assert!(expected_platform_error, "{}: {failure:?}", case.name);
        let status = h.provider.poll_status();
        assert_eq!(
            (status.phase, status.last_error),
            (case.phase, Some(case.failure)),
            "{}",
            case.name
        );

        let requests = h.bank.request_count();
        after_long_idle().await;
        assert!(h.stays_quiet().await, "{} polled again", case.name);
        assert_eq!(h.bank.request_count(), requests, "{}", case.name);
    }
}

#[tokio::test]
async fn selecting_the_missing_jar_resumes_a_stopped_poller() {
    let bank = Bank::new(&[JAR]);
    bank.add(JAR, top_up("D-1", now_unix() - 10));
    let mut h = Harness::with_creds(bank, MemCreds::with(TOKEN, None)).await;
    let _missing_jar = h.next().await;

    h.provider.select_jar(JAR).await.unwrap();
    let donation = h.next().await.unwrap();

    assert_eq!(donation.donation_id, "D-1");
}

#[tokio::test]
async fn switching_to_another_jar_takes_a_fresh_history_baseline() {
    let bank = Bank::new(&[JAR, OTHER_JAR]);
    bank.add(JAR, top_up("FIRST-JAR", now_unix() - 10));
    bank.add(OTHER_JAR, top_up("SECOND-JAR", now_unix() - 10));
    let mut h = Harness::start(bank).await;
    h.next().await.unwrap();
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    h.provider.select_jar(OTHER_JAR).await.unwrap();
    let other = h.next().await.unwrap();

    assert_eq!(
        (other.donation_id.as_str(), other.origin),
        ("SECOND-JAR", DonationOrigin::History)
    );
}

#[tokio::test]
async fn pause_and_resume_keeps_the_seen_top_ups() {
    let bank = Bank::new(&[JAR]);
    bank.add(JAR, top_up("SEEN", now_unix() - 10));
    let mut h = Harness::start(bank).await;
    h.next().await.unwrap();
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    assert!(h.provider.disconnect().await.is_ok());
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    h.bank.add(JAR, top_up("NEW", now_unix()));
    assert!(h.provider.reconnect().await.is_ok());
    let next = h.next().await.unwrap();

    assert_eq!(
        (next.donation_id.as_str(), next.origin),
        ("NEW", DonationOrigin::Live)
    );
}

#[tokio::test]
async fn paused_poller_sends_no_requests() {
    let mut h = Harness::start(Bank::new(&[JAR])).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;
    assert!(h.provider.disconnect().await.is_ok());
    h.wait_for("paused", |s| s.phase == PollPhase::Paused).await;
    let requests = h.bank.request_count();

    after_long_idle().await;
    assert!(h.stays_quiet().await);

    assert_eq!(h.bank.request_count(), requests);
}

#[tokio::test]
async fn removing_the_token_moves_the_poller_to_awaiting_a_token() {
    let mut h = Harness::start(Bank::new(&[JAR])).await;
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
async fn status_carries_the_selected_jar_once_polling() {
    let h = Harness::start(Bank::new(&[JAR])).await;
    h.wait_for("baseline poll", |s| s.last_poll_at.is_some())
        .await;

    let jar = h.provider.poll_status().jar.unwrap();

    assert_eq!(
        (jar.id.as_str(), jar.currency.map(|c| c.as_str().to_owned())),
        (JAR, Some("UAH".to_owned()))
    );
}
