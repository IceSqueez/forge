#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::OverlaySendRunner;
use forge_runtime::{
    EventBus, NullEventLogRepo, OVERLAY_TARGET_KEY, OverlayDelivery, OverlayDispatch,
    OverlayFrameSink, OverlayReceivers, OverlayServiceCell, OverlayServiceError,
    OverlayServiceHandle, SHOW_CEILING, SHOW_QUEUE_CAPACITY, ShowEnd, ShowTicket,
};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo,
};
use forge_types::{ArgStack, EventId, SubActionConfig, SubActionOutcome, Variant};
use time::OffsetDateTime;
use tokio::time::Instant;

const ALERT_KIND: &str = "overlay.alert";
const CHAT_KIND: &str = "overlay.chat";
const GOAL_KIND: &str = "overlay.goal";

const STAGE: &str = "stage-alert";
const SIDE: &str = "side-alert";

const HEADLINE_KEY: &str = "headline";
const WAIT_KEY: &str = "wait_for_show";
const DURATION_KEY: &str = "duration_secs";

const ALERT_WINDOW: Duration = Duration::from_secs(5);
const NEVER: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
struct Frame {
    identity: OverlayId,
    headline: Option<String>,
    duration_ms: Option<u64>,
    at: Duration,
}

struct TimedSink {
    started: Instant,
    frames: Mutex<Vec<Frame>>,
    absent_for: AtomicUsize,
}

impl TimedSink {
    fn new(absent_for: usize) -> Self {
        Self {
            started: Instant::now(),
            frames: Mutex::new(Vec::new()),
            absent_for: AtomicUsize::new(absent_for),
        }
    }

    fn frames(&self) -> Vec<Frame> {
        self.frames.lock().unwrap().clone()
    }

    fn arrivals(&self, identity: &str) -> Vec<(String, Duration)> {
        self.frames()
            .into_iter()
            .filter(|frame| frame.identity.as_str() == identity)
            .map(|frame| (frame.headline.unwrap_or_default(), frame.at))
            .collect()
    }
}

#[async_trait]
impl OverlayFrameSink for TimedSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers {
        self.frames.lock().unwrap().push(Frame {
            identity: identity.clone(),
            headline: content[HEADLINE_KEY].as_str().map(str::to_owned),
            duration_ms,
            at: self.started.elapsed(),
        });
        let absent = self
            .absent_for
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        OverlayReceivers {
            sources: usize::from(!absent),
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _: Event) {}
}

fn definition(id: &str, kind_id: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: id.to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

struct Harness {
    sink: Arc<TimedSink>,
    service: OverlayServiceHandle,
}

fn harness_with(definitions: Vec<OverlayDefinition>, absent_for: usize) -> Harness {
    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(move |id| Ok(definitions.iter().find(|d| &d.id == id).cloned()));
    repo.expect_delete().returning(|_| Ok(true));
    repo.expect_set_enabled().returning(|_, _| Ok(true));
    repo.expect_set_retained_content().returning(|_, _| Ok(()));
    repo.expect_get_retained_content().returning(|_| Ok(None));
    let mut settings = MockSettingsRepo::new();
    settings.expect_get_string().returning(|_| Ok(None));

    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
    let sink = Arc::new(TimedSink::new(absent_for));

    let service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(settings) as Arc<dyn SettingsRepo>,
        Arc::new(kinds),
        EventBus::new(Arc::new(NullEventLogRepo)),
        Some(Arc::clone(&sink) as Arc<dyn OverlayFrameSink>),
    );
    Harness { sink, service }
}

fn harness(definitions: Vec<OverlayDefinition>) -> Harness {
    harness_with(definitions, 0)
}

fn headline(text: &str) -> OverlayConfig {
    OverlayConfig::from([(HEADLINE_KEY.to_owned(), Variant::String(text.to_owned()))])
}

