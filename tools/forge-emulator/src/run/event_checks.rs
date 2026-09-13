use std::collections::HashMap;

use forge_events::Event;
use forge_types::EventId;
use tokio::time::Instant;

use super::journal::{Journal, JournalEntry, JournalView};
use super::outcome::{
    CausationEvidence, EVIDENCE_LIMIT, EventEvidence, Evidence, FailureCause, Gap, GapKind,
    JournaledEvent, NearMiss, RunClock, Verdict,
};
use crate::control::Observation;
use crate::scenario::{AbsentEvent, Causation, EventPattern, ObservedCount, ObservedEvent};

const CHAIN_DEPTH: usize = 8;

/// Observations at `from` or later that arrived no later than `deadline` belong to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) from: usize,
    pub(crate) deadline: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct Assessment {
    pub(crate) verdict: Verdict,
    pub(crate) evidence: Evidence,
    /// The event a single-count expectation matched, for later causation checks.
    pub(crate) matched: Option<JournaledEvent>,
}

pub(crate) type NamedEvents = HashMap<String, JournaledEvent>;

pub(crate) async fn observe_event(
    journal: &Journal,
    window: Window,
    expected: &ObservedEvent,
    clock: RunClock,
) -> Assessment {
    let pattern = expected.pattern();
    journal
        .wait_until(window.deadline, |view| {
            let matched = matches_in_window(&view, window, &pattern);
            match expected.count {
                ObservedCount::AtLeast(needed) => matched >= needed as usize,
                ObservedCount::Exactly(expected) => matched > expected as usize,
            }
        })
        .await;
    journal.read(|view| assess_event(&view, window, expected, clock))
}

pub(crate) async fn observe_absence(
    journal: &Journal,
    window: Window,
    absent: &AbsentEvent,
    clock: RunClock,
) -> Assessment {
    let pattern = absent.pattern();
    journal
        .wait_until(window.deadline, |view| {
            matches_in_window(&view, window, &pattern) > 0
        })
        .await;
    journal.read(|view| assess_absence(&view, window, absent, clock))
}

pub(crate) fn assess_event(
    view: &JournalView<'_>,
    window: Window,
    expected: &ObservedEvent,
    clock: RunClock,
) -> Assessment {
    let scan = scan(view, window, &expected.pattern(), clock);
    let observed = scan.matched.len();
    let closed_early = closed_before(view, window);
    let verdict = match expected.count {
        ObservedCount::AtLeast(needed) if observed >= needed as usize => Verdict::Passed,
        ObservedCount::AtLeast(needed) => scan
            .gap_cause()
            .or_else(|| closed_early.then_some(FailureCause::StreamClosed))
            .map_or(
                Verdict::Failed(FailureCause::NotObserved { needed, observed }),
                Verdict::Failed,
            ),
        ObservedCount::Exactly(expected) if observed > expected as usize => {
            Verdict::Failed(FailureCause::WrongCount { expected, observed })
        }
        ObservedCount::Exactly(expected) => match (scan.gap_cause(), closed_early) {
            (Some(gap), _) => Verdict::Failed(gap),
            (None, true) => Verdict::Failed(FailureCause::StreamClosed),
            (None, false) if observed == expected as usize => Verdict::Passed,
            (None, false) => Verdict::Failed(FailureCause::WrongCount { expected, observed }),
        },
    };
    let matched = (verdict == Verdict::Passed && expected.count.is_single())
        .then(|| scan.matched.first().cloned())
        .flatten();
    Assessment {
        verdict,
        evidence: Evidence::Events(scan.into_evidence()),
        matched,
    }
}

pub(crate) fn assess_absence(
    view: &JournalView<'_>,
    window: Window,
    absent: &AbsentEvent,
    clock: RunClock,
) -> Assessment {
    let scan = scan(view, window, &absent.pattern(), clock);
    let verdict = if !scan.matched.is_empty() {
        Verdict::Failed(FailureCause::Present {
            observed: scan.matched.len(),
        })
    } else if let Some(gap) = scan.gap_cause() {
        Verdict::Failed(gap)
    } else if closed_before(view, window) {
        Verdict::Failed(FailureCause::StreamClosed)
    } else {
        Verdict::Passed
    };
    Assessment {
        verdict,
        evidence: Evidence::Events(scan.into_evidence()),
        matched: None,
    }
}

