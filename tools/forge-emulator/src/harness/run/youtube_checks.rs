use tokio::time::Instant;

use super::http_checks::{count_verdict, counted, request_matches};
use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, Verdict, YouTubeEvidence};
use crate::fakes::youtube::{FakeYouTube, YouTubeLedger, YouTubeRequest};
use crate::harness::scenario::{HttpRequestSeen, RequestCount};

pub(crate) async fn observe_youtube_request(
    fake: &FakeYouTube,
    from: usize,
    deadline: Instant,
    expected: &HttpRequestSeen,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("a YouTube request", remaining, |ledger| {
            ledger
                .requests
                .iter()
                .skip(from)
                .any(|request| request_matches(request, expected))
                .then_some(())
        })
        .await
        .is_ok();
    let ledger = fake.ledger();
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoYouTubeRequest {
            observed: ledger.requests.len().saturating_sub(from),
        })
    };
    let requests = ledger
        .requests
        .iter()
        .skip(from)
        .filter(|request| request.path == expected.path);
    (verdict, evidence(requests))
}

pub(crate) fn assess_youtube_request_count(
    ledger: &YouTubeLedger,
    expected: &RequestCount,
) -> (Verdict, Evidence) {
    let observed = ledger
        .requests
        .iter()
        .filter(|request| counted(*request, expected))
        .count();
    let verdict = count_verdict(observed, expected);
    (
        verdict,
        evidence(
            ledger
                .requests
                .iter()
                .filter(|request| counted(*request, expected)),
        ),
    )
}

pub(crate) fn assess_no_unexpected_youtube_requests(ledger: &YouTubeLedger) -> (Verdict, Evidence) {
    let count = ledger.unexpected_requests().count();
    let verdict = if count == 0 {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::UnexpectedYouTubeRequests { count })
    };
    (verdict, evidence(ledger.unexpected_requests()))
}

fn evidence<'a>(requests: impl Iterator<Item = &'a YouTubeRequest>) -> Evidence {
    Evidence::YouTube(YouTubeEvidence {
        requests: requests.take(EVIDENCE_LIMIT).cloned().collect(),
    })
}
