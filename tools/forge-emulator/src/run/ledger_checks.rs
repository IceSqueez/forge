use tokio::time::Instant;

use super::outcome::{EVIDENCE_LIMIT, Evidence, FailureCause, LedgerExcerpt, Verdict};
use crate::scenario::{RequestCount, TwitchSubscription};
use crate::twitch::{FakeTwitch, Ledger, RecordedRequest};

const SUBSCRIPTION_PATH: &str = "/helix/eventsub/subscriptions";

pub(crate) fn holds_live_subscription(
    ledger: &Ledger,
    subscription_type: &str,
    version: Option<&str>,
) -> bool {
    ledger.subscriptions.iter().any(|subscription| {
        subscription.subscription_type == subscription_type
            && version.is_none_or(|version| subscription.version == version)
            && ledger
                .live_sessions()
                .any(|session| session.id == subscription.session_id)
    })
}

/// The distinct subscription types a live session holds, in first-seen order.
pub(crate) fn live_subscription_types(ledger: &Ledger) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for subscription in &ledger.subscriptions {
        let live = ledger
            .live_sessions()
            .any(|session| session.id == subscription.session_id);
        if live && !types.contains(&subscription.subscription_type) {
            types.push(subscription.subscription_type.clone());
        }
    }
    types
}

/// Sessions, subscriptions, and the subscription-creation requests that produced them.
pub(crate) fn subscription_excerpt(ledger: &Ledger) -> LedgerExcerpt {
    LedgerExcerpt {
        requests: capped(
            ledger
                .requests
                .iter()
                .filter(|request| request.path == SUBSCRIPTION_PATH),
        ),
        subscriptions: ledger.subscriptions.clone(),
        sessions: ledger.sessions.clone(),
    }
}

pub(crate) async fn observe_subscription(
    fake: &FakeTwitch,
    deadline: Instant,
    expected: &TwitchSubscription,
) -> (Verdict, Evidence) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let version = expected.version.as_deref();
    let held = fake
        .wait_for("subscription", remaining, |ledger| {
            holds_live_subscription(ledger, &expected.subscription_type, version).then_some(())
        })
        .await
        .is_ok();
    let verdict = if held {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::NoSubscription)
    };
    (
        verdict,
        Evidence::Ledger(subscription_excerpt(&fake.ledger())),
    )
}

/// Covers the whole run so far, not only the current step.
pub(crate) fn assess_no_unexpected_requests(ledger: &Ledger) -> (Verdict, Evidence) {
    let unexpected: Vec<&RecordedRequest> = ledger.unexpected_requests().collect();
    let verdict = if unexpected.is_empty() {
        Verdict::Passed
    } else {
        Verdict::Failed(FailureCause::UnexpectedRequests {
            count: unexpected.len(),
        })
    };
    let excerpt = LedgerExcerpt {
        requests: capped(unexpected.into_iter()),
        ..LedgerExcerpt::default()
    };
    (verdict, Evidence::Ledger(excerpt))
}

/// Counts over the whole run so far.
pub(crate) fn assess_request_count(
    ledger: &Ledger,
    expected: &RequestCount,
) -> (Verdict, Evidence) {
    let matching: Vec<&RecordedRequest> = ledger
        .requests
        .iter()
        .filter(|request| {
            request.path == expected.path
                && expected
                    .method
                    .as_deref()
                    .is_none_or(|method| request.method == method)
        })
        .collect();
    let observed = matching.len();
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
    let excerpt = LedgerExcerpt {
        requests: capped(matching.into_iter()),
        ..LedgerExcerpt::default()
    };
    (verdict, Evidence::Ledger(excerpt))
}