pub(crate) fn assess_causation(
    view: &JournalView<'_>,
    named: &NamedEvents,
    causation: &Causation,
    clock: RunClock,
) -> Assessment {
    let effect = named.get(&causation.effect).cloned();
    let cause = named.get(&causation.cause).cloned();
    let chain = effect
        .as_ref()
        .map(|effect| ancestors(view, &effect.event, clock))
        .unwrap_or_default();
    let verdict = match (&effect, &cause) {
        (None, _) => Verdict::Failed(FailureCause::UnresolvedName {
            name: causation.effect.clone(),
        }),
        (_, None) => Verdict::Failed(FailureCause::UnresolvedName {
            name: causation.cause.clone(),
        }),
        (Some(effect), Some(cause)) if effect.event.caused_by == Some(cause.event.id) => {
            Verdict::Passed
        }
        (Some(effect), Some(cause)) => Verdict::Failed(FailureCause::WrongCause {
            expected: cause.event.id,
            actual: effect.event.caused_by,
        }),
    };
    Assessment {
        verdict,
        evidence: Evidence::Causation(CausationEvidence {
            effect,
            cause,
            chain,
        }),
        matched: None,
    }
}

fn ancestors(view: &JournalView<'_>, effect: &Event, clock: RunClock) -> Vec<JournaledEvent> {
    let mut chain: Vec<JournaledEvent> = Vec::new();
    let mut next = effect.caused_by;
    while let Some(id) = next {
        if chain.len() == CHAIN_DEPTH || chain.iter().any(|known| known.event.id == id) {
            break;
        }
        let Some(found) = find_event(view, id, clock) else {
            break;
        };
        next = found.event.caused_by;
        chain.push(found);
    }
    chain
}

fn find_event(view: &JournalView<'_>, id: EventId, clock: RunClock) -> Option<JournaledEvent> {
    view.entries
        .iter()
        .find_map(|entry| match &entry.observation {
            Observation::Event(event) if event.id == id => Some(journaled(entry, event, clock)),
            _ => None,
        })
}

fn journaled(entry: &JournalEntry, event: &Event, clock: RunClock) -> JournaledEvent {
    JournaledEvent {
        arrived_ms: clock.millis(entry.arrived),
        event: event.clone(),
    }
}

fn scoped<'v>(view: &JournalView<'v>, window: Window) -> &'v [JournalEntry] {
    view.entries.get(window.from..).unwrap_or_default()
}

fn matches_in_window(view: &JournalView<'_>, window: Window, pattern: &EventPattern<'_>) -> usize {
    scoped(view, window)
        .iter()
        .filter(|entry| entry.arrived <= window.deadline)
        .filter(|entry| matches!(&entry.observation, Observation::Event(event) if pattern.matches(event)))
        .count()
}

fn closed_before(view: &JournalView<'_>, window: Window) -> bool {
    view.closed_at
        .is_some_and(|closed| closed < window.deadline)
}

#[derive(Default)]
struct Scan {
    matched: Vec<JournaledEvent>,
    late: Vec<JournaledEvent>,
    near_misses: Vec<NearMiss>,
    gaps: Vec<Gap>,
}

impl Scan {
    fn gap_cause(&self) -> Option<FailureCause> {
        if self.gaps.is_empty() {
            return None;
        }
        let dropped = self
            .gaps
            .iter()
            .map(|gap| match gap.kind {
                GapKind::Dropped(count) => count,
                GapKind::Undecodable { .. } => 0,
            })
            .sum();
        let undecodable = self
            .gaps
            .iter()
            .filter(|gap| matches!(gap.kind, GapKind::Undecodable { .. }))
            .count();
        Some(FailureCause::StreamGap {
            dropped,
            undecodable,
        })
    }

