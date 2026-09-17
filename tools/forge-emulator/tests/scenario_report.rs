//! Bug reports rendered from synthetic run outcomes. Nothing here starts forge.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forge_emulator::EmulatorError;
use forge_emulator::fixture::{Fixture, REDACTED, Redactions, seed};
use forge_emulator::launch::{DEFAULT_LOG_DIRECTIVES, ForgeExit};
use forge_emulator::report::{
    BugEntry, JSON_FILE, MARKDOWN_FILE, RunContext, RunReport, bug_entries, write_report,
};
use forge_emulator::run::{
    ActionDetail, ActionReport, CausationEvidence, EventEvidence, Evidence, ExpectationOutcome,
    FailureCause, ForgeEvidence, Gap, GapKind, JournaledEvent, LedgerExcerpt, LogEvidence,
    LogRecord, NearMiss, ScenarioOutcome, ScenarioVerdict, StepOutcome, StepStatus, Verdict,
};
use forge_emulator::scenario::Scenario;
use forge_emulator::twitch::{
    CredentialCheck, RecordedRequest, RecordedSession, RecordedSubscription,
};
use forge_events::{Event, EventSource};
use forge_types::EventId;
use serde_json::{Value, json};
use time::macros::datetime;

const EMULATOR: &str = env!("CARGO_BIN_EXE_forge-emulator");
const CHAT_ID: &str = "01J8ZQ0000000000000000CHAT";
const COMMAND_ID: &str = "01J8ZQ00000000000000000CMD";
const START_ID: &str = "01J8ZQ0000000000000000STRT";

const STEP_SUBSCRIBED: usize = 1;
const STEP_CHAT: usize = 2;
const STEP_CROWD: usize = 3;

const EXPECT_CHAT: usize = 0;
const EXPECT_STARTED: usize = 1;
const EXPECT_NOT_BLOCKED: usize = 2;
const EXPECT_CAUSED: usize = 3;
const EXPECT_LOG: usize = 4;
const EXPECT_NO_UNMODELED: usize = 5;
const EXPECT_SUBSCRIPTION_COUNT: usize = 6;
const EXPECT_SUBSCRIPTION: usize = 7;

const ALICE: &str = "As a Twitch viewer `alice` (shown as `Alice Doe`) with badges moderator, subscriber (12 months) I send \"!ping\" in chat";

fn scenario() -> Scenario {
    serde_json::from_value(json!({
        "name": "ping reaches forge",
        "purpose": "A viewer command runs its action exactly once.",
        "fixture": { "twitch": {}, "chat_commands": [ { "phrase": "!ping", "action_name": "Ping" } ] },
        "fakes": { "twitch": {} },
        "steps": [
            { "do": { "forge_ready": { "within_ms": 30000 } } },
            { "do": { "twitch_subscribed": { "types": ["channel.chat.message"], "within_ms": 5000 } } },
            {
                "do": { "chat": {
                    "viewer": { "user_id": "200000042", "login": "alice", "display_name": "Alice Doe",
                                "badges": ["moderator", { "subscriber": { "months": 12 } }] },
                    "text": "!ping"
                } },
                "expect": [
                    { "event": { "name": "chat", "source": "twitch", "kind": "chat.message",
                                 "payload": { "/message": { "equals": "!ping" }, "/user/login": { "equals": "alice" } },
                                 "within_ms": 2000 } },
                    { "event": { "name": "started", "kind": "action.start",
                                 "payload": { "/action_name": { "equals": "Ping" } },
                                 "count": { "exactly": 1 }, "within_ms": 2000 } },
                    { "event_absent": { "kind": "trigger.blocked", "window_ms": 2000 } },
                    { "caused_by": { "effect": "started", "cause": "chat" } },
                    { "log_line": { "target": "forge::trigger", "fields": { "reason": "permission" }, "within_ms": 500 } },
                    { "twitch_no_unexpected_requests": {} },
                    { "twitch_request_count": { "method": "POST", "path": "/helix/eventsub/subscriptions", "min": 1, "max": 2 } },
                    { "twitch_subscription": { "type": "channel.chat.message", "version": "1", "within_ms": 1000 } }
                ]
            },
            {
                "do": { "crowd": { "viewers": 4, "chatter": ["hi {login}"], "command_senders": 4,
                                   "commands": ["!dice"], "spacing_ms": 50 } },
                "expect": [
                    { "event": { "kind": "action.start", "payload": { "/action_name": { "equals": "Dice" } },
                                 "count": { "exactly": 4 }, "within_ms": 3000 } }
                ]
            },
            { "do": { "run_action": { "action": "Ping", "args": { "x": 1 } } } },
            { "do": { "session_reconnect": { "within_ms": 4000 } } },
            { "do": { "set_global": { "name": "mood", "value": "calm", "persisted": true } } },
            { "do": { "pause": { "ms": 100, "reason": "let the queue settle" } } }
        ]
    }))
    .unwrap()
}

fn id(text: &str) -> EventId {
    serde_json::from_value(json!(text)).unwrap()
}

fn event(id_text: &str, source: EventSource, kind: &str, payload: Value) -> Event {
    Event {
        id: id(id_text),
        source,
        kind: kind.to_owned(),
        timestamp: datetime!(2026-09-13 10:00:00 UTC),
        payload,
        caused_by: None,
        replay: false,
    }
}

fn at(arrived_ms: u64, event: Event) -> JournaledEvent {
    JournaledEvent { arrived_ms, event }
}

fn chat_event(id_text: &str, text: &str) -> Event {
    event(
        id_text,
        EventSource::Twitch,
        "chat.message",
        json!({ "message": text, "user": { "login": "alice" } }),
    )
}

