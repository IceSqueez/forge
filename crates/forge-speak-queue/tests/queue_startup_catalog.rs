#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_speak_queue::{
    QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakQueueHandle,
};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice,
};
use forge_voice::AssignmentStrategy;
use tokio::sync::Semaphore;

use common::{alias, make_deps, recording_sink, request, request_with_overrides, voice, wait_for};

const QUIESCENCE: Duration = Duration::from_secs(10);
const SLOW_CATALOG: Duration = Duration::from_secs(15);
const NEVER_ANSWERING_PATIENCE: Duration = Duration::from_secs(300);
const ENGINE_CATALOG_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
enum Listing {
    Ready,
    GatedOnce(Arc<Semaphore>),
    GatedAfterFirst(Arc<Semaphore>),
    Calls(Arc<[Listing]>),
    Empty,
    Slow(Duration),
    Never,
    Fails,
    Panics,
    CreateFails,
}

struct ScriptedEngine {
    id: EngineId,
    listing: Listing,
    listings: Arc<AtomicUsize>,
}

#[async_trait]
impl TtsEngine for ScriptedEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }
    fn capabilities(&self) -> &EngineCapabilities {
        static CAPS: EngineCapabilities = EngineCapabilities {
            ssml: false,
            neural_voices: false,
            streaming: false,
            custom_lexicons: false,
        };
        &CAPS
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        let call = self.listings.fetch_add(1, Ordering::SeqCst);
        let step = match &self.listing {
            Listing::Calls(steps) => steps[call.min(steps.len() - 1)].clone(),
            other => other.clone(),
        };
        match &step {
            Listing::GatedOnce(gate) if call == 0 => {
                drop(gate.acquire().await.expect("gate open"));
            }
            Listing::GatedAfterFirst(gate) if call > 0 => {
                drop(gate.acquire().await.expect("gate open"));
            }
            Listing::Slow(wait) => tokio::time::sleep(*wait).await,
            Listing::Never => std::future::pending::<()>().await,
            Listing::Fails => return Err(TtsError::NetworkFailed("catalog down".into())),
            Listing::Panics => panic!("catalog listing panicked"),
            Listing::Empty => return Ok(Vec::new()),
            _ => {}
        }
        let edition = match &self.listing {
            Listing::GatedOnce(_) | Listing::GatedAfterFirst(_) | Listing::Calls(_) => call + 1,
            _ => 1,
        };
        Ok(vec![voice(&format!("{}-{edition}", self.id.0), &self.id.0)])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct ScriptedFactory {
    id: EngineId,
    listing: Listing,
    listings: Arc<AtomicUsize>,
}

impl TtsEngineFactory for ScriptedFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        if matches!(self.listing, Listing::CreateFails) {
            return Err(TtsError::EngineUnavailable {
                id: self.id.clone(),
                detail: "not installed".into(),
            });
        }
        Ok(Box::new(ScriptedEngine {
            id: self.id.clone(),
            listing: self.listing.clone(),
            listings: Arc::clone(&self.listings),
        }))
    }
}

struct Started {
    started_at: tokio::time::Instant,
    registry: Arc<std::sync::RwLock<TtsRegistry>>,
    handle: SpeakQueueHandle,
    stream: SpeakEventStream,
    plays: Arc<std::sync::Mutex<usize>>,
    listings: Vec<Arc<AtomicUsize>>,
}

fn start(engines: &[(&str, Listing)]) -> Started {
    start_with(engines, vec![])
}

fn start_with(engines: &[(&str, Listing)], aliases: Vec<forge_voice::VoiceAlias>) -> Started {
    let mut registry = TtsRegistry::new();
    let mut listings = Vec::new();
    for (id, listing) in engines {
        let counter = Arc::new(AtomicUsize::new(0));
        listings.push(Arc::clone(&counter));
        registry.register(
            EngineId((*id).into()),
            Arc::new(ScriptedFactory {
                id: EngineId((*id).into()),
                listing: listing.clone(),
                listings: counter,
            }),
        );
    }
    let (sink, plays) = recording_sink();
    let deps = make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        aliases,
    );
    let registry = Arc::clone(&deps.registry);
    let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    Started {
        started_at: tokio::time::Instant::now(),
        registry,
        handle,
        stream,
        plays,
        listings,
    }
}

