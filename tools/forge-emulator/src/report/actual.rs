use std::collections::BTreeMap;

use forge_events::EventSource;

use super::expected::{
    count_range, describe_matcher, event_label, pointer_label, request_label, source_name,
};
use super::text::{clip, code, compact, plural, quoted, span_ms};
use crate::run::{
    CausationEvidence, EventEvidence, Evidence, ExpectationOutcome, FailureCause, GapKind,
    JournaledEvent, LedgerExcerpt, LogEvidence, NearMiss,
};
use crate::scenario::{Expectation, PayloadMatchers, TwitchSubscription};
use crate::twitch::{CredentialCheck, RecordedRequest};

const VALUE_CHARS: usize = 120;
const LISTED: usize = 5;

pub(crate) struct Actual {
    pub(crate) summary: String,
    pub(crate) details: Vec<String>,
}

pub(crate) fn actual(
    cause: &FailureCause,
    expectation: &Expectation,
    outcome: &ExpectationOutcome,
) -> Actual {
    let events = match &outcome.evidence {
        Evidence::Events(events) => Some(events),
        _ => None,
    };
    match (cause, expectation) {
        (FailureCause::NotObserved { needed, observed }, Expectation::Event(event)) => {
            let label = event_label(&event.kind, &event.payload);
            let summary = if *observed == 0 {
                format!(
                    "forge published no matching {label} within {}",
                    span_ms(event.within_ms)
                )
            } else {
                format!(
                    "forge published {observed} matching {label} within {}, {needed} needed",
                    span_ms(event.within_ms)
                )
            };
            let details = events
                .map(|events| {
                    stream_details(events, event.source, &event.payload, &event.kind, outcome)
                })
                .unwrap_or_default();
            Actual { summary, details }
        }
        (FailureCause::WrongCount { expected, observed }, Expectation::Event(event)) => {
            let summary = format!(
                "forge published {observed} matching {} within {}, expected exactly {expected}",
                event_label(&event.kind, &event.payload),
                span_ms(event.within_ms)
            );
            let mut details = Vec::new();
            if let Some(events) = events {
                if !events.samples.is_empty() {
                    details.push(format!("matches arrived at {}", arrivals(&events.samples)));
                }
                details.extend(stream_details(
                    events,
                    event.source,
                    &event.payload,
                    &event.kind,
                    outcome,
                ));
            }
            Actual { summary, details }
        }
        (FailureCause::Present { observed }, Expectation::EventAbsent(absent)) => {
            let summary = format!(
                "forge published {} matching {} within {}",
                plural(*observed as u64, "event"),
                code(&absent.kind),
                span_ms(absent.window_ms)
            );
            let details = events
                .filter(|events| !events.samples.is_empty())
                .map(|events| vec![format!("they arrived at {}", arrivals(&events.samples))])
                .unwrap_or_default();
            Actual { summary, details }
        }
        (
            FailureCause::StreamGap {
                dropped,
                undecodable,
            },
            _,
        ) => {
            let summary = format!(
                "the event stream lost {} and garbled {} after the step began, so it cannot prove or disprove the claim",
                plural(*dropped, "event"),
                plural(*undecodable as u64, "frame")
            );
            let mut details = Vec::new();
            if let Some(events) = events {
                details.push(format!("matching events seen: {}", events.matched));
                details.extend(events.gaps.iter().take(LISTED).map(|gap| match &gap.kind {
                    GapKind::Dropped(count) => format!(
                        "at +{} ms the server reported {} dropped",
                        gap.arrived_ms,
                        plural(*count, "event")
                    ),
                    GapKind::Undecodable { frame, reason } => format!(
                        "at +{} ms an undecodable frame ({reason}): {}",
                        gap.arrived_ms,
                        code(&clip(frame, VALUE_CHARS))
                    ),
                }));
            }
            Actual { summary, details }
        }
        (FailureCause::StreamClosed, _) => Actual {
            summary: format!(
                "the control connection closed before the window ended, after {} matching event(s)",
                events.map_or(0, |events| events.matched)
            ),
            details: Vec::new(),
        },
        (FailureCause::UnresolvedName { name }, _) => Actual {
            summary: format!(
                "no event was recorded under the name {}: the expectation naming it did not pass, or it allows more than one match",
                code(name)
            ),
            details: Vec::new(),
        },
        (FailureCause::WrongCause { expected, actual }, Expectation::CausedBy(causation)) => {
            let evidence = match &outcome.evidence {
                Evidence::Causation(causation) => causation.clone(),
                _ => CausationEvidence::default(),
            };
            let effect = named_event(&causation.effect, evidence.effect.as_ref());
            let wanted = format!("{} ({})", code(&causation.cause), code(&expected.to_string()));
            let summary = match actual {
                Some(actual) => {
                    let parent = evidence
                        .chain
                        .first()
                        .filter(|parent| parent.event.id == *actual)
                        .map(|parent| format!(" (a {} event)", code(&parent.event.kind)))
                        .unwrap_or_default();
                    format!(
                        "{effect} was caused by {}{parent}, not by {wanted}",
                        code(&actual.to_string())
                    )
                }
                None => format!("{effect} carries no cause; expected {wanted}"),
            };
            let details = match (&evidence.effect, actual) {
                (Some(effect), _) if !evidence.chain.is_empty() => {
                    vec![format!(
                        "observed ancestry, nearest first: {}",
                        ancestry(effect, &evidence.chain)
                    )]
                }
                (Some(_), Some(_)) => vec![
                    "its cause is not among the events this run subscribed to, so the ancestry is unknown"
                        .to_owned(),
                ],
                _ => Vec::new(),
            };
            Actual { summary, details }
        }
        (FailureCause::NoSubscription, Expectation::TwitchSubscription(subscription)) => Actual {
            summary: format!(
                "no live EventSub session held a {} subscription within {}",
                code(&subscription.subscription_type),
                span_ms(subscription.within_ms)
            ),
            details: match &outcome.evidence {
                Evidence::Ledger(ledger) => subscription_details(ledger, subscription),
                _ => Vec::new(),
            },
        },
        (FailureCause::UnexpectedRequests { count }, _) => Actual {
            summary: format!(
                "forge sent {} the fake Twitch has no model for",
                plural(*count as u64, "request")
            ),
            details: match &outcome.evidence {
                Evidence::Ledger(ledger) => ledger
                    .requests
                    .iter()
                    .take(LISTED)
                    .map(request_line)
                    .collect(),
                _ => Vec::new(),
            },
        },
        (
            FailureCause::RequestCountOutOfRange { observed, min, max },
            Expectation::TwitchRequestCount(count),
        ) => Actual {
            summary: format!(
                "forge sent {observed} {} request(s), allowed {}",
                code(&request_label(count)),
                count_range(*min, *max)
            ),
            details: match &outcome.evidence {
                Evidence::Ledger(ledger) if !ledger.requests.is_empty() => {
                    vec![format!("answered {}", statuses(&ledger.requests))]
                }
                _ => Vec::new(),
            },
        },
        (FailureCause::NoFakeTwitch, _) => Actual {
            summary: "the run has no fake Twitch (the scenario needs both `fixture.twitch` and `fakes.twitch`), so the check could not run".to_owned(),
            details: Vec::new(),
        },
        (FailureCause::NoLogLine, Expectation::LogLine(line)) => Actual {
            summary: format!(
                "no line on target {} carried those fields within {}",
                code(&line.target),
                span_ms(line.within_ms)
            ),
            details: match &outcome.evidence {
                Evidence::Log(log) => log_details(log, &line.target, &line.fields.0),
                _ => Vec::new(),
            },
        },
        (FailureCause::LogUnreadable { reason }, _) => Actual {
            summary: format!("forge's log could not be read: {reason}"),
            details: Vec::new(),
        },
        (cause, _) => Actual {
            summary: cause.to_string(),
            details: Vec::new(),
        },
    }
}

