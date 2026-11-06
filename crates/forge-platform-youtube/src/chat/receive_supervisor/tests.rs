use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use forge_events::Event;
use forge_net::DedupSet;
use forge_types::ChatPayload;
use serde_json::{Value, json};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedReceiver;

use super::*;
use crate::api::poll_health::{PollHealthTracker, PollLinkHealth};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use crate::chat::ban_ledger_test_support::MemoryBanLedger;
use crate::chat::chat_poller::ChatPollTimings;
use crate::endpoints_test_support::unreachable_data_api;
use crate::vocabulary::event_kinds;

mod live;

const CHAT: &str = "lc-1";
const OTHER_CHAT: &str = "lc-2";
const BROADCAST: &str = "bc-1";
const PAGE_INTERVAL_MS: u64 = 5_000;
const LONG_CADENCE: ChatPollTimings = ChatPollTimings {
    broadcast_cadence: Duration::from_secs(4 * 3600),
    ..ChatPollTimings::DEFAULT
};
const RECONNECT: Duration = Duration::from_secs(5);
const REPROBE: Duration = Duration::from_secs(600);
const POLL_FLOOR: Duration = Duration::from_secs(3);
const LONG_INTERVAL: Duration = Duration::from_secs(60);
const UNTIL_RESET: Duration = Duration::from_secs(100);
const RESET_WAKE: Duration = Duration::from_secs(105);
const BACKOFF_CEILINGS: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Open(Option<String>),
    Poll(Option<String>),
    Refresh,
}

fn open(token: Option<&str>) -> Call {
    Call::Open(token.map(ToOwned::to_owned))
}

fn poll(token: Option<&str>) -> Call {
    Call::Poll(token.map(ToOwned::to_owned))
}

enum Item {
    Page(ChatMessagesResponse),
    End(StreamOutcome),
}

#[derive(Default)]
struct Script {
    opens: VecDeque<Result<Vec<Item>, StreamOutcome>>,
    polls: VecDeque<Result<ChatMessagesResponse, StreamOutcome>>,
    refreshes: VecDeque<Result<(), PlatformError>>,
    calls: Vec<(Call, Instant)>,
}

#[derive(Clone)]
struct Scripted {
    script: Arc<Mutex<Script>>,
    long_interval: bool,
    until_reset: Duration,
    opened: Arc<Notify>,
    start: Instant,
}

impl Scripted {
    fn new() -> Self {
        Self {
            script: Arc::default(),
            long_interval: false,
            until_reset: UNTIL_RESET,
            opened: Arc::default(),
            start: Instant::now(),
        }
    }

    fn in_long_interval_mode(mut self) -> Self {
        self.long_interval = true;
        self
    }

    fn resetting_in(mut self, until_reset: Duration) -> Self {
        self.until_reset = until_reset;
        self
    }

    fn streams(self, items: Vec<Item>) -> Self {
        self.script.lock().unwrap().opens.push_back(Ok(items));
        self
    }

    fn refuses(self, outcome: StreamOutcome) -> Self {
        self.script.lock().unwrap().opens.push_back(Err(outcome));
        self
    }

    fn refuses_times(self, times: usize, outcome: impl Fn() -> StreamOutcome) -> Self {
        (0..times).fold(self, |script, _| script.refuses(outcome()))
    }

    fn polls(self, page: ChatMessagesResponse) -> Self {
        self.script.lock().unwrap().polls.push_back(Ok(page));
        self
    }

    fn poll_fails(self, outcome: StreamOutcome) -> Self {
        self.script.lock().unwrap().polls.push_back(Err(outcome));
        self
    }

    fn refresh_fails(self) -> Self {
        self.script
            .lock()
            .unwrap()
            .refreshes
            .push_back(Err(PlatformError::ReauthRequired {
                platform: "youtube".to_owned(),
            }));
        self
    }

    fn record(&self, call: Call) {
        self.script
            .lock()
            .unwrap()
            .calls
            .push((call, Instant::now()));
    }

    fn calls(&self) -> Vec<Call> {
        self.timeline().into_iter().map(|(call, _)| call).collect()
    }