fn caused(mut event: Event, cause: &str) -> Event {
    event.caused_by = Some(id(cause));
    event
}

fn scenario_keyword(scenario: &Scenario, step: usize) -> String {
    scenario.steps[step].action.keyword().to_owned()
}

fn passed_step(scenario: &Scenario, index: usize) -> StepOutcome {
    let expectations = scenario.steps[index]
        .expect
        .iter()
        .enumerate()
        .map(|(position, expectation)| ExpectationOutcome {
            index: position,
            keyword: expectation.keyword().to_owned(),
            verdict: Verdict::Passed,
            deadline_ms: None,
            evaluated_ms: Some(1000),
            evidence: Evidence::None,
        })
        .collect();
    StepOutcome {
        index,
        keyword: scenario_keyword(scenario, index),
        status: StepStatus::Passed,
        started_ms: Some(100 * index as u64),
        acted_ms: Some(100 * index as u64 + 5),
        action: Some(ActionReport::Done(ActionDetail::Paused)),
        expectations,
    }
}

fn all_passed(scenario: &Scenario) -> Vec<StepOutcome> {
    (0..scenario.steps.len())
        .map(|index| passed_step(scenario, index))
        .collect()
}

/// Every step passed except `expectation` of `step`, which failed with `cause` and `evidence`.
fn one_failure(
    scenario: &Scenario,
    step: usize,
    expectation: usize,
    cause: FailureCause,
    evidence: Evidence,
) -> Vec<StepOutcome> {
    let mut steps = all_passed(scenario);
    let failed = &mut steps[step];
    failed.status = StepStatus::Failed;
    failed.expectations[expectation].verdict = Verdict::Failed(cause);
    failed.expectations[expectation].deadline_ms = Some(2100);
    failed.expectations[expectation].evidence = evidence;
    steps
}

fn forge_evidence() -> ForgeEvidence {
    ForgeEvidence {
        run_root: PathBuf::from("/tmp/forge-emulator-run"),
        data_dir: PathBuf::from("/tmp/forge-emulator-run/attempt-1/data"),
        log_dir: PathBuf::from("/tmp/forge-emulator-run/attempt-1/data/logs"),
        pid: 4242,
        version: Some("0.5.1".to_owned()),
        attempts: 1,
        exited_during_run: false,
        exit: Some(ForgeExit {
            status: "exit status: 0".to_owned(),
            code: Some(0),
            forced: false,
        }),
        teardown_error: None,
        stderr_tail: vec!["forge-desktop: shutting down".to_owned()],
        stdout_tail: Vec::new(),
        log_tail: vec!["2026-09-13T10:00:01Z  INFO forge::boot: ready".to_owned()],
    }
}

fn outcome(scenario: &Scenario, steps: Vec<StepOutcome>) -> ScenarioOutcome {
    let verdict = if steps.iter().all(|step| step.status == StepStatus::Passed) {
        ScenarioVerdict::Passed
    } else {
        ScenarioVerdict::Failed
    };
    ScenarioOutcome {
        name: scenario.name.clone(),
        verdict,
        steps,
        forge: Some(forge_evidence()),
        redactions: Redactions::for_run(&scenario.fixture, None),
    }
}

fn context() -> RunContext {
    RunContext {
        scenario_file: PathBuf::from("/home/qa/scenarios/ping.json"),
        forge_binary: PathBuf::from("/home/qa/forge/target/debug/forge"),
        log_directives: DEFAULT_LOG_DIRECTIVES.to_owned(),
        run_root: PathBuf::from("/tmp/forge-emulator-run"),
        started_at: datetime!(2026-09-13 10:00:00 UTC),
        duration_ms: 12_345,
    }
}

fn only_bug(scenario: &Scenario, outcome: &ScenarioOutcome) -> BugEntry {
    let mut bugs = bug_entries(scenario, outcome);
    assert_eq!(bugs.len(), 1, "{bugs:#?}");
    bugs.remove(0)
}

fn request(method: &str, path: &str, status: u16, credentials: CredentialCheck) -> RecordedRequest {
    RecordedRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        query: Vec::new(),
        body: None,
        credentials,
        status,
        response: json!({}),
        modeled: true,
    }
}

fn log_record(fields: &[(&str, &str)]) -> LogRecord {
    let fields: BTreeMap<String, String> = fields
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    LogRecord {
        file: PathBuf::from("/tmp/forge-emulator-run/attempt-1/data/logs/forge.log.2026-09-13"),
        level: "DEBUG".to_owned(),
        target: "forge::trigger".to_owned(),
        line: format!("2026-09-13T10:00:01Z DEBUG forge::trigger: trigger rejected {fields:?}"),
        fields,
    }
}

struct Case {
    label: &'static str,
    step: usize,
    expectation: usize,
    cause: FailureCause,
    evidence: Evidence,
    title: &'static str,
    expected: &'static str,
    actual: &'static str,
    details: Vec<&'static str>,
}

fn assert_cases(cases: Vec<Case>) {
    let scenario = scenario();
    for case in cases {
        let steps = one_failure(
            &scenario,
            case.step,
            case.expectation,
            case.cause,
            case.evidence,
        );
        let bug = only_bug(&scenario, &outcome(&scenario, steps));
        assert_eq!(bug.step, Some(case.step), "{}", case.label);
        assert_eq!(bug.expectation, Some(case.expectation), "{}", case.label);
        assert_eq!(bug.title, case.title, "{}", case.label);
        assert_eq!(bug.expected, case.expected, "{}", case.label);
        assert_eq!(bug.actual, case.actual, "{}", case.label);
        assert_eq!(bug.details, case.details, "{}", case.label);
    }
}