fn capped<'a>(requests: impl Iterator<Item = &'a RecordedRequest>) -> Vec<RecordedRequest> {
    requests.take(EVIDENCE_LIMIT).cloned().collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::twitch::{CredentialCheck, RecordedSession, RecordedSubscription};

    fn request(method: &str, path: &str, modeled: bool) -> RecordedRequest {
        RecordedRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            query: Vec::new(),
            body: None,
            credentials: CredentialCheck::Accepted,
            status: if modeled { 200 } else { 404 },
            response: Value::Null,
            modeled,
        }
    }

    fn session(id: &str, live: bool) -> RecordedSession {
        RecordedSession {
            id: id.to_owned(),
            reconnected_from: None,
            connected_at: "2026-09-13T10:00:00Z".to_owned(),
            live,
        }
    }

    fn subscription(session_id: &str, subscription_type: &str) -> RecordedSubscription {
        RecordedSubscription {
            id: format!("sub-{session_id}"),
            session_id: session_id.to_owned(),
            subscription_type: subscription_type.to_owned(),
            version: "1".to_owned(),
            condition: json!({}),
            created_at: "2026-09-13T10:00:00Z".to_owned(),
        }
    }

    fn count(spec: Value) -> RequestCount {
        serde_json::from_value(spec).unwrap()
    }

    #[test]
    fn request_count_holds_inclusively_between_its_bounds() {
        let ledger = Ledger {
            requests: vec![
                request("POST", SUBSCRIPTION_PATH, true),
                request("POST", SUBSCRIPTION_PATH, true),
                request("GET", SUBSCRIPTION_PATH, true),
                request("POST", "/helix/eventsub/subscriptions/extra", false),
            ],
            ..Ledger::default()
        };
        let path = SUBSCRIPTION_PATH;
        for (spec, holds) in [
            (json!({ "method": "POST", "path": path, "min": 2 }), true),
            (json!({ "method": "POST", "path": path, "min": 3 }), false),
            (json!({ "method": "POST", "path": path, "max": 2 }), true),
            (json!({ "method": "POST", "path": path, "max": 1 }), false),
            (json!({ "path": path, "min": 3, "max": 3 }), true),
            (json!({ "method": "DELETE", "path": path, "max": 0 }), true),
        ] {
            let (verdict, _) = assess_request_count(&ledger, &count(spec.clone()));
            assert_eq!(verdict == Verdict::Passed, holds, "{spec}");
        }
    }

    #[test]
    fn unmodeled_requests_fail_the_run_and_are_listed_as_evidence() {
        let ledger = Ledger {
            requests: vec![
                request("GET", "/helix/users", true),
                request("GET", "/helix/channels", false),
            ],
            ..Ledger::default()
        };
        let (verdict, evidence) = assess_no_unexpected_requests(&ledger);

        assert_eq!(
            verdict,
            Verdict::Failed(FailureCause::UnexpectedRequests { count: 1 })
        );
        let Evidence::Ledger(excerpt) = evidence else {
            panic!("ledger evidence expected");
        };
        let paths: Vec<&str> = excerpt.requests.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/helix/channels"]);
    }

    #[test]
    fn only_a_subscription_on_a_live_session_with_a_matching_version_is_held() {
        let ledger = Ledger {
            sessions: vec![session("closed", false), session("live", true)],
            subscriptions: vec![
                subscription("closed", "channel.follow"),
                subscription("live", "channel.chat.message"),
            ],
            ..Ledger::default()
        };
        for (subscription_type, version, held) in [
            ("channel.chat.message", None, true),
            ("channel.chat.message", Some("1"), true),
            ("channel.chat.message", Some("2"), false),
            ("channel.follow", None, false),
            ("channel.raid", None, false),
        ] {
            assert_eq!(
                holds_live_subscription(&ledger, subscription_type, version),
                held,
                "{subscription_type} {version:?}"
            );
        }
    }

    #[test]
    fn live_types_names_each_type_once_and_drops_the_ones_only_a_closed_session_held() {
        let ledger = Ledger {
            requests: Vec::new(),
            subscriptions: vec![
                subscription("gone", "channel.raid"),
                subscription("live", "channel.chat.message"),
                subscription("live", "channel.subscribe"),
                subscription("successor", "channel.chat.message"),
            ],
            sessions: vec![
                session("gone", false),
                session("live", true),
                session("successor", true),
            ],
        };

        assert_eq!(
            live_subscription_types(&ledger),
            ["channel.chat.message", "channel.subscribe"]
        );
    }
}
