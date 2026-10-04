use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, KickEvidence, Verdict};
use crate::kick::{FakeKick, KickLedger, KickRequest};
use crate::scenario::{KickRequestSeen, RequestCount};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KickMark {
    pub(crate) requests: usize,
    pub(crate) sessions: usize,
}

impl KickMark {
    pub(crate) fn of(ledger: &KickLedger) -> Self {
        Self {
            requests: ledger.requests.len(),
            sessions: ledger.sessions.len(),
        }
    }
}

pub(crate) fn request_matches(request: &KickRequest, expected: &KickRequestSeen) -> bool {
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

fn counted(request: &KickRequest, expected: &RequestCount) -> bool {
    request.path == expected.path
        && expected
            .method
            .as_deref()
            .is_none_or(|method| request.method == method)
}

pub(crate) async fn observe_kick_request(
    fake: &FakeKick,
    from: KickMark,
    deadline: Instant,
    expected: &KickRequestSeen,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("a Kick request", remaining, |ledger| {
            ledger
                .requests
                .iter()
                .skip(from.requests)
                .any(|request| request_matches(request, expected))
                .then_some(())
        })
        .await
        .is_ok();
    let ledger = fake.ledger();
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoKickRequest {
            observed: ledger.requests.len().saturating_sub(from.requests),
        })
    };
    let requests = ledger
        .requests
        .iter()
        .skip(from.requests)
        .filter(|request| request.path == expected.path);
    (verdict, evidence(&ledger, from, requests))
}

pub(crate) fn assess_kick_request_count(
    ledger: &KickLedger,
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
    let requests = ledger
        .requests
        .iter()
        .filter(|request| counted(request, expected));
    (verdict, evidence(ledger, KickMark::default(), requests))
}

pub(crate) fn assess_no_unexpected_kick_requests(ledger: &KickLedger) -> (Verdict, Evidence) {
    let count = ledger.unexpected_requests().count();
    let verdict = if count == 0 {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::UnexpectedKickRequests { count })
    };
    (
        verdict,
        evidence(ledger, KickMark::default(), ledger.unexpected_requests()),
    )
}

fn evidence<'a>(
    ledger: &KickLedger,
    from: KickMark,
    requests: impl Iterator<Item = &'a KickRequest>,
) -> Evidence {
    Evidence::Kick(KickEvidence {
        sessions: ledger
            .sessions
            .iter()
            .skip(from.sessions)
            .take(EVIDENCE_LIMIT)
            .cloned()
            .collect(),
        requests: requests.take(EVIDENCE_LIMIT).cloned().collect(),
    })
}