#[test]
fn chat_expectation_failure_tells_the_viewer_story_and_pins_the_matcher() {
    let scenario = scenario();
    let steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CHAT,
        FailureCause::NotObserved {
            needed: 1,
            observed: 0,
        },
        Evidence::Events(EventEvidence::default()),
    );
    let bug = only_bug(&scenario, &outcome(&scenario, steps));
    assert_eq!(bug.story, ALICE);
    assert_eq!(
        bug.matcher.as_deref(),
        Some(
            r#"{"event":{"name":"chat","source":"twitch","kind":"chat.message","payload":{"/message":{"equals":"!ping"},"/user/login":{"equals":"alice"}},"count":{"at_least":1},"within_ms":2000}}"#
        )
    );
}

#[test]
fn event_stream_failures_spell_out_counts_near_misses_and_gaps() {
    assert_cases(vec![
        Case {
            label: "not observed, one late match, two near misses",
            step: STEP_CHAT,
            expectation: EXPECT_CHAT,
            cause: FailureCause::NotObserved {
                needed: 1,
                observed: 0,
            },
            evidence: Evidence::Events(EventEvidence {
                matched: 0,
                samples: Vec::new(),
                late: vec![at(2350, chat_event(CHAT_ID, "!ping"))],
                near_misses: vec![
                    NearMiss {
                        event: at(
                            900,
                            event(COMMAND_ID, EventSource::Core, "chat.message", json!({})),
                        ),
                        mismatched: vec![
                            "source".to_owned(),
                            "/message".to_owned(),
                            "/user/login".to_owned(),
                        ],
                    },
                    NearMiss {
                        event: at(1200, chat_event(START_ID, "!pong")),
                        mismatched: vec!["/message".to_owned()],
                    },
                ],
                gaps: Vec::new(),
            }),
            title: "No `chat.message` after chat message \"!ping\"",
            expected: "forge publishes `chat.message` from twitch for this message where `/user/login` equals \"alice\" within 2s",
            actual: "forge published no matching `chat.message` within 2s",
            details: vec![
                "1 matching event arrived after the deadline, the first at +2350 ms, 250 ms late",
                "closest near miss: `chat.message` at +1200 ms differs at `/message` (wanted equals \"!ping\", got \"!pong\") (1 other `chat.message` event(s) also missed)",
            ],
        },
        Case {
            label: "not observed, nothing of the kind at all",
            step: STEP_CHAT,
            expectation: EXPECT_CHAT,
            cause: FailureCause::NotObserved {
                needed: 1,
                observed: 0,
            },
            evidence: Evidence::Events(EventEvidence::default()),
            title: "No `chat.message` after chat message \"!ping\"",
            expected: "forge publishes `chat.message` from twitch for this message where `/user/login` equals \"alice\" within 2s",
            actual: "forge published no matching `chat.message` within 2s",
            details: vec!["no `chat.message` event arrived at all"],
        },
        Case {
            label: "near miss on source",
            step: STEP_CHAT,
            expectation: EXPECT_CHAT,
            cause: FailureCause::NotObserved {
                needed: 1,
                observed: 0,
            },
            evidence: Evidence::Events(EventEvidence {
                near_misses: vec![NearMiss {
                    event: at(
                        800,
                        event(
                            CHAT_ID,
                            EventSource::YouTube,
                            "chat.message",
                            json!({ "message": "!ping", "user": { "login": "alice" } }),
                        ),
                    ),
                    mismatched: vec!["source".to_owned()],
                }],
                ..EventEvidence::default()
            }),
            title: "No `chat.message` after chat message \"!ping\"",
            expected: "forge publishes `chat.message` from twitch for this message where `/user/login` equals \"alice\" within 2s",
            actual: "forge published no matching `chat.message` within 2s",
            details: vec![
                "closest near miss: `chat.message` at +800 ms differs at source (wanted twitch, got you_tube)",
            ],
        },
        Case {
            label: "wrong count in a crowd",
            step: STEP_CROWD,
            expectation: 0,
            cause: FailureCause::WrongCount {
                expected: 4,
                observed: 5,
            },
            evidence: Evidence::Events(EventEvidence {
                matched: 5,
                samples: [100, 200, 300, 400, 500, 600]
                    .into_iter()
                    .map(|ms| {
                        at(
                            ms,
                            event(
                                START_ID,
                                EventSource::Core,
                                "action.start",
                                json!({ "action_name": "Dice" }),
                            ),
                        )
                    })
                    .collect(),
                ..EventEvidence::default()
            }),
            title: "`action.start` for `Dice` seen 5 times instead of 4 after a crowd of 4 viewers",
            expected: "action `Dice` starts exactly 4 times within 3s",
            actual: "forge published 5 matching `action.start` for `Dice` within 3s, expected exactly 4",
            details: vec!["matches arrived at +100 ms, +200 ms, +300 ms, +400 ms, +500 ms, 1 more"],
        },
        Case {
            label: "present when it must be absent",
            step: STEP_CHAT,
            expectation: EXPECT_NOT_BLOCKED,
            cause: FailureCause::Present { observed: 1 },
            evidence: Evidence::Events(EventEvidence {
                matched: 1,
                samples: vec![at(
                    900,
                    event(
                        START_ID,
                        EventSource::Core,
                        "trigger.blocked",
                        json!({ "reason": "cooldown" }),
                    ),
                )],
                ..EventEvidence::default()
            }),
            title: "Unexpected `trigger.blocked` after chat message \"!ping\"",
            expected: "forge publishes no `trigger.blocked` within 2s",
            actual: "forge published at least 1 event matching `trigger.blocked` within 2s",
            details: vec!["they arrived at +900 ms"],
        },
        Case {
            label: "stream gap",
            step: STEP_CHAT,
            expectation: EXPECT_CHAT,
            cause: FailureCause::StreamGap {
                dropped: 3,
                undecodable: 1,
            },
            evidence: Evidence::Events(EventEvidence {
                gaps: vec![
                    Gap {
                        arrived_ms: 700,
                        kind: GapKind::Dropped(3),
                    },
                    Gap {
                        arrived_ms: 750,
                        kind: GapKind::Undecodable {
                            frame: "not json".to_owned(),
                            reason: "expected value".to_owned(),
                        },
                    },
                ],
                ..EventEvidence::default()
            }),
            title: "Event stream gap while checking `chat.message` after chat message \"!ping\"",
            expected: "forge publishes `chat.message` from twitch for this message where `/user/login` equals \"alice\" within 2s",
            actual: "the event stream lost 3 events and garbled 1 frame after the step began, so it cannot prove or disprove the claim",
            details: vec![
                "matching events seen: 0",
                "at +700 ms the server reported 3 events dropped",
                "at +750 ms an undecodable frame (expected value): `not json`",
            ],
        },
        Case {
            label: "stream closed",
            step: STEP_CHAT,
            expectation: EXPECT_STARTED,
            cause: FailureCause::StreamClosed,
            evidence: Evidence::Events(EventEvidence::default()),
            title: "Control connection closed while checking `action.start` for `Ping` after chat message \"!ping\"",
            expected: "action `Ping` starts exactly once within 2s",
            actual: "the control connection closed before the window ended, after 0 matching event(s)",
            details: Vec::new(),
        },
    ]);
}