fn stream_details(
    events: &EventEvidence,
    source: Option<EventSource>,
    payload: &PayloadMatchers,
    kind: &str,
    outcome: &ExpectationOutcome,
) -> Vec<String> {
    let mut details = Vec::new();
    if let Some(first) = events.late.first() {
        let late_by = outcome
            .deadline_ms
            .map(|deadline| format!(", {} ms late", first.arrived_ms.saturating_sub(deadline)))
            .unwrap_or_default();
        details.push(format!(
            "{} arrived after the deadline, the first at +{} ms{late_by}",
            plural(events.late.len() as u64, "matching event"),
            first.arrived_ms
        ));
    }
    let closest = events
        .near_misses
        .iter()
        .min_by_key(|miss| miss.mismatched.len());
    match closest {
        Some(miss) => {
            let mut line = format!(
                "closest near miss: {} at +{} ms differs at {}",
                code(kind),
                miss.event.arrived_ms,
                differences(miss, source, payload)
            );
            if events.near_misses.len() > 1 {
                line.push_str(&format!(
                    " ({} other {} event(s) also missed)",
                    events.near_misses.len() - 1,
                    code(kind)
                ));
            }
            details.push(line);
        }
        None if events.late.is_empty() && events.matched == 0 => {
            details.push(format!("no {} event arrived at all", code(kind)));
        }
        None => {}
    }
    details
}

fn differences(miss: &NearMiss, source: Option<EventSource>, payload: &PayloadMatchers) -> String {
    let lines: Vec<String> =
        miss.mismatched
            .iter()
            .map(|field| {
                if field == "source" {
                    format!(
                        "source (wanted {}, got {})",
                        source.map_or_else(|| "any".to_owned(), source_name),
                        source_name(miss.event.event.source)
                    )
                } else {
                    let wanted = payload
                        .0
                        .get(field)
                        .map_or_else(|| "a match".to_owned(), describe_matcher);
                    let got =
                        miss.event.event.payload.pointer(field).map_or_else(
                            || "nothing".to_owned(),
                            |value| compact(value, VALUE_CHARS),
                        );
                    format!("{} (wanted {wanted}, got {got})", pointer_label(field))
                }
            })
            .collect();
    lines.join("; ")
}

