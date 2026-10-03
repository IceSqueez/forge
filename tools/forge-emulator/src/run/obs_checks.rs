use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, ObsEvidence, Verdict};
use crate::obs::{Authentication, FakeObs, ObsLedger, ObsRequest, ObsSession};
use crate::scenario::{ObsAuthOutcome, ObsRequestSeen};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObsMark {
    pub(crate) sessions: usize,
    pub(crate) requests: usize,
}

impl ObsMark {
    pub(crate) fn of(ledger: &ObsLedger) -> Self {
        Self {
            sessions: ledger.sessions.len(),
            requests: ledger.requests.len(),
        }
    }
}

pub(crate) fn request_matches(request: &ObsRequest, expected: &ObsRequestSeen) -> bool {
    request.request_type == expected.request_type
        && expected.code.is_none_or(|code| request.code == code)
        && expected
            .request_data
            .0
            .iter()
            .all(|(pointer, matcher)| matcher.matches(request.request_data.pointer(pointer)))
}

pub(crate) fn auth_matches(session: &ObsSession, expected: &ObsAuthOutcome) -> bool {
    if expected.accepted {
        session.identified
    } else {
        session.authentication == Authentication::Rejected
    }
}

pub(crate) async fn observe_obs_request(
    fake: &FakeObs,
    from: ObsMark,
    deadline: Instant,
    expected: &ObsRequestSeen,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("an OBS request", remaining, |ledger| {
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
        Verdict::Failed(FailureCause::NoObsRequest { observed })
    };
    (
        verdict,
        evidence(&ledger, from, Some(&expected.request_type)),
    )
}

pub(crate) async fn observe_obs_auth(
    fake: &FakeObs,
    from: ObsMark,
    deadline: Instant,
    expected: &ObsAuthOutcome,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let found = fake
        .wait_for("an OBS authentication outcome", remaining, |ledger| {
            ledger
                .sessions
                .iter()
                .skip(from.sessions)
                .any(|session| auth_matches(session, expected))
                .then_some(())
        })
        .await
        .is_ok();
    let ledger = fake.ledger();
    let verdict = if found {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoObsAuth {
            sessions: ledger.sessions.len().saturating_sub(from.sessions),
        })
    };
    (verdict, evidence(&ledger, from, None))
}

fn evidence(ledger: &ObsLedger, from: ObsMark, request_type: Option<&str>) -> Evidence {
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
        .filter(|request| request_type.is_none_or(|kind| request.request_type == kind))
        .take(EVIDENCE_LIMIT)
        .cloned()
        .collect();
    Evidence::Obs(ObsEvidence { sessions, requests })
}