#[test]
fn causation_failures_name_the_actual_cause_and_the_observed_ancestry() {
    let chat = at(1000, chat_event(CHAT_ID, "!ping"));
    let command = at(
        1010,
        caused(
            event(
                COMMAND_ID,
                EventSource::Core,
                "command.matched",
                json!({ "command": "!ping" }),
            ),
            CHAT_ID,
        ),
    );
    assert_cases(vec![
        Case {
            label: "unresolved name",
            step: STEP_CHAT,
            expectation: EXPECT_CAUSED,
            cause: FailureCause::UnresolvedName {
                name: "started".to_owned(),
            },
            evidence: Evidence::Causation(CausationEvidence {
                cause: Some(chat.clone()),
                ..CausationEvidence::default()
            }),
            title: "No event named `started` to check causation",
            expected: "the event named `started` is directly caused by the event named `chat`",
            actual: "no event was recorded under the name `started`: the expectation naming it did not pass, or it allows more than one match",
            details: Vec::new(),
        },
        Case {
            label: "caused by an intermediate event",
            step: STEP_CHAT,
            expectation: EXPECT_CAUSED,
            cause: FailureCause::WrongCause {
                expected: id(CHAT_ID),
                actual: Some(id(COMMAND_ID)),
            },
            evidence: Evidence::Causation(CausationEvidence {
                effect: Some(at(
                    1020,
                    caused(
                        event(START_ID, EventSource::Core, "action.start", json!({})),
                        COMMAND_ID,
                    ),
                )),
                cause: Some(chat.clone()),
                chain: vec![command.clone(), chat.clone()],
            }),
            title: "`started` not caused by `chat`",
            expected: "the event named `started` is directly caused by the event named `chat`",
            actual: "`started` (`action.start` `01J8ZQ0000000000000000STRT`) was caused by `01J8ZQ00000000000000000CMD` (a `command.matched` event), not by `chat` (`01J8ZQ0000000000000000CHAT`)",
            details: vec![
                "observed ancestry, nearest first: `action.start` `01J8ZQ0000000000000000STRT` <- `command.matched` `01J8ZQ00000000000000000CMD` <- `chat.message` `01J8ZQ0000000000000000CHAT`",
            ],
        },
        Case {
            label: "caused by an event the run never subscribed to",
            step: STEP_CHAT,
            expectation: EXPECT_CAUSED,
            cause: FailureCause::WrongCause {
                expected: id(CHAT_ID),
                actual: Some(id(COMMAND_ID)),
            },
            evidence: Evidence::Causation(CausationEvidence {
                effect: Some(at(
                    1020,
                    caused(
                        event(START_ID, EventSource::Core, "action.start", json!({})),
                        COMMAND_ID,
                    ),
                )),
                cause: Some(chat.clone()),
                chain: Vec::new(),
            }),
            title: "`started` not caused by `chat`",
            expected: "the event named `started` is directly caused by the event named `chat`",
            actual: "`started` (`action.start` `01J8ZQ0000000000000000STRT`) was caused by `01J8ZQ00000000000000000CMD`, not by `chat` (`01J8ZQ0000000000000000CHAT`)",
            details: vec![
                "its cause is not among the events this run subscribed to, so the ancestry is unknown",
            ],
        },
        Case {
            label: "no cause at all",
            step: STEP_CHAT,
            expectation: EXPECT_CAUSED,
            cause: FailureCause::WrongCause {
                expected: id(CHAT_ID),
                actual: None,
            },
            evidence: Evidence::Causation(CausationEvidence {
                effect: Some(at(
                    1020,
                    event(START_ID, EventSource::Core, "action.start", json!({})),
                )),
                cause: Some(chat),
                chain: Vec::new(),
            }),
            title: "`started` not caused by `chat`",
            expected: "the event named `started` is directly caused by the event named `chat`",
            actual: "`started` (`action.start` `01J8ZQ0000000000000000STRT`) carries no cause; expected `chat` (`01J8ZQ0000000000000000CHAT`)",
            details: Vec::new(),
        },
    ]);
}