fn arrivals(events: &[JournaledEvent]) -> String {
    let mut listed: Vec<String> = events
        .iter()
        .take(LISTED)
        .map(|event| format!("+{} ms", event.arrived_ms))
        .collect();
    if events.len() > LISTED {
        listed.push(format!("{} more", events.len() - LISTED));
    }
    listed.join(", ")
}

fn named_event(name: &str, event: Option<&JournaledEvent>) -> String {
    match event {
        Some(event) => format!(
            "{} ({} {})",
            code(name),
            code(&event.event.kind),
            code(&event.event.id.to_string())
        ),
        None => code(name),
    }
}

fn ancestry(effect: &JournaledEvent, chain: &[JournaledEvent]) -> String {
    std::iter::once(effect)
        .chain(chain)
        .map(|event| {
            format!(
                "{} {}",
                code(&event.event.kind),
                code(&event.event.id.to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(" <- ")
}

fn subscription_details(ledger: &LedgerExcerpt, expected: &TwitchSubscription) -> Vec<String> {
    let live = ledger
        .sessions
        .iter()
        .filter(|session| session.live)
        .count();
    let mut details = vec![format!(
        "EventSub sessions opened: {} ({live} live)",
        ledger.sessions.len()
    )];
    let held: Vec<String> = ledger
        .subscriptions
        .iter()
        .filter(|subscription| subscription.subscription_type == expected.subscription_type)
        .map(|subscription| {
            format!(
                "version {} on session {}",
                code(&subscription.version),
                code(&subscription.session_id)
            )
        })
        .collect();
    if held.is_empty() {
        details.push(format!(
            "no {} subscription exists on any session",
            code(&expected.subscription_type)
        ));
    } else {
        details.push(format!(
            "{} subscriptions held: {}",
            code(&expected.subscription_type),
            held.join(", ")
        ));
    }
    if ledger.requests.is_empty() {
        details.push("forge sent no subscription-creation request".to_owned());
    } else {
        details.push(format!(
            "subscription-creation requests: {}, answered {}",
            ledger.requests.len(),
            statuses(&ledger.requests)
        ));
    }
    let refused = ledger
        .requests
        .iter()
        .filter(|request| request.credentials != CredentialCheck::Accepted)
        .count();
    if refused > 0 {
        details.push(format!(
            "{} failed the fake's credential check",
            plural(refused as u64, "request")
        ));
    }
    details
}

fn log_details(log: &LogEvidence, target: &str, wanted: &BTreeMap<String, String>) -> Vec<String> {
    let closest = log.near_misses.iter().min_by_key(|record| {
        wanted
            .iter()
            .filter(|(key, value)| record.fields.get(*key) != Some(*value))
            .count()
    });
    let Some(record) = closest else {
        let files: Vec<String> = log
            .files
            .iter()
            .filter_map(|file| file.file_name())
            .map(|name| code(&name.to_string_lossy()))
            .collect();
        let read = if files.is_empty() {
            "no log file existed".to_owned()
        } else {
            format!("files read: {}", files.join(", "))
        };
        return vec![format!(
            "forge logged nothing on {} in the window; {read}",
            code(target)
        )];
    };
    let differences: Vec<String> = wanted
        .iter()
        .filter(|(key, value)| record.fields.get(*key) != Some(*value))
        .map(|(key, value)| {
            let got = record
                .fields
                .get(key)
                .map_or_else(|| "nothing".to_owned(), |got| quoted(got));
            format!("{} (wanted {}, got {got})", code(key), quoted(value))
        })
        .collect();
    let mut line = format!(
        "closest line on {} differs at {}",
        code(target),
        differences.join("; ")
    );
    if log.near_misses.len() > 1 {
        line.push_str(&format!(
            " ({} other line(s) on the target also missed)",
            log.near_misses.len() - 1
        ));
    }
    vec![line]
}

pub(crate) fn request_line(request: &RecordedRequest) -> String {
    let query: Vec<String> = request
        .query
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    let target = if query.is_empty() {
        format!("{} {}", request.method, request.path)
    } else {
        format!("{} {}?{}", request.method, request.path, query.join("&"))
    };
    format!(
        "{} answered {}{}",
        code(&clip(&target, VALUE_CHARS)),
        request.status,
        credentials_note(request.credentials)
    )
}

fn credentials_note(check: CredentialCheck) -> &'static str {
    match check {
        CredentialCheck::Accepted => "",
        CredentialCheck::MissingBearer => " (no bearer token)",
        CredentialCheck::WrongBearer => " (wrong bearer token)",
        CredentialCheck::MissingClientId => " (no client id)",
        CredentialCheck::WrongClientId => " (wrong client id)",
    }
}

fn statuses(requests: &[RecordedRequest]) -> String {
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
    for request in requests {
        *counts.entry(request.status).or_default() += 1;
    }
    counts
        .iter()
        .map(|(status, count)| format!("{status} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}