impl Harness {
    async fn dispatch(&self, id: &str, text: &str, duration_ms: Option<u64>) -> OverlayDispatch {
        self.service
            .send_to(
                &OverlayId::new(id),
                &headline(text),
                &ArgStack::new(),
                duration_ms,
            )
            .await
            .expect("a stored overlay of a shipped look accepts a send")
    }

    async fn queue(&self, id: &str, text: &str, duration_ms: Option<u64>) -> ShowTicket {
        match self.dispatch(id, text, duration_ms).await {
            OverlayDispatch::Queued(ticket) => ticket,
            OverlayDispatch::Applied(delivery) => {
                panic!("a transient show was applied at once ({delivery:?}) instead of queued")
            }
        }
    }

    /// Lets the presenter take the head show onto the page, so what remains is what waits.
    async fn until_on_screen(&self, id: &str, shown: usize) {
        for _ in 0..1_000 {
            if self.sink.arrivals(id).len() >= shown {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the presenter never put show {shown} of '{id}' on the page");
    }
}

async fn finished(ticket: ShowTicket) -> ShowEnd {
    tokio::time::timeout(NEVER, ticket.finished())
        .await
        .expect("a queued show never ended, so its presenter is stuck or gone")
}

fn texts(arrivals: &[(String, Duration)]) -> Vec<&str> {
    arrivals.iter().map(|(text, _)| text.as_str()).collect()
}

#[tokio::test(start_paused = true)]
async fn shows_on_one_overlay_reach_the_page_in_arrival_order_each_after_the_previous_one_ends() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);

    let mut tickets = Vec::new();
    for text in ["first", "second", "third"] {
        tickets.push(harness.queue(STAGE, text, None).await);
    }
    for ticket in tickets {
        finished(ticket).await;
    }

    assert_eq!(
        harness.sink.arrivals(STAGE),
        vec![
            ("first".to_owned(), Duration::ZERO),
            ("second".to_owned(), ALERT_WINDOW),
            ("third".to_owned(), ALERT_WINDOW * 2),
        ],
        "a burst on one overlay was not shown one at a time, oldest first, each for its window"
    );
}

#[tokio::test(start_paused = true)]
async fn one_overlays_backlog_never_delays_a_show_on_another() {
    let harness = harness(vec![
        definition(STAGE, ALERT_KIND),
        definition(SIDE, ALERT_KIND),
    ]);

    let busy = [
        harness.queue(STAGE, "stage-1", None).await,
        harness.queue(STAGE, "stage-2", None).await,
    ];
    let side = harness.queue(SIDE, "side-1", None).await;
    finished(side).await;
    for ticket in busy {
        finished(ticket).await;
    }

    assert_eq!(
        harness.sink.arrivals(SIDE),
        vec![("side-1".to_owned(), Duration::ZERO)],
        "a show on an idle overlay waited behind another overlay's queue"
    );
}