    fn timeline(&self) -> Vec<(Call, Duration)> {
        self.script
            .lock()
            .unwrap()
            .calls
            .iter()
            .map(|(call, at)| (call.clone(), *at - self.start))
            .collect()
    }

    fn gaps(&self) -> Vec<Duration> {
        self.timeline()
            .windows(2)
            .map(|pair| pair[1].1 - pair[0].1)
            .collect()
    }
}

struct ScriptedStream {
    items: VecDeque<Item>,
    resume: Option<String>,
}

impl PageStream for ScriptedStream {
    async fn next(&mut self) -> Option<StreamEvent> {
        match self.items.pop_front() {
            Some(Item::Page(page)) => {
                if let Some(token) = page.next_page_token.clone().filter(|t| !t.is_empty()) {
                    self.resume = Some(token);
                }
                Some(StreamEvent::Page(page))
            }
            Some(Item::End(outcome)) => Some(StreamEvent::Ended(outcome)),
            None => std::future::pending().await,
        }
    }

    fn resume_token(&self) -> Option<String> {
        self.resume.clone()
    }
}

impl ChatTransports for Scripted {
    type Stream = ScriptedStream;

    async fn open_stream(
        &self,
        _live_chat_id: &str,
        page_token: Option<&str>,
        _cancel: &CancellationToken,
    ) -> Result<ScriptedStream, StreamOutcome> {
        self.record(open(page_token));
        self.opened.notify_one();
        let next = self.script.lock().unwrap().opens.pop_front();
        next.unwrap_or_else(|| Ok(Vec::new()))
            .map(|items| ScriptedStream {
                items: items.into(),
                resume: page_token.map(ToOwned::to_owned),
            })
    }

    async fn poll_page(
        &self,
        _live_chat_id: &str,
        page_token: Option<&str>,
        _cancel: &CancellationToken,
    ) -> Result<ChatMessagesResponse, StreamOutcome> {
        self.record(poll(page_token));
        let next = self.script.lock().unwrap().polls.pop_front();
        next.unwrap_or(Err(StreamOutcome::Cancelled))
    }

    async fn refresh_token(&self) -> Result<(), PlatformError> {
        self.record(Call::Refresh);
        let next = self.script.lock().unwrap().refreshes.pop_front();
        next.unwrap_or(Ok(()))
    }

    async fn long_interval_mode(&self) -> bool {
        self.long_interval
    }

    fn until_quota_reset(&self) -> Duration {
        self.until_reset
    }
}

fn text_item(id: &str) -> Value {
    json!({
        "id": id,
        "snippet": { "type": "textMessageEvent", "displayMessage": "hi" },
        "authorDetails": {
            "displayName": "Viewer",
            "channelId": "UCviewer",
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": false
        }
    })
}

fn ended_item(id: &str) -> Value {
    json!({ "id": id, "snippet": { "type": "chatEndedEvent" } })
}

fn page_of(token: &str, items: Vec<Value>, interval_ms: u64) -> ChatMessagesResponse {
    ChatMessagesResponse {
        items,
        next_page_token: Some(token.to_owned()),
        polling_interval_millis: interval_ms,
    }
}

fn page(token: &str, ids: &[&str]) -> ChatMessagesResponse {
    page_of(
        token,
        ids.iter().map(|id| text_item(id)).collect(),
        PAGE_INTERVAL_MS,
    )
}

fn page_every(interval_ms: u64, token: &str) -> ChatMessagesResponse {
    page_of(token, Vec::new(), interval_ms)
}

fn broken() -> StreamOutcome {
    StreamOutcome::Transport(PlatformError::Network {
        reason: "connection reset".to_owned(),
    })
}

fn throttled(retry_after: Duration) -> StreamOutcome {
    StreamOutcome::RateLimited { retry_after }
}

struct Rig {
    poller: YouTubeChatPoller,
    bus: UnboundedReceiver<Event>,
    dedup: DedupSet,
    health: PollHealthTracker,
    reports: Arc<Mutex<Vec<PollLinkHealth>>>,
    is_live: bool,
    cancel: CancellationToken,
}

