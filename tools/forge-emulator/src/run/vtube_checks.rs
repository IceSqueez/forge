use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, VTubeEvidence, Verdict};
use crate::scenario::{VTubeAuthOutcome, VTubeRequestSeen};
use crate::vtube::{FakeVTube, TokenCheck, VTubeLedger, VTubeRequest, VTubeSession};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct VTubeMark {
    pub(crate) sessions: usize,
    pub(crate) requests: usize,
}

impl VTubeMark {
    pub(crate) fn of(ledger: &VTubeLedger) -> Self {
        Self {
            sessions: ledger.sessions.len(),
            requests: ledger.requests.len(),
        }
    }
}

pub(crate) fn request_matches(request: &VTubeRequest, expected: &VTubeRequestSeen) -> bool {
    request.message_type == expected.message_type
        && expected
            .succeeded
            .is_none_or(|succeeded| request.error_id.is_none() == succeeded)
        && expected
            .error_id
            .is_none_or(|error_id| request.error_id == Some(error_id))
        && expected
            .data
            .0
            .iter()
            .all(|(pointer, matcher)| matcher.matches(request.data.pointer(pointer)))
}

pub(crate) fn auth_matches(session: &VTubeSession, expected: &VTubeAuthOutcome) -> bool {
    let wanted = if expected.accepted {
        TokenCheck::Accepted
    } else {
        TokenCheck::Rejected
    };
    session.authentication == wanted
}

pub(crate) async fn observe_vtube_request(
    fake: &FakeVTube,
    from: VTubeMark,
    deadline: Instant,
    expected: &VTubeRequestSeen,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("a VTube Studio request", remaining, |ledger| {
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
    let observed = ledger.requests.len().saturating_sub(from.requests);
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoVTubeRequest { observed })
    };
    (
        verdict,
        evidence(&ledger, from, Some(&expected.message_type)),
    )
}

pub(crate) async fn observe_vtube_auth(
    fake: &FakeVTube,
    from: VTubeMark,
    deadline: Instant,
    expected: &VTubeAuthOutcome,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for(
            "a VTube Studio authentication outcome",
            remaining,
            |ledger| {
                ledger
                    .sessions
                    .iter()
                    .skip(from.sessions)
                    .any(|session| auth_matches(session, expected))
                    .then_some(())
            },
        )
        .await
        .is_ok();
    let ledger = fake.ledger();
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoVTubeAuth {
            sessions: ledger.sessions.len().saturating_sub(from.sessions),
        })
    };
    (verdict, evidence(&ledger, from, None))
}

fn evidence(ledger: &VTubeLedger, from: VTubeMark, message_type: Option<&str>) -> Evidence {
    let sessions = ledger
        .sessions
        .iter()
        .skip(from.sessions)
        .take(EVIDENCE_LIMIT)
        .cloned()
        .collect();
    let requests = ledger
        .requests
        .iter()
        .skip(from.requests)
        .filter(|request| message_type.is_none_or(|kind| request.message_type == kind))
        .take(EVIDENCE_LIMIT)
        .cloned()
        .collect();
    Evidence::VTube(VTubeEvidence { sessions, requests })
}