#[tokio::test(start_paused = true)]
async fn a_show_holds_its_overlay_for_the_step_duration_but_never_past_the_ceiling() {
    let ceiling_ms = u64::try_from(SHOW_CEILING.as_millis()).unwrap();
    for (override_ms, held, timed_ms, label) in [
        (
            2_000,
            Duration::from_secs(2),
            2_000,
            "a step duration below the ceiling",
        ),
        (
            ceiling_ms * 5,
            SHOW_CEILING,
            ceiling_ms,
            "a step duration far past the ceiling",
        ),
    ] {
        let harness = harness(vec![definition(STAGE, ALERT_KIND)]);

        let head = harness.queue(STAGE, "head", Some(override_ms)).await;
        let next = harness.queue(STAGE, "next", None).await;
        finished(head).await;
        finished(next).await;

        let frames = harness.sink.frames();
        assert_eq!(
            (frames[0].duration_ms, frames[1].at),
            (Some(timed_ms), held),
            "{label}: the page was told the wrong hide time, or the next show came at the wrong moment"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_show_that_waited_past_any_staleness_bound_is_still_shown() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    let ceiling_ms = u64::try_from(SHOW_CEILING.as_millis()).unwrap();

    let mut tickets = Vec::new();
    for text in ["one", "two", "three", "late"] {
        tickets.push(harness.queue(STAGE, text, Some(ceiling_ms)).await);
    }
    let mut ends = Vec::new();
    for ticket in tickets {
        ends.push(finished(ticket).await);
    }

    assert_eq!(
        (ends[3], harness.sink.arrivals(STAGE).pop()),
        (
            ShowEnd::Shown(OverlayDelivery::Delivered { sources: 1 }),
            Some(("late".to_owned(), SHOW_CEILING * 3)),
        ),
        "a show that waited six minutes in the queue was dropped instead of shown"
    );
}

#[tokio::test(start_paused = true)]
async fn a_burst_past_the_capacity_refuses_only_the_excess_and_shows_every_accepted_show() {
    const EXCESS: usize = 3;
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    let mut accepted = vec![harness.queue(STAGE, "0", None).await];
    harness.until_on_screen(STAGE, 1).await;

    let mut refused = Vec::new();
    for index in 1..=SHOW_QUEUE_CAPACITY + EXCESS {
        match harness
            .service
            .send_to(
                &OverlayId::new(STAGE),
                &headline(&index.to_string()),
                &ArgStack::new(),
                None,
            )
            .await
        {
            Ok(OverlayDispatch::Queued(ticket)) => accepted.push(ticket),
            Ok(OverlayDispatch::Applied(delivery)) => {
                panic!("show {index} bypassed the queue ({delivery:?})")
            }
            Err(error) => refused.push((index, error)),
        }
    }
    let refused_at: Vec<usize> = refused.iter().map(|(index, _)| *index).collect();
    assert_eq!(
        refused_at,
        (SHOW_QUEUE_CAPACITY + 1..=SHOW_QUEUE_CAPACITY + EXCESS).collect::<Vec<_>>(),
        "the queue refused a show below its capacity or accepted one past it"
    );
    assert!(
        refused.iter().all(|(_, error)| matches!(
            error,
            OverlayServiceError::ShowQueueFull { id, capacity }
                if id.as_str() == STAGE && *capacity == SHOW_QUEUE_CAPACITY
        )),
        "an overflow was reported as something other than a full show queue: {refused:?}"
    );

    for ticket in accepted {
        assert_eq!(
            finished(ticket).await,
            ShowEnd::Shown(OverlayDelivery::Delivered { sources: 1 }),
            "an accepted show ended without being shown"
        );
    }
    let expected: Vec<String> = (0..=SHOW_QUEUE_CAPACITY).map(|i| i.to_string()).collect();
    assert_eq!(
        texts(&harness.sink.arrivals(STAGE)),
        expected.iter().map(String::as_str).collect::<Vec<_>>(),
        "the page did not receive every accepted show exactly once, in order"
    );
}

#[tokio::test(start_paused = true)]
async fn clearing_ends_every_waiting_show_as_cleared_and_lets_the_one_on_screen_finish() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    let on_screen = harness.queue(STAGE, "on-screen", None).await;
    let waiting = [
        harness.queue(STAGE, "waiting-1", None).await,
        harness.queue(STAGE, "waiting-2", None).await,
    ];
    harness.until_on_screen(STAGE, 1).await;

    let cleared = harness.service.clear_shows(&OverlayId::new(STAGE));

    assert_eq!(
        (
            cleared,
            harness.service.pending_shows(&OverlayId::new(STAGE))
        ),
        (2, 0),
        "clearing did not drop exactly the shows still waiting"
    );
    for ticket in waiting {
        assert_eq!(finished(ticket).await, ShowEnd::Cleared);
    }
    assert_eq!(
        finished(on_screen).await,
        ShowEnd::Shown(OverlayDelivery::Delivered { sources: 1 }),
        "clearing cut short the show already on the page"
    );
    assert_eq!(
        texts(&harness.sink.arrivals(STAGE)),
        vec!["on-screen"],
        "a cleared show still reached the page"
    );
}

#[derive(Debug, Clone, Copy)]
enum Lifecycle {
    Delete,
    Disable,
    Enable,
}

#[tokio::test(start_paused = true)]
async fn deleting_or_disabling_an_overlay_withdraws_its_waiting_shows_and_enabling_keeps_them() {
    for (change, expected) in [
        (Lifecycle::Delete, ShowEnd::Withdrawn),
        (Lifecycle::Disable, ShowEnd::Withdrawn),
        (
            Lifecycle::Enable,
            ShowEnd::Shown(OverlayDelivery::Delivered { sources: 1 }),
        ),
    ] {
        let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
        let on_screen = harness.queue(STAGE, "on-screen", None).await;
        let waiting = harness.queue(STAGE, "waiting", None).await;
        harness.until_on_screen(STAGE, 1).await;
        let id = OverlayId::new(STAGE);

        match change {
            Lifecycle::Delete => assert!(harness.service.delete(&id).await.unwrap()),
            Lifecycle::Disable => assert!(harness.service.set_enabled(&id, false).await.unwrap()),
            Lifecycle::Enable => assert!(harness.service.set_enabled(&id, true).await.unwrap()),
        }

        assert_eq!(
            finished(waiting).await,
            expected,
            "{change:?} left the waiting show with the wrong end"
        );
        finished(on_screen).await;
    }
}

#[tokio::test(start_paused = true)]
async fn an_overlay_whose_queue_ran_dry_presents_the_next_show_at_once() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    finished(harness.queue(STAGE, "before", None).await).await;
    tokio::time::advance(Duration::from_secs(60)).await;

    let after = harness.queue(STAGE, "after", None).await;
    finished(after).await;

    assert_eq!(
        harness.sink.arrivals(STAGE).last(),
        Some(&("after".to_owned(), ALERT_WINDOW + Duration::from_secs(60))),
        "a show sent to an idle overlay was not presented the moment it arrived"
    );
}