fn gate() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(0))
}

fn finished(id: &RequestId) -> impl Fn(&SpeakEvent) -> bool + '_ {
    move |e| matches!(e, SpeakEvent::Finished { request_id } if request_id == id)
}

fn voice_ids(handle: &SpeakQueueHandle) -> Vec<String> {
    handle
        .available_voices()
        .iter()
        .map(|v| v.id.0.clone())
        .collect()
}

async fn round_trip(q: &mut Started) -> Vec<SpeakEvent> {
    q.handle.send(SpeakCommand::Pause).await.unwrap();
    q.handle.send(SpeakCommand::Resume).await.unwrap();
    let mut seen = Vec::new();
    loop {
        let event = q.stream.recv().await.unwrap();
        let resumed = matches!(event, SpeakEvent::Resumed);
        seen.push(event);
        if resumed {
            return seen;
        }
    }
}

fn starts_anything(events: &[SpeakEvent]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            SpeakEvent::Started { .. } | SpeakEvent::Failed { .. } | SpeakEvent::Skipped { .. }
        )
    })
}

async fn first_terminal(
    stream: &mut SpeakEventStream,
    id: &RequestId,
    within: Duration,
) -> Option<SpeakEvent> {
    tokio::time::timeout(within, async {
        loop {
            match stream.recv().await.unwrap() {
                e @ (SpeakEvent::Finished { .. }
                | SpeakEvent::Failed { .. }
                | SpeakEvent::Skipped { .. })
                    if terminal_id(&e) == Some(id) =>
                {
                    return e;
                }
                _ => {}
            }
        }
    })
    .await
    .ok()
}

fn terminal_id(event: &SpeakEvent) -> Option<&RequestId> {
    match event {
        SpeakEvent::Finished { request_id }
        | SpeakEvent::Failed { request_id, .. }
        | SpeakEvent::Skipped { request_id, .. } => Some(request_id),
        _ => None,
    }
}

#[tokio::test]
async fn control_commands_take_effect_while_the_first_catalog_is_still_building() {
    type Expect = fn(&SpeakEvent) -> bool;
    let cases: [(SpeakCommand, Expect); 3] = [
        (SpeakCommand::Pause, |e| {
            matches!(e, SpeakEvent::Paused { .. })
        }),
        (SpeakCommand::Resume, |e| matches!(e, SpeakEvent::Resumed)),
        (SpeakCommand::Clear, |e| matches!(e, SpeakEvent::Cleared)),
    ];
    for (command, expected) in cases {
        let mut q = start(&[("alpha", Listing::GatedOnce(gate()))]);
        q.handle.send(command).await.unwrap();
        wait_for(&mut q.stream, expected, 1_000).await;
    }
}

#[tokio::test]
async fn volume_changes_apply_while_the_first_catalog_is_still_building() {
    let mut q = start(&[("alpha", Listing::GatedOnce(gate()))]);
    q.handle.send(SpeakCommand::SetVolume(0.25)).await.unwrap();
    round_trip(&mut q).await;
    assert!((q.handle.master_volume() - 0.25).abs() < f32::EPSILON);
}

#[tokio::test]
async fn an_early_enqueue_waits_for_the_first_catalog_then_plays() {
    let gate = gate();
    let mut q = start(&[("alpha", Listing::GatedOnce(Arc::clone(&gate)))]);
    let req = request("viewer", "hello");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    let before = round_trip(&mut q).await;
    assert!(
        !starts_anything(&before),
        "dequeued before the catalog: {before:?}"
    );
    assert_eq!(*q.plays.lock().unwrap(), 0);

    gate.add_permits(1);
    wait_for(&mut q.stream, finished(&id), 2_000).await;
    assert_eq!(*q.plays.lock().unwrap(), 1);
}