#[test]
fn fake_twitch_ledger_failures_summarise_sessions_requests_and_statuses() {
    let subscriptions = "/helix/eventsub/subscriptions";
    assert_cases(vec![
        Case {
            label: "no subscription",
            step: STEP_CHAT,
            expectation: EXPECT_SUBSCRIPTION,
            cause: FailureCause::NoSubscription,
            evidence: Evidence::Ledger(LedgerExcerpt {
                requests: vec![
                    request("POST", subscriptions, 409, CredentialCheck::Accepted),
                    request("POST", subscriptions, 401, CredentialCheck::WrongBearer),
                    request("POST", subscriptions, 401, CredentialCheck::MissingBearer),
                ],
                subscriptions: vec![RecordedSubscription {
                    id: "sub-1".to_owned(),
                    session_id: "session-1".to_owned(),
                    subscription_type: "channel.follow".to_owned(),
                    version: "2".to_owned(),
                    condition: json!({}),
                    created_at: "2026-09-13T10:00:00Z".to_owned(),
                }],
                sessions: vec![RecordedSession {
                    id: "session-1".to_owned(),
                    reconnected_from: None,
                    connected_at: "2026-09-13T10:00:00Z".to_owned(),
                    live: true,
                }],
            }),
            title: "No live `channel.chat.message` subscription after chat message \"!ping\"",
            expected: "forge holds a live EventSub subscription to `channel.chat.message` (version `1`) within 1s",
            actual: "no live EventSub session held a `channel.chat.message` subscription within 1s",
            details: vec![
                "EventSub sessions opened: 1 (1 live)",
                "no `channel.chat.message` subscription exists on any session",
                "subscription-creation requests: 3, answered 401 (2), 409 (1)",
                "2 requests failed the fake's credential check",
            ],
        },
        Case {
            label: "unmodeled requests",
            step: STEP_CHAT,
            expectation: EXPECT_NO_UNMODELED,
            cause: FailureCause::UnexpectedRequests { count: 2 },
            evidence: Evidence::Ledger(LedgerExcerpt {
                requests: vec![
                    RecordedRequest {
                        query: vec![("broadcaster_id".to_owned(), "100000001".to_owned())],
                        modeled: false,
                        ..request(
                            "GET",
                            "/helix/moderation/banned",
                            404,
                            CredentialCheck::Accepted,
                        )
                    },
                    RecordedRequest {
                        modeled: false,
                        ..request(
                            "DELETE",
                            "/helix/chat/messages",
                            404,
                            CredentialCheck::MissingClientId,
                        )
                    },
                ],
                ..LedgerExcerpt::default()
            }),
            title: "forge sent Twitch requests the fake does not model",
            expected: "forge sends the fake Twitch only requests it models, over the whole run so far",
            actual: "forge sent 2 requests the fake Twitch has no model for",
            details: vec![
                "`GET /helix/moderation/banned?broadcaster_id=100000001` answered 404",
                "`DELETE /helix/chat/messages` answered 404 (no client id)",
            ],
        },
        Case {
            label: "request count above the maximum",
            step: STEP_CHAT,
            expectation: EXPECT_SUBSCRIPTION_COUNT,
            cause: FailureCause::RequestCountOutOfRange {
                observed: 3,
                min: Some(1),
                max: Some(2),
            },
            evidence: Evidence::Ledger(LedgerExcerpt {
                requests: vec![
                    request("POST", subscriptions, 202, CredentialCheck::Accepted),
                    request("POST", subscriptions, 409, CredentialCheck::Accepted),
                    request("POST", subscriptions, 409, CredentialCheck::Accepted),
                ],
                ..LedgerExcerpt::default()
            }),
            title: "`POST /helix/eventsub/subscriptions` sent 3 times",
            expected: "forge sends between 1 and 2 `POST /helix/eventsub/subscriptions` request(s) over the whole run so far",
            actual: "forge sent 3 `POST /helix/eventsub/subscriptions` request(s), allowed between 1 and 2",
            details: vec!["answered 202 (1), 409 (2)"],
        },
        Case {
            label: "no fake Twitch",
            step: STEP_CHAT,
            expectation: EXPECT_NO_UNMODELED,
            cause: FailureCause::NoFakeTwitch,
            evidence: Evidence::None,
            title: "`twitch_no_unexpected_requests` check has no fake Twitch",
            expected: "forge sends the fake Twitch only requests it models, over the whole run so far",
            actual: "the run has no fake Twitch (the scenario needs both `fixture.twitch` and `fakes.twitch`), so the check could not run",
            details: Vec::new(),
        },
    ]);
}