#[tokio::test(start_paused = true)]
async fn a_look_that_applies_content_on_arrival_is_never_queued() {
    for kind in [GOAL_KIND, CHAT_KIND] {
        let harness = harness(vec![definition(STAGE, kind)]);

        let mut dispatches = Vec::new();
        for text in ["one", "two", "three"] {
            dispatches.push(harness.dispatch(STAGE, text, Some(5_000)).await);
        }

        assert!(
            dispatches.iter().all(|dispatch| matches!(
                dispatch,
                OverlayDispatch::Applied(OverlayDelivery::Delivered { sources: 1 })
            )) && harness.sink.frames().len() == 3,
            "a {kind} send was held in a show queue instead of applied at once: {dispatches:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn audio_and_a_test_fire_reach_a_page_whose_show_queue_is_busy_at_once() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    let on_screen = harness.queue(STAGE, "on-screen", None).await;
    let waiting = harness.queue(STAGE, "waiting", None).await;
    harness.until_on_screen(STAGE, 1).await;
    let id = OverlayId::new(STAGE);

    let audio = harness
        .service
        .deliver_audio(&id, headline("announcement"))
        .await
        .expect("an alert can carry audio");
    let fired = harness
        .service
        .test_fire(&id)
        .await
        .expect("an alert can be test-fired");

    assert_eq!(
        (
            audio,
            fired.delivery,
            harness.sink.frames().len(),
            harness.service.pending_shows(&id),
        ),
        (
            OverlayDelivery::Delivered { sources: 1 },
            OverlayDelivery::Delivered { sources: 1 },
            3,
            1,
        ),
        "audio or a test fire waited behind the show queue, or displaced a waiting show"
    );
    finished(on_screen).await;
    finished(waiting).await;
}

fn step(wait: bool) -> SubActionConfig {
    SubActionConfig::from([
        (
            OVERLAY_TARGET_KEY.to_owned(),
            Variant::String(STAGE.to_owned()),
        ),
        (WAIT_KEY.to_owned(), Variant::Bool(wait)),
        (DURATION_KEY.to_owned(), Variant::Int(3)),
    ])
}

fn runner_for(harness: &Harness) -> OverlaySendRunner {
    let cell = OverlayServiceCell::new();
    cell.set(harness.service.clone());
    OverlaySendRunner::new(cell)
}

#[tokio::test(start_paused = true)]
async fn a_waiting_step_returns_when_its_show_leaves_the_screen_and_a_plain_one_at_once() {
    for (wait, returns_after) in [(true, Duration::from_secs(3)), (false, Duration::ZERO)] {
        let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
        let runner = runner_for(&harness);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let started = Instant::now();

        let (telemetry, _) = runner.execute(&step(wait), &ctx).await;

        assert_eq!(
            (telemetry.outcome, started.elapsed()),
            (SubActionOutcome::Success, returns_after),
            "a step with wait_for_show={wait} returned at the wrong moment"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_waiting_step_whose_show_is_dropped_before_it_runs_fails_at_once() {
    for change in [Lifecycle::Delete, Lifecycle::Disable] {
        let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
        let on_screen = harness.queue(STAGE, "on-screen", None).await;
        harness.until_on_screen(STAGE, 1).await;
        let runner = runner_for(&harness);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let id = OverlayId::new(STAGE);
        let waiting_step = step(true);
        let started = Instant::now();

        let ((telemetry, _), ()) = tokio::join!(runner.execute(&waiting_step, &ctx), async {
            while harness.service.pending_shows(&id) == 0 {
                tokio::task::yield_now().await;
            }
            match change {
                Lifecycle::Delete => {
                    harness.service.delete(&id).await.unwrap();
                }
                _ => {
                    harness.service.set_enabled(&id, false).await.unwrap();
                }
            }
        });

        assert!(
            matches!(telemetry.outcome, SubActionOutcome::Failed(_))
                && started.elapsed() < ALERT_WINDOW,
            "{change:?} left a waiting step reporting {:?} after {:?}",
            telemetry.outcome,
            started.elapsed()
        );
        finished(on_screen).await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_waiting_step_whose_show_is_cleared_fails_at_once() {
    let harness = harness(vec![definition(STAGE, ALERT_KIND)]);
    let on_screen = harness.queue(STAGE, "on-screen", None).await;
    harness.until_on_screen(STAGE, 1).await;
    let runner = runner_for(&harness);
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let id = OverlayId::new(STAGE);
    let waiting_step = step(true);
    let started = Instant::now();

    let ((telemetry, _), cleared) = tokio::join!(runner.execute(&waiting_step, &ctx), async {
        while harness.service.pending_shows(&id) == 0 {
            tokio::task::yield_now().await;
        }
        harness.service.clear_shows(&id)
    });

    assert!(
        cleared == 1
            && matches!(telemetry.outcome, SubActionOutcome::Failed(_))
            && started.elapsed() < ALERT_WINDOW,
        "a cleared show left its waiting step reporting {:?} after {:?}",
        telemetry.outcome,
        started.elapsed()
    );
    finished(on_screen).await;
}

#[tokio::test(start_paused = true)]
async fn a_page_briefly_absent_mid_burst_does_not_flush_the_rest_of_the_queue() {
    let harness = harness_with(vec![definition(STAGE, ALERT_KIND)], 1);

    let mut tickets = Vec::new();
    for text in ["during-reload", "after-reload", "later"] {
        tickets.push(harness.queue(STAGE, text, None).await);
    }
    for ticket in tickets {
        finished(ticket).await;
    }

    let arrivals = harness.sink.arrivals(STAGE);
    assert!(
        arrivals
            .windows(2)
            .all(|pair| pair[1].1 >= pair[0].1 + ALERT_WINDOW),
        "one delivery that found no page let the rest of the burst through at once, so a browser \
         source reloading mid-burst loses every show behind it: {arrivals:?}"
    );
}