#[tokio::test]
async fn skip_before_the_first_catalog_leaves_waiting_requests_queued() {
    let gate = gate();
    let mut q = start(&[("alpha", Listing::GatedOnce(Arc::clone(&gate)))]);
    let req = request("viewer", "hello");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    q.handle.send(SpeakCommand::Skip).await.unwrap();
    round_trip(&mut q).await;
    assert_eq!(q.handle.queue_depth(), 1);

    gate.add_permits(1);
    wait_for(&mut q.stream, finished(&id), 2_000).await;
}

#[tokio::test]
async fn clear_before_the_first_catalog_drops_waiting_requests() {
    let gate = gate();
    let mut q = start(&[("alpha", Listing::GatedOnce(Arc::clone(&gate)))]);
    for text in ["one", "two"] {
        q.handle
            .send(SpeakCommand::Enqueue(request("viewer", text)))
            .await
            .unwrap();
    }
    q.handle.send(SpeakCommand::Clear).await.unwrap();
    wait_for(&mut q.stream, |e| matches!(e, SpeakEvent::Cleared), 1_000).await;
    gate.add_permits(1);

    let later = request("viewer", "after");
    let later_id = later.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(later)).await.unwrap();
    let mut seen = Vec::new();
    loop {
        let event = q.stream.recv().await.unwrap();
        let done = finished(&later_id)(&event);
        seen.push(event);
        if done {
            break;
        }
    }
    let started: Vec<&RequestId> = seen
        .iter()
        .filter_map(|e| match e {
            SpeakEvent::Started { request_id, .. } => Some(request_id),
            _ => None,
        })
        .collect();
    assert!(started.iter().all(|id| **id == later_id), "{seen:?}");
}

#[tokio::test]
async fn a_failing_engine_still_lets_the_first_catalog_complete() {
    for failing in [Listing::CreateFails, Listing::Fails, Listing::Panics] {
        let mut q = start(&[("alpha", Listing::Ready), ("broken", failing)]);
        let req = request("viewer", "hello");
        let id = req.request_id.clone();
        q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
        wait_for(&mut q.stream, finished(&id), 2_000).await;
        assert_eq!(voice_ids(&q.handle), vec!["alpha-1".to_owned()]);
    }
}