#[test]
fn log_failures_show_the_closest_line_or_the_files_read() {
    assert_cases(vec![
        Case {
            label: "closest line differs",
            step: STEP_CHAT,
            expectation: EXPECT_LOG,
            cause: FailureCause::NoLogLine,
            evidence: Evidence::Log(LogEvidence {
                matched: None,
                near_misses: vec![log_record(&[("reason", "cooldown")]), log_record(&[])],
                files: Vec::new(),
            }),
            title: "No `forge::trigger` log line after chat message \"!ping\"",
            expected: "forge logs a line on target `forge::trigger` with `reason` = \"permission\" within 500 ms",
            actual: "no line on target `forge::trigger` carried those fields within 500 ms",
            details: vec![
                "closest line on `forge::trigger` differs at `reason` (wanted \"permission\", got \"cooldown\") (1 other line(s) on the target also missed)",
            ],
        },
        Case {
            label: "nothing on the target",
            step: STEP_CHAT,
            expectation: EXPECT_LOG,
            cause: FailureCause::NoLogLine,
            evidence: Evidence::Log(LogEvidence {
                matched: None,
                near_misses: Vec::new(),
                files: vec![PathBuf::from("/tmp/run/logs/forge.log.2026-09-13")],
            }),
            title: "No `forge::trigger` log line after chat message \"!ping\"",
            expected: "forge logs a line on target `forge::trigger` with `reason` = \"permission\" within 500 ms",
            actual: "no line on target `forge::trigger` carried those fields within 500 ms",
            details: vec![
                "forge logged nothing on `forge::trigger` in the window; files read: `forge.log.2026-09-13`",
            ],
        },
        Case {
            label: "log unreadable",
            step: STEP_CHAT,
            expectation: EXPECT_LOG,
            cause: FailureCause::LogUnreadable {
                reason: "Permission denied (os error 13)".to_owned(),
            },
            evidence: Evidence::Log(LogEvidence::default()),
            title: "forge log unreadable after chat message \"!ping\"",
            expected: "forge logs a line on target `forge::trigger` with `reason` = \"permission\" within 500 ms",
            actual: "forge's log could not be read: Permission denied (os error 13)",
            details: Vec::new(),
        },
    ]);
}

#[test]
fn a_failed_step_action_becomes_a_bug_told_from_that_step_actor() {
    let scenario = scenario();
    for (step, story, title, expected) in [
        (
            STEP_SUBSCRIBED,
            "As the streamer I connect forge to Twitch and wait up to 5s for its `channel.chat.message` subscription(s)",
            "Twitch subscriptions not established",
            "forge holds a live EventSub subscription of each type `channel.chat.message` within 5s",
        ),
        (
            STEP_CHAT,
            ALICE,
            "Chat message not delivered to forge",
            "the fake Twitch delivers the message to forge's EventSub session",
        ),
        (
            STEP_CROWD,
            "As a crowd of 4 Twitch viewers I send 8 messages in chat, 4 of them commands (`!dice`), 50 ms apart",
            "Crowd not delivered to forge",
            "the fake Twitch delivers all 8 messages to forge's EventSub session",
        ),
        (
            4,
            "As the streamer I run action `Ping` with arguments `{\"x\":1}`",
            "Action `Ping` could not be run",
            "forge accepts the request to run action `Ping`",
        ),
        (
            5,
            "As Twitch I ask forge to reconnect its EventSub session and wait up to 4s for the successor",
            "forge did not reconnect its EventSub session",
            "forge opens a successor EventSub session within 4s",
        ),
        (
            6,
            "As the streamer I set persisted global `mood` to `\"calm\"`",
            "Global `mood` could not be set",
            "forge accepts setting global `mood`",
        ),
    ] {
        let mut steps = all_passed(&scenario);
        steps[step].status = StepStatus::Failed;
        steps[step].action = Some(ActionReport::Failed {
            reason: "timed out waiting for a successor EventSub session".to_owned(),
            ledger: None,
        });
        for expectation in &mut steps[step].expectations {
            expectation.verdict = Verdict::NotEvaluated;
        }
        let bug = only_bug(&scenario, &outcome(&scenario, steps));
        assert_eq!(
            (bug.step, bug.expectation),
            (Some(step), None),
            "step {step}"
        );
        assert_eq!(bug.story, story, "step {step}");
        assert_eq!(bug.title, title, "step {step}");
        assert_eq!(bug.expected, expected, "step {step}");
        assert_eq!(
            bug.actual, "timed out waiting for a successor EventSub session",
            "step {step}"
        );
    }
}

#[test]
fn forge_exiting_during_an_otherwise_passing_run_is_its_own_bug() {
    let scenario = scenario();
    let mut outcome = outcome(&scenario, all_passed(&scenario));
    outcome.verdict = ScenarioVerdict::Failed;
    let forge = outcome.forge.as_mut().unwrap();
    forge.exited_during_run = true;
    forge.exit = Some(ForgeExit {
        status: "signal: 11 (SIGSEGV)".to_owned(),
        code: None,
        forced: false,
    });

    assert_eq!(
        bug_entries(&scenario, &outcome),
        [BugEntry {
            step: None,
            expectation: None,
            title: "forge exited during the run".to_owned(),
            story: "As the streamer I run scenario `ping reaches forge` with forge open".to_owned(),
            expected: "forge keeps running until the emulator shuts it down".to_owned(),
            matcher: None,
            actual: "forge exited on its own during the run (signal: 11 (SIGSEGV))".to_owned(),
            details: Vec::new(),
        }]
    );
}

#[test]
fn forge_exiting_during_a_failing_run_is_noted_on_every_bug_instead() {
    let scenario = scenario();
    let steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CHAT,
        FailureCause::StreamClosed,
        Evidence::Events(EventEvidence::default()),
    );
    let mut outcome = outcome(&scenario, steps);
    let forge = outcome.forge.as_mut().unwrap();
    forge.exited_during_run = true;
    forge.exit = Some(ForgeExit {
        status: "exit status: 101".to_owned(),
        code: Some(101),
        forced: false,
    });

    let bug = only_bug(&scenario, &outcome);
    assert_eq!(
        bug.details,
        ["forge exited on its own during the run (exit status: 101)"]
    );
}