    fn into_evidence(self) -> EventEvidence {
        let matched = self.matched.len();
        let mut samples = self.matched;
        samples.truncate(EVIDENCE_LIMIT);
        let mut late = self.late;
        late.truncate(EVIDENCE_LIMIT);
        EventEvidence {
            matched,
            samples,
            late,
            near_misses: self.near_misses,
            gaps: self.gaps,
        }
    }
}

/// Every gap after `from` counts, even past the deadline: a drop notice trails the events it lost.
fn scan(
    view: &JournalView<'_>,
    window: Window,
    pattern: &EventPattern<'_>,
    clock: RunClock,
) -> Scan {
    let mut scan = Scan::default();
    for entry in scoped(view, window) {
        let arrived_ms = clock.millis(entry.arrived);
        match &entry.observation {
            Observation::Event(event) if pattern.matches(event) => {
                let found = journaled(entry, event, clock);
                if entry.arrived <= window.deadline {
                    scan.matched.push(found);
                } else {
                    scan.late.push(found);
                }
            }
            Observation::Event(event) if event.kind == pattern.kind => {
                if scan.near_misses.len() < EVIDENCE_LIMIT {
                    scan.near_misses.push(NearMiss {
                        event: journaled(entry, event, clock),
                        mismatched: mismatches(pattern, event),
                    });
                }
            }
            Observation::Event(_) => {}
            Observation::Dropped(count) => scan.gaps.push(Gap {
                arrived_ms,
                kind: GapKind::Dropped(*count),
            }),
            Observation::Undecodable { frame, reason } => scan.gaps.push(Gap {
                arrived_ms,
                kind: GapKind::Undecodable {
                    frame: frame.clone(),
                    reason: reason.clone(),
                },
            }),
        }
    }
    scan
}

