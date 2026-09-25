use std::collections::{BTreeMap, HashMap};

use forge_events::{Event, EventSource};
use forge_types::EventId;
use serde_json::Value;
use tokio::time::Instant;

use super::marker::find_marker;

const ACTION_START: &str = "action.start";
const ACTION_DONE: &str = "action.done";
const ACTION_SKIPPED: &str = "action.skipped";
const ACTION_NAME_POINTER: &str = "/action_name";
const OUTCOME_POINTER: &str = "/outcome";
const REASON_POINTER: &str = "/reason";
const ACTION_ID_POINTER: &str = "/action_id";
const DROPPED_KEY: &str = "dropped";

/// Latencies are kept per phase, measured from the moment the generator injected the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Leg {
    /// Twitch event published by forge.
    Ingest,
    Start,
    Done,
    /// A connected page received the frame; the index names the page.
    Frame(usize),
    /// forge's chat reply reached the fake Helix.
    Reply,
}

#[derive(Debug, Clone, Copy)]
struct Injected {
    at: Instant,
    phase: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionCounts {
    pub started: u64,
    pub done: u64,
    pub failed: u64,
    pub skipped: BTreeMap<String, u64>,
}

impl ActionCounts {
    pub fn skipped_total(&self) -> u64 {
        self.skipped.values().sum()
    }
}

/// Everything the harness saw, correlated back to the injection that caused it.
#[derive(Debug)]
pub struct Tracker {
    actions: Vec<String>,
    /// For each stimulus, the action indexes each of its events should start.
    runs: Vec<Vec<usize>>,
    injected: Vec<Injected>,
    injected_per_stimulus: Vec<u64>,
    cause: HashMap<EventId, u64>,
    started: HashMap<EventId, (u64, usize)>,
    action_ids: HashMap<String, usize>,
    counts: Vec<ActionCounts>,
    latencies: HashMap<(usize, usize, Leg), Vec<u32>>,
    pub twitch_events: u64,
    pub uncorrelated: u64,
    pub observer_dropped: u64,
    pub frames: Vec<u64>,
    /// Frames forge's server announced it dropped for each page because the page fell behind.
    pub page_dropped: Vec<u64>,
    pub replies: u64,
    /// Every other observed kind, by kind.
    pub other_kinds: BTreeMap<String, u64>,
}

impl Tracker {
    pub fn new(actions: Vec<String>, runs: Vec<Vec<usize>>, pages: usize) -> Self {
        let count = actions.len();
        let stimuli = runs.len();
        Self {
            actions,
            runs,
            injected: Vec::new(),
            injected_per_stimulus: vec![0; stimuli],
            cause: HashMap::new(),
            started: HashMap::new(),
            action_ids: HashMap::new(),
            counts: vec![ActionCounts::default(); count],
            latencies: HashMap::new(),
            twitch_events: 0,
            uncorrelated: 0,
            observer_dropped: 0,
            frames: vec![0; pages],
            page_dropped: vec![0; pages],
            replies: 0,
            other_kinds: BTreeMap::new(),
        }
    }

    /// Sequence numbers must arrive in order starting at 0; the generator is the only caller.
    pub fn injected(&mut self, seq: u64, stimulus: usize, phase: usize, at: Instant) {
        debug_assert_eq!(seq, self.injected.len() as u64);
        self.injected.push(Injected { at, phase });
        if let Some(count) = self.injected_per_stimulus.get_mut(stimulus) {
            *count += 1;
        }
    }

    pub fn injected_total(&self) -> u64 {
        self.injected.len() as u64
    }

    pub fn on_event(&mut self, event: &Event, arrived: Instant) {
        if event.source == EventSource::Twitch {
            self.twitch_events += 1;
            if let Some(seq) = marker_in(&event.payload) {
                self.cause.insert(event.id, seq);
                self.record(seq, Leg::Ingest, arrived);
            }
            return;
        }
        match event.kind.as_str() {
            ACTION_START => self.on_start(event, arrived),
            ACTION_DONE => self.on_done(event, arrived),
            ACTION_SKIPPED => self.on_skipped(event),
            kind => {
                *self.other_kinds.entry(kind.to_owned()).or_default() += 1;
                if let Some(seq) = event
                    .caused_by
                    .and_then(|cause| self.cause.get(&cause).copied())
                {
                    self.cause.insert(event.id, seq);
                }
            }
        }
    }