#[test]
fn failed_run_renders_each_bug_as_story_expected_actual_with_evidence() {
    let scenario = scenario();
    let steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CHAT,
        FailureCause::NotObserved {
            needed: 1,
            observed: 0,
        },
        Evidence::Events(EventEvidence {
            near_misses: vec![NearMiss {
                event: at(1200, chat_event(START_ID, "!pong")),
                mismatched: vec!["/message".to_owned()],
            }],
            ..EventEvidence::default()
        }),
    );
    let report = RunReport::new(context(), &scenario, &outcome(&scenario, steps)).unwrap();
    let markdown = report.to_markdown();
    let start = markdown.find("### Bug 1").unwrap();
    let end = markdown.find("## forge output").unwrap();
    assert_eq!(
        &markdown[start..end],
        r#"### Bug 1: No `chat.message` after chat message "!ping"

As a Twitch viewer `alice` (shown as `Alice Doe`) with badges moderator, subscriber (12 months) I send "!ping" in chat.

**Expected:** forge publishes `chat.message` from twitch for this message where `/user/login` equals "alice" within 2s.

**Actual:** forge published no matching `chat.message` within 2s.

- closest near miss: `chat.message` at +1200 ms differs at `/message` (wanted equals "!ping", got "!pong")

Matcher (step 2, expectation 0):

```json
{"event":{"name":"chat","source":"twitch","kind":"chat.message","payload":{"/message":{"equals":"!ping"},"/user/login":{"equals":"alice"}},"count":{"at_least":1},"within_ms":2000}}
```

#### Evidence

- matching events in the window: 0
- near miss: +1200 ms twitch `chat.message` `01J8ZQ0000000000000000STRT` `{"message":"!pong","user":{"login":"alice"}}`
- forge stderr and log tails: see [forge output](#forge-output)

"#
    );
}

#[test]
fn passed_run_report_is_a_summary_without_bugs_or_forge_output() {
    let scenario = scenario();
    let report = RunReport::new(
        context(),
        &scenario,
        &outcome(&scenario, all_passed(&scenario)),
    )
    .unwrap();
    let markdown = report.to_markdown();

    assert!(markdown.starts_with("# Scenario `ping reaches forge`: passed\n\n> A viewer command runs its action exactly once.\n"), "{markdown}");
    assert!(
        markdown.contains("| forge | version `0.5.1`, pid 4242, 1 launch attempt |"),
        "{markdown}"
    );
    assert!(
        markdown.contains("| 2 | `chat` | passed | +200 ms | +205 ms | 8/8 passed |"),
        "{markdown}"
    );
    assert!(
        markdown.contains(
            "```sh\nforge-emulator scenario run /home/qa/scenarios/ping.json --forge /home/qa/forge/target/debug/forge\n```"
        ),
        "{markdown}"
    );
    assert!(!markdown.contains("## Bugs"), "{markdown}");
    assert!(!markdown.contains("## forge output"), "{markdown}");
}

#[test]
fn interrupted_run_report_judges_nothing_a_bug() {
    let scenario = scenario();
    let mut steps = all_passed(&scenario);
    steps[STEP_CHAT].status = StepStatus::Interrupted;
    for expectation in &mut steps[STEP_CHAT].expectations {
        expectation.verdict = Verdict::NotEvaluated;
    }
    let mut outcome = outcome(&scenario, steps);
    outcome.verdict = ScenarioVerdict::Interrupted;
    let report = RunReport::new(context(), &scenario, &outcome).unwrap();

    assert!(report.bugs.is_empty(), "{:#?}", report.bugs);
    assert!(
        report
            .to_markdown()
            .contains("The run was interrupted, so nothing was judged a bug."),
        "{}",
        report.to_markdown()
    );
}

#[test]
fn verdict_line_names_the_scenario_and_counts_bugs() {
    let scenario = scenario();
    let failing = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CHAT,
        FailureCause::StreamClosed,
        Evidence::None,
    );
    let mut interrupted = outcome(&scenario, all_passed(&scenario));
    interrupted.verdict = ScenarioVerdict::Interrupted;
    for (outcome, expected) in [
        (
            outcome(&scenario, all_passed(&scenario)),
            "scenario `ping reaches forge`: passed",
        ),
        (
            outcome(&scenario, failing),
            "scenario `ping reaches forge`: FAILED, 1 bug",
        ),
        (interrupted, "scenario `ping reaches forge`: interrupted"),
    ] {
        let report = RunReport::new(context(), &scenario, &outcome).unwrap();
        assert_eq!(report.verdict_line(), expected);
    }
}

#[test]
fn repro_command_reruns_the_same_file_and_binary_with_a_non_default_log_filter() {
    let scenario = scenario();
    let outcome = outcome(&scenario, all_passed(&scenario));
    for (scenario_file, log, expected) in [
        (
            "/home/qa/scenarios/ping.json",
            DEFAULT_LOG_DIRECTIVES,
            "forge-emulator scenario run /home/qa/scenarios/ping.json --forge /home/qa/forge/target/debug/forge",
        ),
        (
            "/home/qa/my scenarios/ping.json",
            "info,forge::trigger=trace",
            "forge-emulator scenario run '/home/qa/my scenarios/ping.json' --forge /home/qa/forge/target/debug/forge --log info,forge::trigger=trace",
        ),
    ] {
        let run = RunContext {
            scenario_file: PathBuf::from(scenario_file),
            log_directives: log.to_owned(),
            ..context()
        };
        let report = RunReport::new(run, &scenario, &outcome).unwrap();
        assert_eq!(report.repro_command(), expected);
    }
}

