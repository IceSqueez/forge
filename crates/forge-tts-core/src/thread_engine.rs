use std::sync::mpsc as blocking_mpsc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice};
use tokio::sync::{mpsc, oneshot};

use crate::{SynthesisRequest, TtsEngine, TtsError};

pub const ABANDON_POLL_INTERVAL: Duration = Duration::from_millis(100);

const QUEUED_REQUEST_LIMIT: usize = 16;
const THREAD_STOPPED_DETAIL: &str = "engine worker thread stopped";

type SynthesisOutcome = Result<PcmBuffer, TtsError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsmlInput {
    Accepted,
    Refused,
}

#[derive(Debug, Clone)]
pub struct ThreadEngineSpec {
    pub engine_id: EngineId,
    pub thread_name: &'static str,
    pub ssml_input: SsmlInput,
    pub synthesis_budget: Duration,
}

pub struct AbandonWatch<'a> {
    deadline: Option<Instant>,
    requester_left: &'a dyn Fn() -> bool,
}

impl AbandonWatch<'_> {
    pub fn is_abandoned(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
            || (self.requester_left)()
    }
}

struct QueuedSynthesis {
    request: SynthesisRequest,
    deadline: Option<Instant>,
    reply: oneshot::Sender<Option<SynthesisOutcome>>,
}

pub struct ThreadEngine {
    id: EngineId,
    ssml_input: SsmlInput,
    synthesis_budget: Duration,
    voice_catalog: Vec<TtsVoice>,
    queued: mpsc::Sender<QueuedSynthesis>,
}

impl ThreadEngine {
    pub fn spawn<State, Open, Synthesize>(
        spec: ThreadEngineSpec,
        open: Open,
        mut synthesize: Synthesize,
    ) -> Result<Self, TtsError>
    where
        State: 'static,
        Open: FnOnce() -> Result<(State, Vec<TtsVoice>), TtsError> + Send + 'static,
        Synthesize: FnMut(&mut State, SynthesisRequest, &AbandonWatch<'_>) -> SynthesisOutcome
            + Send
            + 'static,
    {
        let ThreadEngineSpec {
            engine_id,
            thread_name,
            ssml_input,
            synthesis_budget,
        } = spec;
        let (queued, received) = mpsc::channel(QUEUED_REQUEST_LIMIT);
        let (opened_tx, opened_rx) = blocking_mpsc::sync_channel(1);

        std::thread::Builder::new()
            .name(thread_name.to_owned())
            .spawn(move || {
                let (catalog, state) = match open() {
                    Ok((state, catalog)) => (Ok(catalog), Some(state)),
                    Err(error) => (Err(error), None),
                };
                if opened_tx.send(catalog).is_ok()
                    && let Some(state) = state
                {
                    serve_queue(state, received, &mut synthesize);
                }
            })?;

        let voice_catalog = opened_rx.recv().map_err(|_| thread_stopped(&engine_id))??;
        Ok(Self {
            id: engine_id,
            ssml_input,
            synthesis_budget,
            voice_catalog,
            queued,
        })
    }

    async fn run_on_thread(&self, request: SynthesisRequest) -> SynthesisOutcome {
        let budget = self.synthesis_budget;
        let (reply, reply_rx) = oneshot::channel();
        let queued = QueuedSynthesis {
            request,
            deadline: Instant::now().checked_add(budget),
            reply,
        };
        let exchange = async {
            self.queued
                .send(queued)
                .await
                .map_err(|_| thread_stopped(&self.id))?;
            reply_rx.await.map_err(|_| thread_stopped(&self.id))
        };

        tokio::time::timeout(budget, exchange)
            .await
            .map_err(|_| TtsError::timeout_after(budget))??
            .unwrap_or_else(|| Err(TtsError::timeout_after(budget)))
    }
}

fn serve_queue<State>(
    mut state: State,
    mut received: mpsc::Receiver<QueuedSynthesis>,
    synthesize: &mut impl FnMut(&mut State, SynthesisRequest, &AbandonWatch<'_>) -> SynthesisOutcome,
) {
    while let Some(queued) = received.blocking_recv() {
        let QueuedSynthesis {
            request,
            deadline,
            reply,
        } = queued;
        let requester_left = || reply.is_closed();
        let watch = AbandonWatch {
            deadline,
            requester_left: &requester_left,
        };
        let outcome = (!watch.is_abandoned()).then(|| synthesize(&mut state, request, &watch));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the requester may stop waiting for the reply"
        )]
        let _ = reply.send(outcome);
    }
}