fn rig_with(timings: ChatPollTimings) -> Rig {
    let (tx, bus) = tokio::sync::mpsc::unbounded_channel();
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let poller = YouTubeChatPoller::new(
        unreachable_data_api(),
        Arc::new(|| Box::pin(async { Ok("unused".to_owned()) })),
        tx,
        "UCbroadcaster".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        MemoryBanLedger::shared(),
    )
    .with_timings(timings);
    Rig {
        poller,
        bus,
        dedup: DedupSet::bounded(500),
        health: PollHealthTracker::new(Some(Arc::new(move |health| {
            sink.lock().unwrap().push(health);
        }))),
        reports,
        is_live: true,
        cancel: CancellationToken::new(),
    }
}

fn rig() -> Rig {
    rig_with(LONG_CADENCE)
}

fn supervisor(transports: &Scripted) -> ReceiveSupervisor<Scripted> {
    ReceiveSupervisor::new(transports.clone(), ReceiveTimings::DEFAULT)
}

impl Rig {
    async fn receive<T: ChatTransports>(
        &mut self,
        supervisor: &mut ReceiveSupervisor<T>,
    ) -> ReceiveExit {
        self.receive_on(supervisor, CHAT).await
    }

    async fn receive_on<T: ChatTransports>(
        &mut self,
        supervisor: &mut ReceiveSupervisor<T>,
        live_chat_id: &str,
    ) -> ReceiveExit {
        let session = ChatSession {
            live_chat_id,
            broadcast_id: BROADCAST,
            resolved_at: Instant::now(),
            dedup: &mut self.dedup,
            health: &mut self.health,
            is_live: &mut self.is_live,
            cancel: &self.cancel,
        };
        supervisor.receive(&self.poller, session).await
    }

    fn published(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.bus.try_recv() {
            events.push(event);
        }
        events
    }

    fn chat_ids(&mut self) -> Vec<String> {
        self.published()
            .into_iter()
            .filter(|event| event.kind == event_kinds::CHAT_MESSAGE)
            .map(|event| {
                serde_json::from_value::<ChatPayload>(event.payload[ChatPayload::KEY].clone())
                    .unwrap()
                    .platform_msg_id
            })
            .collect()
    }

    fn reports(&self) -> Vec<PollLinkHealth> {
        self.reports.lock().unwrap().clone()
    }
}

fn ids(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|id| (*id).to_owned()).collect()
}

#[tokio::test(start_paused = true)]
async fn a_live_stream_is_received_without_polling() {
    let transports = Scripted::new().streams(vec![Item::Page(page("s-1", &["m-1", "m-2"]))]);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        (transports.calls(), rig.chat_ids()),
        (vec![open(None)], ids(&["m-1", "m-2"]))
    );
}

#[tokio::test(start_paused = true)]
async fn a_closed_stream_reopens_after_exactly_the_reconnect_pause_from_the_resume_token() {
    let transports = Scripted::new().streams(vec![
        Item::Page(page("s-1", &["m-1"])),
        Item::End(StreamOutcome::Closed),
    ]);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        transports.timeline(),
        [(open(None), Duration::ZERO), (open(Some("s-1")), RECONNECT)]
    );
}

#[tokio::test(start_paused = true)]
async fn long_interval_mode_stretches_the_reconnect_pause() {
    let transports = Scripted::new()
        .in_long_interval_mode()
        .streams(vec![Item::End(StreamOutcome::Closed)]);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(transports.gaps(), [LONG_INTERVAL]);
}

#[tokio::test(start_paused = true)]
async fn a_stream_the_upstream_does_not_offer_falls_back_to_polling_at_the_page_interval_or_floor()
{
    for (label, interval_ms, long_interval, wait) in [
        ("page interval", 10_000, false, Duration::from_secs(10)),
        ("floor", 1_000, false, POLL_FLOOR),
        ("long interval", 10_000, true, LONG_INTERVAL),
    ] {
        let mut transports = Scripted::new()
            .refuses(StreamOutcome::ProtocolNotOffered)
            .polls(page_every(interval_ms, "p-1"))
            .polls(page_every(interval_ms, "p-2"));
        if long_interval {
            transports = transports.in_long_interval_mode();
        }
        let mut rig = rig();

        let exit = rig.receive(&mut supervisor(&transports)).await;

        assert_eq!(
            (exit, transports.timeline()),
            (
                ReceiveExit::Stop,
                vec![
                    (open(None), Duration::ZERO),
                    (poll(None), Duration::ZERO),
                    (poll(Some("p-1")), wait),
                    (poll(Some("p-2")), wait * 2),
                ]
            ),
            "{label}"
        );
    }
}