    fn on_start(&mut self, event: &Event, arrived: Instant) {
        let Some(action) = event
            .payload
            .pointer(ACTION_NAME_POINTER)
            .and_then(Value::as_str)
            .and_then(|name| self.action_index(name))
        else {
            return;
        };
        if let Some(id) = event
            .payload
            .pointer(ACTION_ID_POINTER)
            .and_then(Value::as_str)
        {
            self.action_ids.insert(id.to_owned(), action);
        }
        self.counts[action].started += 1;
        let Some(seq) = event
            .caused_by
            .and_then(|cause| self.cause.get(&cause).copied())
        else {
            self.uncorrelated += 1;
            return;
        };
        self.started.insert(event.id, (seq, action));
        self.record_for(seq, action, Leg::Start, arrived);
    }

    fn on_done(&mut self, event: &Event, arrived: Instant) {
        let Some((seq, action)) = event
            .caused_by
            .and_then(|start| self.started.remove(&start))
        else {
            self.uncorrelated += 1;
            return;
        };
        let counts = &mut self.counts[action];
        counts.done += 1;
        if event
            .payload
            .pointer(OUTCOME_POINTER)
            .and_then(Value::as_str)
            != Some("success")
        {
            counts.failed += 1;
        }
        self.record_for(seq, action, Leg::Done, arrived);
    }

    fn on_skipped(&mut self, event: &Event) {
        let Some(action) = event
            .payload
            .pointer(ACTION_ID_POINTER)
            .and_then(Value::as_str)
            .and_then(|id| self.action_ids.get(id).copied())
        else {
            self.uncorrelated += 1;
            return;
        };
        let reason = event
            .payload
            .pointer(REASON_POINTER)
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        *self.counts[action].skipped.entry(reason).or_default() += 1;
    }

    pub fn on_frame(&mut self, page: usize, frame: &Value, arrived: Instant) {
        if let Some(dropped) = frame.get(DROPPED_KEY).and_then(Value::as_u64) {
            if let Some(count) = self.page_dropped.get_mut(page) {
                *count += dropped;
            }
            return;
        }
        let Some(seq) = marker_in(frame) else {
            return;
        };
        if let Some(count) = self.frames.get_mut(page) {
            *count += 1;
        }
        self.record(seq, Leg::Frame(page), arrived);
    }

    pub fn on_reply(&mut self, body: Option<&Value>, arrived: Instant) {
        let Some(seq) = body.and_then(marker_in) else {
            return;
        };
        self.replies += 1;
        self.record(seq, Leg::Reply, arrived);
    }

    /// Learns an action's id from outside the bus, so a skip that precedes its first start
    /// still lands on the right action.
    pub fn learn_action_id(&mut self, id: String, name: &str) {
        if let Some(action) = self.action_index(name) {
            self.action_ids.insert(id, action);
        }
    }

    pub fn action_position(&self, name: &str) -> Option<usize> {
        self.action_index(name)
    }

    fn action_index(&self, name: &str) -> Option<usize> {
        self.actions.iter().position(|action| action == name)
    }

    fn record(&mut self, seq: u64, leg: Leg, arrived: Instant) {
        self.record_for(seq, usize::MAX, leg, arrived);
    }

    fn record_for(&mut self, seq: u64, action: usize, leg: Leg, arrived: Instant) {
        let Some(injected) = usize::try_from(seq).ok().and_then(|i| self.injected.get(i)) else {
            self.uncorrelated += 1;
            return;
        };
        let millis = arrived.saturating_duration_since(injected.at).as_millis();
        self.latencies
            .entry((injected.phase, action, leg))
            .or_default()
            .push(u32::try_from(millis).unwrap_or(u32::MAX));
    }

    pub fn latencies(&self, phase: usize, action: Option<usize>, leg: Leg) -> Option<&[u32]> {
        self.latencies
            .get(&(phase, action.unwrap_or(usize::MAX), leg))
            .map(Vec::as_slice)
    }

    pub fn counts(&self, action: usize) -> &ActionCounts {
        &self.counts[action]
    }