fn thread_stopped(id: &EngineId) -> TtsError {
    TtsError::EngineUnavailable {
        id: id.clone(),
        detail: THREAD_STOPPED_DETAIL.into(),
    }
}

#[async_trait]
impl TtsEngine for ThreadEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(self.voice_catalog.clone())
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if self.ssml_input == SsmlInput::Refused && request.text.is_ssml() {
            return Err(TtsError::SsmlUnsupported {
                id: self.id.clone(),
            });
        }
        if !self
            .voice_catalog
            .iter()
            .any(|voice| voice.id == request.voice_id)
        {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id,
            });
        }
        self.run_on_thread(request).await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::future::Future;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Waker};

    use forge_speech_types::{VoiceGender, VoiceId};

    use super::*;
    use crate::SpeechText;

    const WORKER_THREAD: &str = "thread-engine-test";
    const ENGINE: &str = "fake";
    const KNOWN_VOICE: &str = "known";
    const BUSY: &str = "busy";
    const LONG_BUDGET: Duration = Duration::from_secs(3600);
    const SHORT_BUDGET: Duration = Duration::from_millis(20);
    const HANG_GUARD: Duration = Duration::from_secs(120);
    const GATE_STOPS: usize = 2;

    fn engine_id() -> EngineId {
        EngineId(ENGINE.into())
    }

    fn spec(ssml_input: SsmlInput, synthesis_budget: Duration) -> ThreadEngineSpec {
        ThreadEngineSpec {
            engine_id: engine_id(),
            thread_name: WORKER_THREAD,
            ssml_input,
            synthesis_budget,
        }
    }

    fn voice(id: &str) -> TtsVoice {
        TtsVoice {
            id: VoiceId(id.into()),
            name: id.into(),
            locale: "en-US".into(),
            gender: VoiceGender::Neutral,
            engine_id: engine_id(),
            is_neural: false,
            sample_rate_hint: 22_050,
        }
    }

    fn pcm_of_len(len: usize) -> PcmBuffer {
        PcmBuffer::new(vec![0; len], 22_050, 1)
    }

    fn plain(text: &str) -> SynthesisRequest {
        SynthesisRequest::new(
            SpeechText::plain(text.into()),
            VoiceId(KNOWN_VOICE.into()),
            0.0,
            1.0,
        )
    }

    fn current_thread_name() -> String {
        std::thread::current().name().unwrap_or_default().to_owned()
    }

    fn budget_ms(budget: Duration) -> u64 {
        u64::try_from(budget.as_millis()).expect("test budgets fit in u64")
    }

    async fn guarded<F: Future>(future: F) -> F::Output {
        tokio::time::timeout(HANG_GUARD, future)
            .await
            .expect("the engine answered instead of hanging")
    }

    fn poll_once<F: Future + Unpin>(future: &mut F) -> bool {
        let mut context = Context::from_waker(Waker::noop());
        std::pin::Pin::new(future).poll(&mut context).is_pending()
    }

    struct GatedState {
        released: Option<oneshot::Sender<String>>,
    }

    impl Drop for GatedState {
        fn drop(&mut self) {
            if let Some(released) = self.released.take() {
                released.send(current_thread_name()).ok();
            }
        }
    }

    struct GatedWorker {
        entered: mpsc::UnboundedReceiver<String>,
        permits: blocking_mpsc::Sender<()>,
        abandoned_seen: mpsc::UnboundedReceiver<bool>,
        served: Arc<Mutex<Vec<String>>>,
        released: oneshot::Receiver<String>,
    }

    fn spawn_gated(ssml_input: SsmlInput, budget: Duration) -> (ThreadEngine, GatedWorker) {
        let (entered_tx, entered) = mpsc::unbounded_channel();
        let (permits, gate) = blocking_mpsc::channel::<()>();
        let (abandoned_tx, abandoned_seen) = mpsc::unbounded_channel();
        let (released_tx, released) = oneshot::channel();
        let served = Arc::new(Mutex::new(Vec::new()));
        let served_by_worker = Arc::clone(&served);

        let engine = ThreadEngine::spawn(
            spec(ssml_input, budget),
            move || {
                let state = GatedState {
                    released: Some(released_tx),
                };
                Ok((state, vec![voice(KNOWN_VOICE)]))
            },
            move |_state, request, watch| {
                let text = request.text.into_string();
                served_by_worker
                    .lock()
                    .expect("served log")
                    .push(text.clone());
                entered_tx.send(text.clone()).ok();
                for _ in 0..GATE_STOPS {
                    gate.recv().ok();
                    abandoned_tx.send(watch.is_abandoned()).ok();
                }
                Ok(pcm_of_len(text.len()))
            },
        )
        .expect("the worker opens");

        (
            engine,
            GatedWorker {
                entered,
                permits,
                abandoned_seen,
                served,
                released,
            },
        )
    }

    async fn engine_with_busy_worker() -> (ThreadEngine, GatedWorker, Duration) {
        let mut budget = SHORT_BUDGET;
        loop {
            let (engine, mut worker) = spawn_gated(SsmlInput::Refused, budget);
            let mut busy = engine.synthesize(plain(BUSY));
            let worker_entered = tokio::select! {
                biased;
                entered = worker.entered.recv() => entered.is_some(),
                _ = &mut busy => false,
            };
            drop(busy);
            if worker_entered {
                return (engine, worker, budget);
            }
            budget *= 2;
        }
    }

    async fn served_after_worker_exit(engine: ThreadEngine, worker: GatedWorker) -> Vec<String> {
        let GatedWorker {
            permits,
            served,
            released,
            ..
        } = worker;
        drop(permits);
        drop(engine);
        guarded(released).await.expect("state released");
        served.lock().expect("served log").clone()
    }

    #[tokio::test]
    async fn open_and_synthesize_share_non_send_state_on_the_named_thread() {
        let threads = Arc::new(Mutex::new(Vec::new()));
        let threads_at_open = Arc::clone(&threads);
        let threads_at_synthesis = Arc::clone(&threads);
        let engine = ThreadEngine::spawn(
            spec(SsmlInput::Refused, LONG_BUDGET),
            move || {
                threads_at_open
                    .lock()
                    .expect("thread log")
                    .push(current_thread_name());
                Ok((Rc::new(Cell::new(0_usize)), vec![voice(KNOWN_VOICE)]))
            },
            move |calls: &mut Rc<Cell<usize>>, _request, _watch| {
                threads_at_synthesis
                    .lock()
                    .expect("thread log")
                    .push(current_thread_name());
                calls.set(calls.get() + 1);
                Ok(pcm_of_len(calls.get()))
            },
        )
        .expect("the worker opens");

        let first = guarded(engine.synthesize(plain("one"))).await.expect("pcm");
        let second = guarded(engine.synthesize(plain("two"))).await.expect("pcm");

        assert_eq!((first.samples.len(), second.samples.len()), (1, 2));
        assert_eq!(*threads.lock().expect("thread log"), [WORKER_THREAD; 3]);
    }

    #[test]
    fn spawn_returns_the_error_open_failed_with() {
        let spawned = ThreadEngine::spawn(
            spec(SsmlInput::Refused, LONG_BUDGET),
            || -> Result<((), Vec<TtsVoice>), TtsError> {
                Err(TtsError::AuthFailed {
                    reason: "no licence".into(),
                })
            },
            |_state, _request, _watch| Ok(pcm_of_len(0)),
        );

        assert!(
            matches!(&spawned, Err(TtsError::AuthFailed { reason }) if reason == "no licence"),
            "expected the error from open"
        );
    }

    #[test]
    fn a_panic_in_open_reports_the_engine_unavailable_from_spawn() {
        let spawned = ThreadEngine::spawn(
            spec(SsmlInput::Refused, LONG_BUDGET),
            || -> Result<((), Vec<TtsVoice>), TtsError> { panic!("voice list crashed") },
            |_state, _request, _watch| Ok(pcm_of_len(0)),
        );

        assert!(
            matches!(&spawned, Err(TtsError::EngineUnavailable { id, .. }) if *id == engine_id()),
            "expected EngineUnavailable for the fake engine"
        );
    }

    #[tokio::test]
    async fn a_synthesis_past_the_budget_times_out_with_the_budget_in_ms() {
        let (engine, _worker) = spawn_gated(SsmlInput::Refused, SHORT_BUDGET);

        let outcome = guarded(engine.synthesize(plain("stuck"))).await;

        assert!(
            matches!(outcome, Err(TtsError::Timeout { ms }) if ms == budget_ms(SHORT_BUDGET)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_zero_budget_times_out_without_reaching_synthesize() {
        let (engine, worker) = spawn_gated(SsmlInput::Refused, Duration::ZERO);

        let outcome = guarded(engine.synthesize(plain("late"))).await;

        assert!(
            matches!(outcome, Err(TtsError::Timeout { ms: 0 })),
            "{outcome:?}"
        );
        assert!(served_after_worker_exit(engine, worker).await.is_empty());
    }

    #[tokio::test]
    async fn a_budget_past_the_clock_range_still_serves_the_request() {
        let (engine, worker) = spawn_gated(SsmlInput::Refused, Duration::MAX);
        drop(worker.permits);

        let outcome = guarded(engine.synthesize(plain("abc"))).await;

        assert_eq!(outcome.expect("pcm").samples.len(), "abc".len());
    }

    #[test]
    fn the_worker_skips_a_queued_request_only_when_it_is_abandoned() {
        let live_deadline = || Instant::now().checked_add(LONG_BUDGET);
        let expired_deadline = || Some(Instant::now());
        for (deadline, requester_left, reaches_synthesize) in [
            (None, false, true),
            (live_deadline(), false, true),
            (expired_deadline(), false, false),
            (live_deadline(), true, false),
            (None, true, false),
        ] {
            let (queue, received) = mpsc::channel(1);
            let (reply, mut reply_rx) = oneshot::channel();
            queue
                .try_send(QueuedSynthesis {
                    request: plain("queued"),
                    deadline,
                    reply,
                })
                .expect("room in the queue");
            drop(queue);
            if requester_left {
                reply_rx.close();
            }
            let mut synthesized = 0;

            serve_queue((), received, &mut |_state, _request, _watch| {
                synthesized += 1;
                Ok(pcm_of_len(1))
            });

            assert_eq!(
                synthesized == 1,
                reaches_synthesize,
                "deadline={deadline:?} requester_left={requester_left}"
            );
            if !requester_left {
                let answered = reply_rx.try_recv().expect("the worker replied");
                assert_eq!(answered.is_some(), reaches_synthesize);
            }
        }
    }

    #[test]
    fn abandon_watch_trips_on_an_expired_deadline_or_a_departed_requester() {
        let live_deadline = || Instant::now().checked_add(LONG_BUDGET);
        let expired_deadline = || Some(Instant::now());
        for (deadline, requester_left, abandoned) in [
            (None, false, false),
            (live_deadline(), false, false),
            (expired_deadline(), false, true),
            (None, true, true),
            (live_deadline(), true, true),
        ] {
            let left = || requester_left;
            let watch = AbandonWatch {
                deadline,
                requester_left: &left,
            };

            assert_eq!(
                watch.is_abandoned(),
                abandoned,
                "deadline={deadline:?} requester_left={requester_left}"
            );
        }
    }

    #[tokio::test]
    async fn a_request_dropped_in_the_queue_never_reaches_synthesize() {
        let (engine, mut worker) = spawn_gated(SsmlInput::Refused, LONG_BUDGET);
        let mut busy = engine.synthesize(plain(BUSY));
        assert!(poll_once(&mut busy));
        guarded(worker.entered.recv())
            .await
            .expect("worker entered");
        let mut dropped = engine.synthesize(plain("dropped"));
        assert!(poll_once(&mut dropped));

        drop(dropped);
        drop(busy);

        assert_eq!(served_after_worker_exit(engine, worker).await, [BUSY]);
    }

    #[tokio::test]
    async fn a_request_timed_out_in_the_queue_never_reaches_synthesize() {
        let (engine, worker, budget) = guarded(engine_with_busy_worker()).await;

        let outcome = guarded(engine.synthesize(plain("expired"))).await;

        assert!(
            matches!(outcome, Err(TtsError::Timeout { ms }) if ms == budget_ms(budget)),
            "{outcome:?}"
        );
        assert_eq!(served_after_worker_exit(engine, worker).await, [BUSY]);
    }

    #[tokio::test]
    async fn aborting_the_caller_abandons_the_running_synthesis_and_the_next_request_succeeds() {
        let (engine, mut worker) = spawn_gated(SsmlInput::Refused, LONG_BUDGET);
        let engine = Arc::new(engine);
        let caller = tokio::spawn({
            let engine = Arc::clone(&engine);
            async move { engine.synthesize(plain("aborted")).await }
        });
        guarded(worker.entered.recv())
            .await
            .expect("worker entered");
        worker.permits.send(()).expect("worker at the gate");
        let before_abort = guarded(worker.abandoned_seen.recv()).await;

        caller.abort();
        assert!(guarded(caller).await.expect_err("aborted").is_cancelled());
        worker.permits.send(()).expect("worker at the gate");
        let after_abort = guarded(worker.abandoned_seen.recv()).await;
        drop(worker.permits);
        let next = guarded(engine.synthesize(plain("next"))).await;

        assert_eq!((before_abort, after_abort), (Some(false), Some(true)));
        assert_eq!(next.expect("pcm").samples.len(), "next".len());
    }

    #[tokio::test]
    async fn a_panic_in_synthesize_reports_the_engine_unavailable_to_every_later_request() {
        let engine = ThreadEngine::spawn(
            spec(SsmlInput::Refused, LONG_BUDGET),
            || Ok(((), vec![voice(KNOWN_VOICE)])),
            |_state, _request, _watch| -> SynthesisOutcome { panic!("speech driver crashed") },
        )
        .expect("the worker opens");

        for attempt in ["crashing", "after the crash"] {
            let outcome = guarded(engine.synthesize(plain(attempt))).await;

            assert!(
                matches!(&outcome, Err(TtsError::EngineUnavailable { id, .. }) if *id == engine_id()),
                "{attempt}: {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn dropping_an_idle_engine_releases_the_state_on_the_worker_thread() {
        let (engine, worker) = spawn_gated(SsmlInput::Refused, LONG_BUDGET);

        drop(engine);
        let released_on = guarded(worker.released).await;

        assert_eq!(released_on.as_deref(), Ok(WORKER_THREAD));
    }

    #[tokio::test]
    async fn dropping_the_engine_mid_synthesis_releases_the_state_after_it_returns() {
        let (engine, mut worker) = spawn_gated(SsmlInput::Refused, LONG_BUDGET);
        let mut running = engine.synthesize(plain("running"));
        poll_once(&mut running);
        guarded(worker.entered.recv())
            .await
            .expect("worker entered");
        drop(running);
        drop(engine);

        assert!(worker.released.try_recv().is_err());
        drop(worker.permits);
        assert!(guarded(worker.released).await.is_ok());
    }

    #[tokio::test]
    async fn requests_over_the_queue_limit_wait_and_are_served_in_order_once_room_opens() {
        let (engine, mut worker) = spawn_gated(SsmlInput::Refused, LONG_BUDGET);
        let texts: Vec<String> = (1..=QUEUED_REQUEST_LIMIT + 2)
            .map(|len| "x".repeat(len))
            .collect();
        let mut callers: Vec<_> = texts
            .iter()
            .map(|text| engine.synthesize(plain(text)))
            .collect();
        let Some((over_limit, within_limit)) = callers.split_last_mut() else {
            panic!("callers were built above");
        };

        assert!(poll_once(&mut within_limit[0]));
        guarded(worker.entered.recv())
            .await
            .expect("worker entered");
        let queued_all = within_limit[1..].iter_mut().all(poll_once);
        let room_left = engine.queued.capacity();
        let over_limit_waits = poll_once(over_limit);
        drop(worker.permits);

        let mut replied_lens = Vec::new();
        for caller in callers {
            replied_lens.push(guarded(caller).await.expect("pcm").samples.len());
        }

        assert!(queued_all);
        assert_eq!(room_left, 0);
        assert!(over_limit_waits);
        assert_eq!(replied_lens, (1..=texts.len()).collect::<Vec<_>>());
        assert_eq!(*worker.served.lock().expect("served log"), texts);
    }

    #[tokio::test]
    async fn a_request_stuck_behind_a_full_queue_times_out_within_the_budget() {
        let (engine, _worker, budget) = guarded(engine_with_busy_worker()).await;
        let mut queued: Vec<_> = (0..QUEUED_REQUEST_LIMIT)
            .map(|_| engine.synthesize(plain("queued")))
            .collect();
        for caller in &mut queued {
            poll_once(caller);
        }
        let room_left = engine.queued.capacity();

        let outcome = guarded(engine.synthesize(plain("overflow"))).await;

        assert_eq!(room_left, 0);
        assert!(
            matches!(outcome, Err(TtsError::Timeout { ms }) if ms == budget_ms(budget)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn the_engine_refuses_ssml_and_unknown_voices_before_the_thread() {
        #[derive(Debug)]
        enum Expected {
            Served,
            RefusedSsml,
            RefusedVoice,
        }
        const UNKNOWN_VOICE: &str = "unknown";
        for (ssml_input, is_ssml, voice_id, expected) in [
            (SsmlInput::Refused, false, KNOWN_VOICE, Expected::Served),
            (SsmlInput::Accepted, false, KNOWN_VOICE, Expected::Served),
            (SsmlInput::Accepted, true, KNOWN_VOICE, Expected::Served),
            (SsmlInput::Refused, true, KNOWN_VOICE, Expected::RefusedSsml),
            (
                SsmlInput::Refused,
                true,
                UNKNOWN_VOICE,
                Expected::RefusedSsml,
            ),
            (
                SsmlInput::Refused,
                false,
                UNKNOWN_VOICE,
                Expected::RefusedVoice,
            ),
            (
                SsmlInput::Accepted,
                true,
                UNKNOWN_VOICE,
                Expected::RefusedVoice,
            ),
        ] {
            let (engine, worker) = spawn_gated(ssml_input, LONG_BUDGET);
            for _ in 0..GATE_STOPS {
                worker.permits.send(()).expect("worker alive");
            }
            let request = SynthesisRequest::new(
                SpeechText::new("<speak>hi</speak>".into(), is_ssml),
                VoiceId(voice_id.into()),
                0.0,
                1.0,
            );

            let outcome = guarded(engine.synthesize(request)).await;

            let case = format!("{ssml_input:?} ssml={is_ssml} {voice_id}: {outcome:?}");
            let reaches_thread = match expected {
                Expected::Served => {
                    assert!(outcome.is_ok(), "{case}");
                    true
                }
                Expected::RefusedSsml => {
                    assert!(
                        matches!(&outcome, Err(TtsError::SsmlUnsupported { id }) if *id == engine_id()),
                        "{case}"
                    );
                    false
                }
                Expected::RefusedVoice => {
                    assert!(
                        matches!(&outcome, Err(TtsError::InvalidVoice { id }) if id.0 == voice_id),
                        "{case}"
                    );
                    false
                }
            };
            assert_eq!(
                served_after_worker_exit(engine, worker).await.len(),
                usize::from(reaches_thread),
                "{case}"
            );
        }
    }
}