#[tokio::test]
async fn an_empty_first_catalog_settles_an_early_request_instead_of_holding_it() {
    let mut q = start(&[("broken", Listing::Fails)]);
    let req = request("viewer", "hello");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    let outcome = first_terminal(&mut q.stream, &id, Duration::from_secs(30)).await;
    assert!(
        matches!(
            outcome,
            Some(SpeakEvent::Skipped { .. } | SpeakEvent::Failed { .. })
        ),
        "{outcome:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_listing_from_an_older_generation_is_dropped() {
    type Trigger = fn() -> SpeakCommand;
    let cases: [(Trigger, Vec<&str>); 2] = [
        (
            || SpeakCommand::SetEngineEnabled(EngineId("beta".into()), false),
            vec!["gamma-2"],
        ),
        (
            || SpeakCommand::RefreshVoiceCatalog,
            vec!["beta-1", "gamma-2"],
        ),
    ];
    for (trigger, expected) in cases {
        let gate = gate();
        let q = start(&[
            ("beta", Listing::Ready),
            ("gamma", Listing::GatedOnce(Arc::clone(&gate))),
        ]);
        while q.listings[1].load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        q.handle.send(trigger()).await.unwrap();
        tokio::time::sleep(QUIESCENCE).await;
        assert_eq!(voice_ids(&q.handle), expected);

        gate.add_permits(1);
        tokio::time::sleep(QUIESCENCE).await;

        assert_eq!(
            voice_ids(&q.handle),
            expected,
            "a stale listing was applied"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn rebuilds_keep_settled_lists_until_their_new_listing_arrives() {
    type Trigger = fn() -> SpeakCommand;
    let cases: [(Trigger, Vec<&str>, Vec<&str>); 2] = [
        (
            || SpeakCommand::SetEngineEnabled(EngineId("beta".into()), false),
            vec!["gamma-1"],
            vec!["gamma-2"],
        ),
        (
            || SpeakCommand::RefreshVoiceCatalog,
            vec!["beta-1", "gamma-1"],
            vec!["beta-1", "gamma-2"],
        ),
    ];
    for (trigger, while_listing, after) in cases {
        let gate = gate();
        let mut q = start(&[
            ("beta", Listing::Ready),
            ("gamma", Listing::GatedAfterFirst(Arc::clone(&gate))),
        ]);
        tokio::time::sleep(QUIESCENCE).await;
        assert_eq!(voice_ids(&q.handle), vec!["beta-1", "gamma-1"]);

        q.handle.send(trigger()).await.unwrap();
        round_trip(&mut q).await;
        assert_eq!(voice_ids(&q.handle), while_listing);

        gate.add_permits(1);
        tokio::time::sleep(QUIESCENCE).await;
        assert_eq!(voice_ids(&q.handle), after);
    }
}

#[tokio::test(start_paused = true)]
async fn a_request_during_a_refresh_is_served_from_the_settled_lists() {
    let gate = gate();
    let mut q = start(&[
        ("beta", Listing::Ready),
        ("gamma", Listing::GatedAfterFirst(Arc::clone(&gate))),
    ]);
    tokio::time::sleep(QUIESCENCE).await;
    q.handle
        .send(SpeakCommand::RefreshVoiceCatalog)
        .await
        .unwrap();
    let req = request("viewer", "hello");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    let outcome = first_terminal(&mut q.stream, &id, QUIESCENCE).await;

    assert!(
        matches!(outcome, Some(SpeakEvent::Finished { .. })),
        "{outcome:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn each_settled_engine_is_published_at_once_in_engine_id_order() {
    let q = start(&[
        ("alpha", Listing::Slow(Duration::from_secs(3))),
        ("beta", Listing::Ready),
        ("gamma", Listing::Slow(Duration::from_secs(1))),
    ]);
    let mut seen = Vec::new();
    for at in [
        Duration::from_millis(500),
        Duration::from_secs(2),
        QUIESCENCE,
    ] {
        tokio::time::sleep_until(q.started_at + at).await;
        seen.push(voice_ids(&q.handle));
    }
    assert_eq!(
        seen,
        vec![
            vec!["beta-1".to_owned()],
            vec!["beta-1".to_owned(), "gamma-1".to_owned()],
            vec![
                "alpha-1".to_owned(),
                "beta-1".to_owned(),
                "gamma-1".to_owned()
            ],
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_waiting_head_request_keeps_an_override_behind_it_in_order() {
    let mut q = start(&[
        ("alpha", Listing::Ready),
        ("cloud", Listing::Slow(SLOW_CATALOG)),
    ]);
    let head = request("first", "waits for every catalog");
    let behind = request_with_overrides("second", Some("alpha"), Some("alpha-1"));
    let order = [head.request_id.clone(), behind.request_id.clone()];
    for req in [head, behind] {
        q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    let mut finished_order = Vec::new();
    while finished_order.len() < order.len() {
        if let SpeakEvent::Finished { request_id } = q.stream.recv().await.unwrap() {
            finished_order.push((request_id, q.started_at.elapsed()));
        }
    }

    assert_eq!(
        finished_order,
        vec![
            (order[0].clone(), SLOW_CATALOG),
            (order[1].clone(), SLOW_CATALOG)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_never_answering_engine_is_settled_empty_at_the_catalog_timeout() {
    let mut q = start(&[("alpha", Listing::Ready), ("cloud", Listing::Never)]);
    let req = request("viewer", "hello");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    let outcome = first_terminal(&mut q.stream, &id, NEVER_ANSWERING_PATIENCE).await;

    assert!(
        matches!(outcome, Some(SpeakEvent::Finished { .. })),
        "{outcome:?}"
    );
    assert_eq!(q.started_at.elapsed(), ENGINE_CATALOG_TIMEOUT);
    assert_eq!(voice_ids(&q.handle), vec!["alpha-1"]);
}

#[tokio::test(start_paused = true)]
async fn a_listing_just_inside_the_catalog_timeout_is_kept() {
    for (listing_takes, kept) in [
        (ENGINE_CATALOG_TIMEOUT - Duration::from_millis(1), true),
        (ENGINE_CATALOG_TIMEOUT + Duration::from_millis(1), false),
    ] {
        let q = start(&[
            ("alpha", Listing::Ready),
            ("cloud", Listing::Slow(listing_takes)),
        ]);
        tokio::time::sleep(ENGINE_CATALOG_TIMEOUT + QUIESCENCE).await;
        assert_eq!(
            voice_ids(&q.handle).contains(&"cloud-1".to_owned()),
            kept,
            "{listing_takes:?}"
        );
    }
}

fn calls(steps: &[Listing]) -> Listing {
    Listing::Calls(steps.to_vec().into())
}

fn register(q: &Started, id: &str, listing: Listing) {
    q.registry.write().unwrap().register(
        EngineId(id.into()),
        Arc::new(ScriptedFactory {
            id: EngineId(id.into()),
            listing,
            listings: Arc::new(AtomicUsize::new(0)),
        }),
    );
}

async fn settled_after(
    q: &mut Started,
    req: forge_speak_queue::SpeakRequest,
) -> (Duration, SpeakEvent) {
    let id = req.request_id.clone();
    let asked_at = tokio::time::Instant::now();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    let outcome = first_terminal(&mut q.stream, &id, NEVER_ANSWERING_PATIENCE)
        .await
        .expect("request never settled");
    (asked_at.elapsed(), outcome)
}

async fn served_after(q: &mut Started, req: forge_speak_queue::SpeakRequest) -> Duration {
    let (waited, outcome) = settled_after(q, req).await;
    assert!(
        matches!(outcome, SpeakEvent::Finished { .. }),
        "{outcome:?}"
    );
    waited
}

#[tokio::test(start_paused = true)]
async fn startup_serves_an_override_or_active_alias_once_its_engine_settles_and_others_wait() {
    let mut blocked = alias("viewer", "alpha", "alpha-1");
    blocked.state = forge_voice::AliasState::Blocked;
    let cases = [
        (
            "override alpha",
            request_with_overrides("viewer", Some("alpha"), Some("alpha-1")),
            vec![],
            Duration::ZERO,
        ),
        (
            "alias alpha",
            request("viewer", "hi"),
            vec![alias("viewer", "alpha", "alpha-1")],
            Duration::ZERO,
        ),
        (
            "alias cloud",
            request("viewer", "hi"),
            vec![alias("viewer", "cloud", "cloud-1")],
            SLOW_CATALOG,
        ),
        (
            "override cloud",
            request_with_overrides("viewer", Some("cloud"), Some("cloud-1")),
            vec![],
            SLOW_CATALOG,
        ),
        (
            "alias for another viewer",
            request("viewer", "hi"),
            vec![alias("someone", "alpha", "alpha-1")],
            SLOW_CATALOG,
        ),
        (
            "blocked alias",
            request("viewer", "hi"),
            vec![blocked],
            SLOW_CATALOG,
        ),
        ("no alias", request("viewer", "hi"), vec![], SLOW_CATALOG),
    ];
    for (label, req, aliases, expected) in cases {
        let mut q = start_with(
            &[
                ("alpha", Listing::Ready),
                ("cloud", Listing::Slow(SLOW_CATALOG)),
            ],
            aliases,
        );
        let (waited, _) = settled_after(&mut q, req).await;
        assert_eq!(waited, expected, "{label}");
    }
}

#[tokio::test(start_paused = true)]
async fn disabling_the_slow_engine_during_startup_releases_waiting_requests() {
    let mut q = start(&[("alpha", Listing::Ready), ("cloud", Listing::Never)]);
    let req = request("viewer", "hi");
    let id = req.request_id.clone();
    q.handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    q.handle
        .send(SpeakCommand::SetEngineEnabled(
            EngineId("cloud".into()),
            false,
        ))
        .await
        .unwrap();
    let outcome = first_terminal(&mut q.stream, &id, Duration::from_secs(1)).await;
    assert!(
        matches!(outcome, Some(SpeakEvent::Finished { .. })),
        "{outcome:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn after_startup_no_rebuild_holds_a_request_back() {
    for case in ["refresh", "re-enable", "new engine"] {
        let mut q = start(&[
            ("alpha", Listing::Ready),
            ("cloud", calls(&[Listing::Ready, Listing::Never])),
        ]);
        tokio::time::sleep(QUIESCENCE).await;
        match case {
            "refresh" => {
                q.handle
                    .send(SpeakCommand::RefreshVoiceCatalog)
                    .await
                    .unwrap();
            }
            "re-enable" => {
                for enabled in [false, true] {
                    q.handle
                        .send(SpeakCommand::SetEngineEnabled(
                            EngineId("cloud".into()),
                            enabled,
                        ))
                        .await
                        .unwrap();
                }
            }
            _ => {
                register(&q, "zeta", Listing::Never);
                q.handle
                    .send(SpeakCommand::RefreshVoiceCatalog)
                    .await
                    .unwrap();
            }
        }
        let waited = served_after(&mut q, request("viewer", "hi")).await;
        assert_eq!(waited, Duration::ZERO, "{case}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_new_engine_joins_the_catalog_when_its_list_arrives() {
    let q = start(&[("alpha", Listing::Ready)]);
    tokio::time::sleep(QUIESCENCE).await;
    register(&q, "zeta", Listing::Slow(Duration::from_secs(5)));
    q.handle
        .send(SpeakCommand::RefreshVoiceCatalog)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    let before = voice_ids(&q.handle);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        (before, voice_ids(&q.handle)),
        (
            vec!["alpha-1".to_owned()],
            vec!["alpha-1".to_owned(), "zeta-1".to_owned()]
        )
    );
}

#[tokio::test(start_paused = true)]
async fn a_refresh_keeps_or_replaces_an_engine_list_by_how_its_listing_ends() {
    for (second, expected) in [
        (Listing::Fails, vec!["alpha-1", "cloud-1"]),
        (Listing::Panics, vec!["alpha-1", "cloud-1"]),
        (Listing::Never, vec!["alpha-1", "cloud-1"]),
        (Listing::Empty, vec!["alpha-1"]),
        (Listing::Ready, vec!["alpha-1", "cloud-2"]),
    ] {
        let q = start(&[
            ("alpha", Listing::Ready),
            ("cloud", calls(&[Listing::Ready, second])),
        ]);
        tokio::time::sleep(QUIESCENCE).await;
        q.handle
            .send(SpeakCommand::RefreshVoiceCatalog)
            .await
            .unwrap();
        tokio::time::sleep(ENGINE_CATALOG_TIMEOUT + QUIESCENCE).await;
        assert_eq!(voice_ids(&q.handle), expected);
    }
}

#[tokio::test(start_paused = true)]
async fn a_re_enabled_engine_whose_listing_fails_has_no_voices() {
    for second in [Listing::Fails, Listing::Never] {
        let q = start(&[
            ("alpha", Listing::Ready),
            ("cloud", calls(&[Listing::Ready, second])),
        ]);
        tokio::time::sleep(QUIESCENCE).await;
        for enabled in [false, true] {
            q.handle
                .send(SpeakCommand::SetEngineEnabled(
                    EngineId("cloud".into()),
                    enabled,
                ))
                .await
                .unwrap();
        }
        tokio::time::sleep(ENGINE_CATALOG_TIMEOUT + QUIESCENCE).await;
        assert_eq!(voice_ids(&q.handle), vec!["alpha-1"]);
    }
}