    /// Events injected that should have started `action`, less the ones that did or were skipped:
    /// work still queued, plus work forge dropped without a trace.
    pub fn unstarted(&self, action: usize) -> u64 {
        let expected = self.expected(action);
        let counts = &self.counts[action];
        expected.saturating_sub(counts.started + counts.skipped_total())
    }

    pub fn in_flight(&self, action: usize) -> u64 {
        let counts = &self.counts[action];
        counts.started.saturating_sub(counts.done)
    }

    pub fn expected(&self, action: usize) -> u64 {
        self.runs
            .iter()
            .zip(&self.injected_per_stimulus)
            .filter(|(runs, _)| runs.contains(&action))
            .map(|(_, injected)| *injected)
            .sum()
    }

    pub fn injected_of(&self, stimulus: usize) -> u64 {
        self.injected_per_stimulus
            .get(stimulus)
            .copied()
            .unwrap_or(0)
    }
}

fn marker_in(value: &Value) -> Option<u64> {
    match value {
        Value::String(text) => find_marker(text),
        Value::Array(items) => items.iter().find_map(marker_in),
        Value::Object(fields) => fields.values().find_map(marker_in),
        _ => None,
    }
}

/// Nearest-rank percentile of an unsorted sample; `None` when it is empty.
pub fn percentile(samples: &[u32], pct: f64) -> Option<u32> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((pct / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted.get(rank.clamp(1, sorted.len()) - 1).copied()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    const CHAT_ACTION: usize = 0;
    const COMMAND_ACTION: usize = 1;

    /// Stimulus 0 (chat) runs the chat action; stimulus 1 (command) runs both.
    fn tracker() -> Tracker {
        Tracker::new(
            vec!["Chat Message".to_owned(), "Hype Command".to_owned()],
            vec![vec![CHAT_ACTION], vec![CHAT_ACTION, COMMAND_ACTION]],
            1,
        )
    }

    fn twitch_event(text: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            json!({ "message": { "text": text } }),
        )
    }

    fn core(kind: &str, payload: Value, cause: EventId) -> Event {
        Event::caused_by(EventSource::Core, kind, payload, cause)
    }

    fn start(name: &str, cause: EventId) -> Event {
        core(
            ACTION_START,
            json!({ "action_name": name, "action_id": format!("id-{name}") }),
            cause,
        )
    }

    fn done(cause: EventId, outcome: &str) -> Event {
        core(ACTION_DONE, json!({ "outcome": outcome }), cause)
    }

    #[test]
    fn a_full_chain_is_timed_from_injection_on_every_leg_in_the_injected_phase() {
        let mut tracker = tracker();
        let t0 = Instant::now();
        tracker.injected(0, 0, 3, t0);
        let chat = twitch_event("hi ~q0~");
        tracker.on_event(&chat, t0 + Duration::from_millis(5));
        let started = start("Chat Message", chat.id);
        tracker.on_event(&started, t0 + Duration::from_millis(20));
        tracker.on_event(&done(started.id, "success"), t0 + Duration::from_millis(70));
        tracker.on_frame(
            0,
            &json!({ "content": { "message": "hi ~q0~" } }),
            t0 + Duration::from_millis(90),
        );

        let leg = |action, leg| tracker.latencies(3, action, leg).map(<[u32]>::to_vec);
        assert_eq!(leg(None, Leg::Ingest), Some(vec![5]));
        assert_eq!(leg(Some(CHAT_ACTION), Leg::Start), Some(vec![20]));
        assert_eq!(leg(Some(CHAT_ACTION), Leg::Done), Some(vec![70]));
        assert_eq!(leg(None, Leg::Frame(0)), Some(vec![90]));
        assert_eq!(tracker.uncorrelated, 0);
    }

    #[test]
    fn an_action_started_through_an_intermediate_event_still_traces_to_its_injection() {
        let mut tracker = tracker();
        let t0 = Instant::now();
        tracker.injected(0, 1, 0, t0);
        let chat = twitch_event("!hype ~q0~");
        tracker.on_event(&chat, t0);
        let matched = core("command.matched", json!({}), chat.id);
        tracker.on_event(&matched, t0);
        tracker.on_event(
            &start("Hype Command", matched.id),
            t0 + Duration::from_millis(3),
        );

        assert_eq!(
            tracker
                .latencies(0, Some(COMMAND_ACTION), Leg::Start)
                .map(<[u32]>::to_vec),
            Some(vec![3])
        );
    }

    #[test]
    fn unfinished_work_is_split_into_queued_and_in_flight() {
        let mut tracker = tracker();
        let t0 = Instant::now();
        for seq in 0..3 {
            tracker.injected(seq, 0, 0, t0);
        }
        tracker.injected(3, 1, 0, t0);
        let chat = twitch_event("~q0~");
        tracker.on_event(&chat, t0);
        let started = start("Chat Message", chat.id);
        tracker.on_event(&started, t0);
        let second = twitch_event("~q1~");
        tracker.on_event(&second, t0);
        let started_second = start("Chat Message", second.id);
        tracker.on_event(&started_second, t0);
        tracker.on_event(&done(started_second.id, "failed"), t0);

        assert_eq!(tracker.expected(CHAT_ACTION), 4);
        assert_eq!(tracker.expected(COMMAND_ACTION), 1);
        assert_eq!(tracker.unstarted(CHAT_ACTION), 2);
        assert_eq!(tracker.in_flight(CHAT_ACTION), 1);
        assert_eq!(tracker.counts(CHAT_ACTION).failed, 1);
    }

    #[test]
    fn a_skip_counts_against_the_action_whose_id_was_learned_before_any_start() {
        let mut tracker = tracker();
        tracker.injected(0, 0, 0, Instant::now());
        tracker.learn_action_id("01CHAT".to_owned(), "Chat Message");
        let chat = twitch_event("~q0~");
        tracker.on_event(
            &core(
                ACTION_SKIPPED,
                json!({ "action_id": "01CHAT", "reason": "queue_pending_overflow" }),
                chat.id,
            ),
            Instant::now(),
        );

        assert_eq!(
            tracker
                .counts(CHAT_ACTION)
                .skipped
                .get("queue_pending_overflow"),
            Some(&1)
        );
        assert_eq!(tracker.unstarted(CHAT_ACTION), 0);
    }

    #[test]
    fn effects_that_cannot_be_traced_are_counted_rather_than_misattributed() {
        let mut tracker = tracker();
        let t0 = Instant::now();
        tracker.injected(0, 0, 0, t0);
        let foreign = Event::new(EventSource::Core, "timer.tick", json!({}));
        tracker.on_event(&start("Chat Message", foreign.id), t0);
        tracker.on_event(&done(EventId::new(), "success"), t0);
        tracker.on_event(
            &core(
                ACTION_SKIPPED,
                json!({ "action_id": "never-seen" }),
                foreign.id,
            ),
            t0,
        );
        tracker.on_reply(Some(&json!({ "message": "thanks ~q99~" })), t0);

        assert_eq!(tracker.uncorrelated, 4);
        assert!(
            tracker
                .latencies(0, Some(CHAT_ACTION), Leg::Start)
                .is_none()
        );
    }

    #[test]
    fn a_dropped_notice_on_a_page_is_counted_against_that_page() {
        let mut tracker = Tracker::new(Vec::new(), Vec::new(), 2);
        tracker.on_frame(1, &json!({ "dropped": 40 }), Instant::now());
        tracker.on_frame(1, &json!({ "dropped": 2 }), Instant::now());

        assert_eq!(tracker.page_dropped, vec![0, 42]);
        assert_eq!(tracker.frames, vec![0, 0]);
    }

    #[test]
    fn a_frame_without_a_marker_is_ignored() {
        let mut tracker = tracker();
        tracker.injected(0, 0, 0, Instant::now());
        tracker.on_frame(
            0,
            &json!({ "content": { "message": "retained" } }),
            Instant::now(),
        );

        assert_eq!(tracker.frames, vec![0]);
        assert_eq!(tracker.uncorrelated, 0);
    }

    #[test]
    fn nearest_rank_percentiles_over_an_unsorted_sample() {
        let samples = [50, 10, 40, 20, 30];
        for (pct, expected) in [(0.0, 10), (20.0, 10), (50.0, 30), (95.0, 50), (100.0, 50)] {
            assert_eq!(percentile(&samples, pct), Some(expected), "p{pct}");
        }
        assert_eq!(percentile(&[], 50.0), None);
    }
}