#[test]
fn forge_output_tails_are_bounded_to_the_last_twenty_lines_each_clipped() {
    let scenario = scenario();
    let steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CHAT,
        FailureCause::StreamClosed,
        Evidence::None,
    );
    for (lines, omitted_note) in [(20, false), (21, true)] {
        let mut outcome = outcome(&scenario, steps.clone());
        let forge = outcome.forge.as_mut().unwrap();
        forge.stderr_tail = (1..=lines).map(|n| format!("stderr line {n}")).collect();
        forge.stderr_tail.push("x".repeat(401));
        forge.stderr_tail.remove(0);
        let markdown = RunReport::new(context(), &scenario, &outcome)
            .unwrap()
            .to_markdown();
        let stderr =
            &markdown[markdown.find("### stderr").unwrap()..markdown.find("### log file").unwrap()];

        assert!(!stderr.contains("stderr line 1\n"), "{stderr}");
        assert!(
            stderr.contains(&format!("stderr line {lines}\n")),
            "{stderr}"
        );
        assert!(
            stderr.contains(&format!("{}...\n", "x".repeat(400))),
            "{stderr}"
        );
        assert_eq!(
            stderr.contains("1 line earlier in `report.json`"),
            omitted_note,
            "{lines} lines: {stderr}"
        );
    }
}

#[test]
fn json_report_round_trips_and_re_renders_the_same_markdown() {
    let scenario = scenario();
    let steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_CAUSED,
        FailureCause::WrongCause {
            expected: id(CHAT_ID),
            actual: Some(id(COMMAND_ID)),
        },
        Evidence::Causation(CausationEvidence {
            effect: Some(at(
                1020,
                caused(
                    event(START_ID, EventSource::Core, "action.start", json!({})),
                    COMMAND_ID,
                ),
            )),
            cause: Some(at(1000, chat_event(CHAT_ID, "!ping"))),
            chain: Vec::new(),
        }),
    );
    let report = RunReport::new(context(), &scenario, &outcome(&scenario, steps)).unwrap();

    let json = report.to_json().unwrap();
    let back: RunReport = serde_json::from_str(&json).unwrap();

    assert_eq!(back.to_json().unwrap(), json);
    assert_eq!(back.to_markdown(), report.to_markdown());
}

#[test]
fn write_report_puts_both_files_in_the_run_directory() {
    let dir = tempfile::tempdir().unwrap();
    let scenario = scenario();
    let report = RunReport::new(
        context(),
        &scenario,
        &outcome(&scenario, all_passed(&scenario)),
    )
    .unwrap();

    let files = write_report(&report, dir.path()).unwrap();

    assert_eq!(files.markdown, dir.path().join(MARKDOWN_FILE));
    assert_eq!(
        std::fs::read_to_string(&files.markdown).unwrap(),
        report.to_markdown()
    );
    let written: RunReport =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(JSON_FILE)).unwrap())
            .unwrap();
    assert_eq!(written.to_json().unwrap(), report.to_json().unwrap());
}

#[test]
fn write_report_into_a_missing_directory_names_the_file_it_could_not_write() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("never-created");
    let scenario = scenario();
    let report = RunReport::new(
        context(),
        &scenario,
        &outcome(&scenario, all_passed(&scenario)),
    )
    .unwrap();

    let refusal = write_report(&report, &missing);

    assert!(
        matches!(&refusal, Err(EmulatorError::ReportWrite { path, .. }) if path.starts_with(&missing)),
        "{refusal:?}"
    );
}

/// The real seeder mints the bearer; the fixture carries the fake Twitch access token.
#[tokio::test]
async fn neither_seeded_token_appears_in_any_rendered_report() {
    let fixture = Fixture::chat_command_mvp();
    let data = tempfile::tempdir().unwrap();
    let seeded = seed(Path::new(EMULATOR), data.path(), &fixture)
        .await
        .unwrap();
    let bearer = seeded.server.bearer_token.clone();
    let access = fixture.twitch.clone().unwrap().access_token;

    let mut scenario = scenario();
    scenario.fixture = fixture.clone();
    scenario.purpose = format!("purpose mentioning {access}");
    let mut steps = one_failure(
        &scenario,
        STEP_CHAT,
        EXPECT_SUBSCRIPTION,
        FailureCause::NoSubscription,
        Evidence::Ledger(LedgerExcerpt {
            requests: vec![RecordedRequest {
                query: vec![("token".to_owned(), bearer.clone())],
                body: Some(json!({ "authorization": format!("Bearer {access}") })),
                ..request(
                    "POST",
                    "/helix/eventsub/subscriptions",
                    401,
                    CredentialCheck::WrongBearer,
                )
            }],
            ..LedgerExcerpt::default()
        }),
    );
    steps[STEP_CHAT].expectations[EXPECT_CHAT].verdict =
        Verdict::Failed(FailureCause::NotObserved {
            needed: 1,
            observed: 0,
        });
    steps[STEP_CHAT].expectations[EXPECT_CHAT].evidence = Evidence::Events(EventEvidence {
        near_misses: vec![NearMiss {
            event: at(1200, chat_event(START_ID, &format!("leaked {bearer}"))),
            mismatched: vec!["/message".to_owned()],
        }],
        ..EventEvidence::default()
    });
    let mut outcome = outcome(&scenario, steps);
    outcome.redactions = Redactions::for_run(&fixture, Some(&seeded));
    let forge = outcome.forge.as_mut().unwrap();
    forge.stderr_tail = vec![format!("auth token={bearer}")];
    forge.log_tail = vec![format!("DEBUG forge::twitch: access {access}")];

    let report = RunReport::new(context(), &scenario, &outcome).unwrap();
    let json = report.to_json().unwrap();
    let markdown = report.to_markdown();

    for (label, rendered) in [("json", &json), ("markdown", &markdown)] {
        assert!(!rendered.contains(&bearer), "{label} carries the bearer");
        assert!(
            !rendered.contains(&access),
            "{label} carries the access token"
        );
        assert!(rendered.contains(REDACTED), "{label} redacted nothing");
    }
}