fn polling_until_the_reprobe() -> Scripted {
    (1..=6).fold(
        Scripted::new().refuses(StreamOutcome::ProtocolNotOffered),
        |script, page| script.polls(page_every(100_000, &format!("p-{page}"))),
    )
}

#[tokio::test(start_paused = true)]
async fn polling_reprobes_the_stream_after_the_reprobe_interval_without_the_poll_token() {
    let transports = polling_until_the_reprobe();
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    let timeline = transports.timeline();
    assert_eq!(
        (timeline.len(), timeline.last().cloned()),
        (8, Some((open(None), REPROBE)))
    );
}

#[tokio::test(start_paused = true)]
async fn a_successful_probe_switches_back_to_streaming() {
    let transports = polling_until_the_reprobe().streams(vec![
        Item::Page(page("s-1", &["m-1"])),
        Item::End(StreamOutcome::Closed),
    ]);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    let timeline = transports.timeline();
    assert_eq!(
        timeline[7..],
        [
            (open(None), REPROBE),
            (open(Some("s-1")), REPROBE + RECONNECT)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_probe_returns_to_polling_after_one_failure() {
    let transports = polling_until_the_reprobe()
        .refuses(broken())
        .polls(page_every(100_000, "p-7"));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    let calls = transports.calls();
    assert!(
        matches!(calls[7..9], [Call::Open(None), Call::Poll(_)]),
        "{calls:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn three_consecutive_stream_failures_fall_back_to_polling_after_backoff_pauses() {
    let transports = Scripted::new()
        .refuses_times(3, broken)
        .polls(page("p-1", &[]));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    let gaps = transports.gaps();
    assert_eq!(
        transports.calls()[..4],
        [open(None), open(None), open(None), poll(None)]
    );
    for (gap, ceiling) in gaps.iter().zip(BACKOFF_CEILINGS) {
        assert!(*gap <= ceiling, "{gaps:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_page_resets_the_stream_failure_count() {
    let transports = Scripted::new()
        .refuses_times(2, broken)
        .streams(vec![Item::Page(page("s-1", &["m-1"])), Item::End(broken())])
        .refuses(broken());
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert!(
        transports
            .calls()
            .iter()
            .all(|call| matches!(call, Call::Open(_))),
        "{:?}",
        transports.calls()
    );
}

#[tokio::test(start_paused = true)]
async fn quota_exhaustion_waits_until_the_reset_margin_then_resolves() {
    let transports = Scripted::new().refuses(StreamOutcome::QuotaExhausted);
    let mut rig = rig();
    let started = Instant::now();

    let exit = rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        (exit, started.elapsed()),
        (ReceiveExit::Resolve, RESET_WAKE)
    );
}

#[tokio::test(start_paused = true)]
async fn quota_exhaustion_while_polling_comes_back_on_the_stream() {
    let transports = Scripted::new()
        .refuses(StreamOutcome::ProtocolNotOffered)
        .poll_fails(StreamOutcome::QuotaExhausted);
    let mut rig = rig();
    let mut supervisor = supervisor(&transports);

    rig.receive(&mut supervisor).await;
    rig.receive(&mut supervisor).await;

    assert_eq!(transports.calls(), [open(None), poll(None), open(None)]);
}

#[tokio::test(start_paused = true)]
async fn a_throttle_waits_its_retry_after_before_reopening() {
    let transports = Scripted::new().refuses(throttled(Duration::from_secs(30)));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(transports.gaps(), [Duration::from_secs(30)]);
}

#[tokio::test(start_paused = true)]
async fn five_consecutive_throttles_keep_retrying_the_stream() {
    let transports = Scripted::new().refuses_times(5, || throttled(Duration::from_secs(1)));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(transports.gaps(), [Duration::from_secs(1); 5]);
}

#[tokio::test(start_paused = true)]
async fn the_sixth_consecutive_throttle_waits_for_the_quota_reset() {
    let transports = Scripted::new().refuses_times(6, || throttled(Duration::from_secs(1)));
    let mut rig = rig();
    let started = Instant::now();

    let exit = rig.receive(&mut supervisor(&transports)).await;

    let last_open = transports.timeline().last().unwrap().1;
    assert_eq!(
        (
            exit,
            transports.calls().len(),
            started.elapsed() - last_open
        ),
        (ReceiveExit::Resolve, 6, RESET_WAKE)
    );
}

#[tokio::test(start_paused = true)]
async fn an_unauthorized_stream_refreshes_the_token_and_reopens_at_once() {
    let transports = Scripted::new().refuses(StreamOutcome::Unauthorized);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        transports.timeline(),
        [
            (open(None), Duration::ZERO),
            (Call::Refresh, Duration::ZERO),
            (open(None), Duration::ZERO),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_second_consecutive_unauthorized_backs_off_without_another_refresh() {
    let transports = Scripted::new().refuses_times(2, || StreamOutcome::Unauthorized);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    let gaps = transports.gaps();
    assert_eq!(
        transports.calls(),
        [open(None), Call::Refresh, open(None), open(None)]
    );
    assert!(gaps[2] <= BACKOFF_CEILINGS[0], "{gaps:?}");
}

#[tokio::test(start_paused = true)]
async fn a_page_between_unauthorized_answers_rearms_the_refresh() {
    let transports = Scripted::new()
        .refuses(StreamOutcome::Unauthorized)
        .streams(vec![
            Item::Page(page("s-1", &["m-1"])),
            Item::End(StreamOutcome::Unauthorized),
        ]);
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        transports.calls(),
        [
            open(None),
            Call::Refresh,
            open(None),
            Call::Refresh,
            open(Some("s-1"))
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_failed_refresh_counts_as_a_stream_failure() {
    let transports = Scripted::new()
        .refuses(StreamOutcome::Unauthorized)
        .refresh_fails()
        .refuses_times(2, broken)
        .polls(page("p-1", &[]));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        transports.calls()[..5],
        [
            open(None),
            Call::Refresh,
            open(None),
            open(None),
            poll(None)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_chat_ended_event_on_the_stream_publishes_one_offline_and_ends_the_broadcast() {
    let transports = Scripted::new().streams(vec![
        Item::Page(page_of(
            "s-1",
            vec![text_item("m-1"), ended_item("end-1")],
            PAGE_INTERVAL_MS,
        )),
        Item::End(StreamOutcome::ChatEnded),
    ]);
    let mut rig = rig();

    let exit = rig.receive(&mut supervisor(&transports)).await;

    let offline = rig
        .published()
        .into_iter()
        .filter(|event| event.kind == event_kinds::STREAM_OFFLINE)
        .count();
    assert_eq!(
        (exit, offline, rig.is_live),
        (ReceiveExit::BroadcastEnded, 1, false)
    );
}

#[tokio::test(start_paused = true)]
async fn a_chat_end_reported_by_the_stream_ends_the_broadcast_and_forgets_the_resume_token() {
    let transports = Scripted::new().streams(vec![
        Item::Page(page("s-1", &["m-1"])),
        Item::End(StreamOutcome::ChatEnded),
    ]);
    let mut rig = rig();
    let mut supervisor = supervisor(&transports);

    let exit = rig.receive(&mut supervisor).await;
    rig.receive(&mut supervisor).await;

    assert_eq!(
        (exit, transports.calls()),
        (ReceiveExit::BroadcastEnded, vec![open(None), open(None)])
    );
}

#[tokio::test(start_paused = true)]
async fn a_chat_end_seen_while_polling_ends_the_broadcast() {
    for (label, transports) in [
        (
            "refusal",
            Scripted::new()
                .refuses(StreamOutcome::ProtocolNotOffered)
                .poll_fails(StreamOutcome::ChatEnded),
        ),
        (
            "event",
            Scripted::new()
                .refuses(StreamOutcome::ProtocolNotOffered)
                .polls(page_of("p-1", vec![ended_item("end-1")], PAGE_INTERVAL_MS)),
        ),
    ] {
        let mut rig = rig();

        let exit = rig.receive(&mut supervisor(&transports)).await;

        assert_eq!(exit, ReceiveExit::BroadcastEnded, "{label}");
    }
}

#[tokio::test(start_paused = true)]
async fn messages_seen_on_the_stream_are_not_republished_after_the_hand_over_to_polling() {
    let transports = Scripted::new()
        .streams(vec![
            Item::Page(page("s-1", &["m-1", "m-2"])),
            Item::End(broken()),
        ])
        .refuses_times(2, broken)
        .polls(page("p-1", &["m-1", "m-2", "m-3"]));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        (rig.chat_ids(), transports.calls().get(3).cloned()),
        (ids(&["m-1", "m-2", "m-3"]), Some(poll(None)))
    );
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_held_stream_stops_at_once() {
    let transports = Scripted::new();
    let mut rig = rig();
    let cancel = rig.cancel.clone();
    let opened = Arc::clone(&transports.opened);
    let mut supervisor = supervisor(&transports);
    let started = Instant::now();

    let (exit, ()) = tokio::join!(rig.receive(&mut supervisor), async {
        opened.notified().await;
        cancel.cancel();
    });

    assert_eq!(
        (exit, started.elapsed()),
        (ReceiveExit::Stop, Duration::ZERO)
    );
}

#[tokio::test(start_paused = true)]
async fn cancelling_during_the_quota_wait_stops_at_once() {
    let transports = Scripted::new()
        .resetting_in(Duration::from_secs(3600))
        .refuses(StreamOutcome::QuotaExhausted);
    let mut rig = rig();
    let cancel = rig.cancel.clone();
    let opened = Arc::clone(&transports.opened);
    let mut supervisor = supervisor(&transports);
    let started = Instant::now();

    let (exit, ()) = tokio::join!(rig.receive(&mut supervisor), async {
        opened.notified().await;
        cancel.cancel();
    });

    assert_eq!(
        (exit, started.elapsed()),
        (ReceiveExit::Stop, Duration::ZERO)
    );
}

#[tokio::test(start_paused = true)]
async fn the_cadence_deadline_cuts_a_held_stream_and_the_next_receive_resumes_from_its_token() {
    let transports = Scripted::new().streams(vec![Item::Page(page("s-1", &["m-1"]))]);
    let mut rig = rig_with(ChatPollTimings::DEFAULT);
    let mut supervisor = supervisor(&transports);
    let started = Instant::now();

    let exit = rig.receive(&mut supervisor).await;
    let cut_at = started.elapsed();
    rig.receive(&mut supervisor).await;

    assert_eq!(
        (exit, cut_at, transports.calls()),
        (
            ReceiveExit::Resolve,
            ChatPollTimings::DEFAULT.broadcast_cadence,
            vec![open(None), open(Some("s-1"))]
        )
    );
}

#[tokio::test(start_paused = true)]
async fn a_new_live_chat_opens_without_the_previous_resume_token() {
    let transports = Scripted::new().streams(vec![Item::Page(page("s-1", &["m-1"]))]);
    let mut rig = rig_with(ChatPollTimings::DEFAULT);
    let mut supervisor = supervisor(&transports);

    rig.receive_on(&mut supervisor, CHAT).await;
    rig.receive_on(&mut supervisor, OTHER_CHAT).await;

    assert_eq!(transports.calls(), [open(None), open(None)]);
}

#[tokio::test(start_paused = true)]
async fn link_health_follows_success_and_loss_across_stream_and_polling() {
    let transports = Scripted::new()
        .streams(vec![Item::Page(page("s-1", &["m-1"])), Item::End(broken())])
        .refuses_times(2, broken)
        .polls(page("p-1", &[]));
    let mut rig = rig();

    rig.receive(&mut supervisor(&transports)).await;

    assert_eq!(
        rig.reports(),
        [
            PollLinkHealth::Healthy,
            PollLinkHealth::Lost,
            PollLinkHealth::Healthy
        ]
    );
}
