use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, Verdict, YouTubeEvidence};
use crate::scenario::{RequestCount, YouTubeRequestSeen};
use crate::youtube::{FakeYouTube, YouTubeLedger, YouTubeRequest};

pub(crate) fn request_matches(request: &YouTubeRequest, expected: &YouTubeRequestSeen) -> bool {
    request.path == expected.path
        && expected
            .method
            .as_deref()
            .is_none_or(|method| request.method == method)
        && expected
            .status
            .is_none_or(|status| request.status == status)
        && expected.body.0.iter().all(|(pointer, matcher)| {
            matcher.matches(request.body.as_ref().and_then(|body| body.pointer(pointer)))
        })
}

fn counted(request: &YouTubeRequest, expected: &RequestCount) -> bool {
    request.path == expected.path
        && expected
            .method
            .as_deref()
            .is_none_or(|method| request.method == method)
}

pub(crate) async fn observe_youtube_request(
    fake: &FakeYouTube,
    from: usize,
    deadline: Instant,
    expected: &YouTubeRequestSeen,
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
        .filter(|request| counted(request, expected))
        .count();
    let below = expected.min.is_some_and(|min| observed < min as usize);
    let above = expected.max.is_some_and(|max| observed > max as usize);
    let verdict = if below || above {
        Verdict::Failed(FailureCause::RequestCountOutOfRange {
            observed,
            min: expected.min,
            max: expected.max,
        })
    } else {
        Verdict::Passed
    };
    (
        verdict,
        evidence(
            ledger
                .requests
                .iter()
                .filter(|request| counted(request, expected)),
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