fn mismatches(pattern: &EventPattern<'_>, event: &Event) -> Vec<String> {
    let source = pattern
        .source
        .filter(|source| *source != event.source)
        .map(|_| "source".to_owned());
    let payload = pattern
        .payload
        .0
        .iter()
        .filter(|(pointer, matcher)| !matcher.matches(event.payload.pointer(pointer)))
        .map(|(pointer, _)| pointer.clone());
    source.into_iter().chain(payload).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use forge_events::EventSource;
    use serde_json::{Value, json};

    use super::*;

    const WINDOW_MS: u64 = 1_000;

    fn chat(text: &str) -> Observation {
        Observation::Event(Event::new(
            EventSource::Twitch,
            "chat.message",
            json!({ "message": text, "user": { "login": "alice" } }),
        ))
    }

    fn dropped(count: u64) -> Observation {
        Observation::Dropped(count)
    }

    fn garbled() -> Observation {
        Observation::Undecodable {
            frame: "{".to_owned(),
            reason: "EOF".to_owned(),
        }
    }

    fn observed(spec: Value) -> ObservedEvent {
        serde_json::from_value(spec).unwrap()
    }

    fn absent(spec: Value) -> AbsentEvent {
        serde_json::from_value(spec).unwrap()
    }

    fn ms(count: u64) -> Duration {
        Duration::from_millis(count)
    }

    struct Timeline {
        base: Instant,
        entries: Vec<JournalEntry>,
        closed_at: Option<Instant>,
    }

    impl Timeline {
        fn new(entries: Vec<(u64, Observation)>) -> Self {
            let base = Instant::now();
            Self {
                base,
                entries: entries
                    .into_iter()
                    .map(|(offset, observation)| JournalEntry {
                        arrived: base + ms(offset),
                        observation,
                    })
                    .collect(),
                closed_at: None,
            }
        }

        fn closed_at(mut self, offset: u64) -> Self {
            self.closed_at = Some(self.base + ms(offset));
            self
        }

        fn view(&self) -> JournalView<'_> {
            JournalView {
                entries: &self.entries,
                closed_at: self.closed_at,
            }
        }

        fn window(&self, from: usize) -> Window {
            Window {
                from,
                deadline: self.base + ms(WINDOW_MS),
            }
        }

        fn clock(&self) -> RunClock {
            RunClock::starting_now()
        }
    }

    fn chat_expectation(count: Value) -> ObservedEvent {
        observed(json!({
            "source": "twitch",
            "kind": "chat.message",
            "payload": { "/message": { "equals": "!ping" } },
            "count": count,
            "within_ms": WINDOW_MS,
        }))
    }

    fn assessed(timeline: &Timeline, from: usize, expected: &ObservedEvent) -> Assessment {
        assess_event(
            &timeline.view(),
            timeline.window(from),
            expected,
            timeline.clock(),
        )
    }

    #[test]
    fn at_least_passes_on_matches_arriving_by_the_deadline_and_names_the_gap_otherwise() {
        let cases: Vec<(&str, u32, Timeline, usize, Verdict)> = vec![
            (
                "match exactly at the deadline",
                1,
                Timeline::new(vec![(WINDOW_MS, chat("!ping"))]),
                0,
                Verdict::Passed,
            ),
            (
                "match one millisecond late",
                1,
                Timeline::new(vec![(WINDOW_MS + 1, chat("!ping"))]),
                0,
                Verdict::Failed(FailureCause::NotObserved {
                    needed: 1,
                    observed: 0,
                }),
            ),
            (
                "one match short of the count",
                2,
                Timeline::new(vec![(10, chat("!ping"))]),
                0,
                Verdict::Failed(FailureCause::NotObserved {
                    needed: 2,
                    observed: 1,
                }),
            ),
            (
                "match belongs to an earlier step",
                1,
                Timeline::new(vec![(10, chat("!ping")), (20, chat("other"))]),
                1,
                Verdict::Failed(FailureCause::NotObserved {
                    needed: 1,
                    observed: 0,
                }),
            ),
            (
                "server dropped events and nothing matched",
                1,
                Timeline::new(vec![(10, dropped(3))]),
                0,
                Verdict::Failed(FailureCause::StreamGap {
                    dropped: 3,
                    undecodable: 0,
                }),
            ),
            (
                "a drop cannot fabricate a match that did arrive",
                1,
                Timeline::new(vec![(10, dropped(3)), (20, chat("!ping"))]),
                0,
                Verdict::Passed,
            ),
            (
                "connection closed before the deadline",
                1,
                Timeline::new(vec![]).closed_at(100),
                0,
                Verdict::Failed(FailureCause::StreamClosed),
            ),
        ];
        for (label, needed, timeline, from, expected) in cases {
            let expectation = chat_expectation(json!({ "at_least": needed }));
            let assessment = assessed(&timeline, from, &expectation);
            assert_eq!(assessment.verdict, expected, "{label}");
        }
    }

    #[test]
    fn exactly_passes_only_on_the_exact_count_with_an_unbroken_open_stream() {
        let cases: Vec<(&str, Timeline, Verdict)> = vec![
            (
                "exact count",
                Timeline::new(vec![(10, chat("!ping")), (WINDOW_MS, chat("!ping"))]),
                Verdict::Passed,
            ),
            (
                "one too many",
                Timeline::new(vec![
                    (10, chat("!ping")),
                    (20, chat("!ping")),
                    (30, chat("!ping")),
                ]),
                Verdict::Failed(FailureCause::WrongCount {
                    expected: 2,
                    observed: 3,
                }),
            ),
            (
                "one too few",
                Timeline::new(vec![(10, chat("!ping"))]),
                Verdict::Failed(FailureCause::WrongCount {
                    expected: 2,
                    observed: 1,
                }),
            ),
            (
                "the extra match arrives after the deadline",
                Timeline::new(vec![
                    (10, chat("!ping")),
                    (20, chat("!ping")),
                    (WINDOW_MS + 1, chat("!ping")),
                ]),
                Verdict::Passed,
            ),
            (
                "exact count but a frame was garbled",
                Timeline::new(vec![
                    (10, chat("!ping")),
                    (15, garbled()),
                    (20, chat("!ping")),
                ]),
                Verdict::Failed(FailureCause::StreamGap {
                    dropped: 0,
                    undecodable: 1,
                }),
            ),
            (
                "exact count but a drop notice trails the deadline",
                Timeline::new(vec![
                    (10, chat("!ping")),
                    (20, chat("!ping")),
                    (WINDOW_MS + 5, dropped(1)),
                ]),
                Verdict::Failed(FailureCause::StreamGap {
                    dropped: 1,
                    undecodable: 0,
                }),
            ),
            (
                "too many is decisive even across a drop",
                Timeline::new(vec![
                    (10, chat("!ping")),
                    (20, dropped(4)),
                    (30, chat("!ping")),
                    (40, chat("!ping")),
                ]),
                Verdict::Failed(FailureCause::WrongCount {
                    expected: 2,
                    observed: 3,
                }),
            ),
            (
                "exact count but the connection closed inside the window",
                Timeline::new(vec![(10, chat("!ping")), (20, chat("!ping"))]).closed_at(500),
                Verdict::Failed(FailureCause::StreamClosed),
            ),
            (
                "exact count and the connection closed after the window",
                Timeline::new(vec![(10, chat("!ping")), (20, chat("!ping"))])
                    .closed_at(WINDOW_MS + 1),
                Verdict::Passed,
            ),
        ];
        let expectation = chat_expectation(json!({ "exactly": 2 }));
        for (label, timeline, expected) in cases {
            let assessment = assessed(&timeline, 0, &expectation);
            assert_eq!(assessment.verdict, expected, "{label}");
        }
    }

    #[test]
    fn failed_match_reports_late_matches_and_same_kind_near_misses_with_their_mismatches() {
        let wrong_source = Observation::Event(Event::new(
            EventSource::Kick,
            "chat.message",
            json!({ "message": "!ping" }),
        ));
        let other_kind = Observation::Event(Event::new(
            EventSource::Twitch,
            "command.matched",
            json!({ "message": "!ping" }),
        ));
        let timeline = Timeline::new(vec![
            (10, chat("!pong")),
            (20, wrong_source),
            (30, other_kind),
            (WINDOW_MS + 50, chat("!ping")),
        ]);
        let assessment = assessed(&timeline, 0, &chat_expectation(json!({ "at_least": 1 })));

        let Evidence::Events(evidence) = assessment.evidence else {
            panic!("event evidence expected");
        };
        let mismatches: Vec<Vec<String>> = evidence
            .near_misses
            .iter()
            .map(|miss| miss.mismatched.clone())
            .collect();
        assert_eq!(
            mismatches,
            [vec!["/message".to_owned()], vec!["source".to_owned()]]
        );
        assert_eq!(evidence.late.len(), 1);
    }

    #[test]
    fn single_count_pass_names_the_first_match() {
        let timeline = Timeline::new(vec![(10, chat("!ping")), (20, chat("!ping"))]);
        let first = match &timeline.entries[0].observation {
            Observation::Event(event) => event.id,
            other => panic!("{other:?}"),
        };
        let assessment = assessed(&timeline, 0, &chat_expectation(json!({ "at_least": 1 })));
        assert_eq!(assessment.matched.map(|found| found.event.id), Some(first));
    }

    #[test]
    fn absence_holds_only_over_a_complete_window_without_a_match() {
        let pattern = absent(json!({
            "kind": "chat.message",
            "payload": { "/message": { "equals": "!ping" } },
            "window_ms": WINDOW_MS,
        }));
        let cases: Vec<(&str, Timeline, usize, Verdict)> = vec![
            (
                "nothing observed",
                Timeline::new(vec![(10, chat("other"))]),
                0,
                Verdict::Passed,
            ),
            (
                "match inside the window",
                Timeline::new(vec![(WINDOW_MS, chat("!ping"))]),
                0,
                Verdict::Failed(FailureCause::Present { observed: 1 }),
            ),
            (
                "match after the window",
                Timeline::new(vec![(WINDOW_MS + 1, chat("!ping"))]),
                0,
                Verdict::Passed,
            ),
            (
                "match from an earlier step",
                Timeline::new(vec![(10, chat("!ping"))]),
                1,
                Verdict::Passed,
            ),
            (
                "server dropped events",
                Timeline::new(vec![(10, dropped(2))]),
                0,
                Verdict::Failed(FailureCause::StreamGap {
                    dropped: 2,
                    undecodable: 0,
                }),
            ),
            (
                "connection closed inside the window",
                Timeline::new(vec![]).closed_at(10),
                0,
                Verdict::Failed(FailureCause::StreamClosed),
            ),
        ];
        for (label, timeline, from, expected) in cases {
            let assessment = assess_absence(
                &timeline.view(),
                timeline.window(from),
                &pattern,
                timeline.clock(),
            );
            assert_eq!(assessment.verdict, expected, "{label}");
        }
    }

    fn event_with_cause(kind: &str, caused_by: Option<EventId>) -> Event {
        let mut event = Event::new(EventSource::Core, kind, json!({}));
        event.caused_by = caused_by;
        event
    }

    fn named(pairs: &[(&str, &Event)]) -> NamedEvents {
        pairs
            .iter()
            .map(|(name, event)| {
                (
                    (*name).to_owned(),
                    JournaledEvent {
                        arrived_ms: 0,
                        event: (*event).clone(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn causation_requires_the_effect_to_carry_the_cause_id_directly() {
        let chat = event_with_cause("chat.message", None);
        let matched = event_with_cause("command.matched", Some(chat.id));
        let started = event_with_cause("action.start", Some(matched.id));
        let uncaused = event_with_cause("action.start", None);
        let causation = |effect: &str, cause: &str| Causation {
            effect: effect.to_owned(),
            cause: cause.to_owned(),
        };
        let cases = [
            (causation("matched", "chat"), Verdict::Passed),
            (
                causation("started", "chat"),
                Verdict::Failed(FailureCause::WrongCause {
                    expected: chat.id,
                    actual: Some(matched.id),
                }),
            ),
            (
                causation("uncaused", "chat"),
                Verdict::Failed(FailureCause::WrongCause {
                    expected: chat.id,
                    actual: None,
                }),
            ),
            (
                causation("missing", "chat"),
                Verdict::Failed(FailureCause::UnresolvedName {
                    name: "missing".to_owned(),
                }),
            ),
            (
                causation("matched", "missing"),
                Verdict::Failed(FailureCause::UnresolvedName {
                    name: "missing".to_owned(),
                }),
            ),
        ];
        let names = named(&[
            ("chat", &chat),
            ("matched", &matched),
            ("started", &started),
            ("uncaused", &uncaused),
        ]);
        let timeline = Timeline::new(vec![]);
        for (causation, expected) in cases {
            let assessment =
                assess_causation(&timeline.view(), &names, &causation, timeline.clock());
            assert_eq!(assessment.verdict, expected, "{causation:?}");
        }
    }

    #[test]
    fn causation_evidence_walks_the_observed_ancestors_nearest_first() {
        let chat = event_with_cause("chat.message", None);
        let matched = event_with_cause("command.matched", Some(chat.id));
        let started = event_with_cause("action.start", Some(matched.id));
        let timeline = Timeline::new(vec![
            (10, Observation::Event(chat.clone())),
            (20, Observation::Event(matched.clone())),
            (30, Observation::Event(started.clone())),
        ]);
        let names = named(&[("started", &started), ("chat", &chat)]);
        let causation = Causation {
            effect: "started".to_owned(),
            cause: "chat".to_owned(),
        };
        let assessment = assess_causation(&timeline.view(), &names, &causation, timeline.clock());

        let Evidence::Causation(evidence) = assessment.evidence else {
            panic!("causation evidence expected");
        };
        let chain: Vec<EventId> = evidence.chain.iter().map(|link| link.event.id).collect();
        assert_eq!(chain, [matched.id, chat.id]);
    }

    fn record_after(journal: &Journal, delay: Duration, observation: Observation) {
        let journal = journal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            journal.record(observation);
        });
    }

    fn live_window(journal: &Journal, started: Instant) -> Window {
        Window {
            from: journal.len(),
            deadline: started + ms(WINDOW_MS),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn at_least_returns_as_soon_as_its_count_is_reached() {
        let journal = Journal::new();
        let started = Instant::now();
        let window = live_window(&journal, started);
        record_after(&journal, ms(10), chat("!ping"));

        let assessment = observe_event(
            &journal,
            window,
            &chat_expectation(json!({ "at_least": 1 })),
            RunClock::starting_now(),
        )
        .await;

        assert_eq!(assessment.verdict, Verdict::Passed);
        assert!(
            started.elapsed() < ms(WINDOW_MS),
            "waited {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn exactly_waits_out_the_full_window_before_passing() {
        let journal = Journal::new();
        let started = Instant::now();
        let window = live_window(&journal, started);
        journal.record(chat("!ping"));

        let assessment = observe_event(
            &journal,
            window,
            &chat_expectation(json!({ "exactly": 1 })),
            RunClock::starting_now(),
        )
        .await;

        assert_eq!(assessment.verdict, Verdict::Passed);
        assert!(
            started.elapsed() >= ms(WINDOW_MS),
            "waited {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn exactly_fails_on_a_surplus_match_just_before_the_deadline() {
        let journal = Journal::new();
        let window = live_window(&journal, Instant::now());
        journal.record(chat("!ping"));
        record_after(&journal, ms(WINDOW_MS - 1), chat("!ping"));

        let assessment = observe_event(
            &journal,
            window,
            &chat_expectation(json!({ "exactly": 1 })),
            RunClock::starting_now(),
        )
        .await;

        assert_eq!(
            assessment.verdict,
            Verdict::Failed(FailureCause::WrongCount {
                expected: 1,
                observed: 2
            })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn absence_waits_out_the_full_window_before_passing() {
        let journal = Journal::new();
        let started = Instant::now();
        let pattern = absent(json!({ "kind": "trigger.blocked", "window_ms": WINDOW_MS }));

        let assessment = observe_absence(
            &journal,
            live_window(&journal, started),
            &pattern,
            RunClock::starting_now(),
        )
        .await;

        assert_eq!(assessment.verdict, Verdict::Passed);
        assert!(
            started.elapsed() >= ms(WINDOW_MS),
            "waited {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn absence_fails_on_a_match_in_the_last_millisecond_of_the_window() {
        let journal = Journal::new();
        let window = live_window(&journal, Instant::now());
        let pattern = absent(json!({ "kind": "chat.message", "window_ms": WINDOW_MS }));
        record_after(&journal, ms(WINDOW_MS - 1), chat("late"));

        let assessment =
            observe_absence(&journal, window, &pattern, RunClock::starting_now()).await;

        assert_eq!(
            assessment.verdict,
            Verdict::Failed(FailureCause::Present { observed: 1 })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_closing_journal_ends_the_wait_as_a_closed_stream() {
        let journal = Journal::new();
        let started = Instant::now();
        let window = live_window(&journal, started);
        let closer = journal.clone();
        tokio::spawn(async move {
            tokio::time::sleep(ms(10)).await;
            closer.close();
        });

        let assessment = observe_event(
            &journal,
            window,
            &chat_expectation(json!({ "at_least": 1 })),
            RunClock::starting_now(),
        )
        .await;

        assert_eq!(
            assessment.verdict,
            Verdict::Failed(FailureCause::StreamClosed)
        );
        assert!(
            started.elapsed() < ms(WINDOW_MS),
            "waited {:?}",
            started.elapsed()
        );
    }
}
