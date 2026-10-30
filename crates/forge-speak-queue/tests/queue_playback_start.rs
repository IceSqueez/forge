#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioSink, ControlledPlayback, RemoteAudioDestination, RemoteClip, RemoteCommand,
    RemoteDelivery, RemoteDestinationId, RemoteSink, RemoteVerdict,
};
use forge_speak_queue::{
    Priority, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakQueueHandle,
};
use forge_speech_types::{AssignmentStrategy, PcmBuffer};
use forge_types::RemoteClipId;
use tokio::sync::{Notify, mpsc, oneshot};

use common::{make_deps, request, standard_registry, wait_for};

const DESTINATION: &str = "stage-audio";
const COMMAND_WAIT: Duration = Duration::from_secs(30);
const PAST_THE_START_BOUND: Duration = Duration::from_secs(6);
const PAST_ONE_CONTROL_POLL: Duration = Duration::from_millis(200);

enum Start {
    Hang,
    Immediately,
    Gated(oneshot::Receiver<()>),
    Panics,
}

struct ScriptedSink {
    script: Mutex<VecDeque<(Start, oneshot::Receiver<()>)>>,
    entered: mpsc::UnboundedSender<usize>,
    calls: AtomicUsize,
    live: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl ScriptedSink {
    fn new(
        script: Vec<(Start, oneshot::Receiver<()>)>,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<usize>) {
        let (entered, arrivals) = mpsc::unbounded_channel();
        let sink = Arc::new(Self {
            script: Mutex::new(script.into()),
            entered,
            calls: AtomicUsize::new(0),
            live: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
        });
        (sink, arrivals)
    }
}

#[async_trait]
impl AudioSink for ScriptedSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }

    async fn play_controlled(&self, _buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let live_now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(live_now, Ordering::SeqCst);
        let scripted = self.script.lock().unwrap().pop_front();
        self.entered.send(call).unwrap();
        let (start, finish) = scripted.expect("every start is scripted");
        match start {
            Start::Hang => std::future::pending::<()>().await,
            Start::Immediately => {}
            Start::Gated(gate) => gate.await.unwrap(),
            Start::Panics => panic!("the device driver blew up"),
        }
        let live = Arc::clone(&self.live);
        Ok(ControlledPlayback::from_future(async move {
            finish.await.unwrap();
            live.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }))
    }
}

fn finishes_at_once() -> oneshot::Receiver<()> {
    let (done, finish) = oneshot::channel();
    done.send(()).unwrap();
    finish
}

fn spawn_queue(sink: Arc<dyn AudioSink>) -> (SpeakQueueHandle, SpeakEventStream) {
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

async fn events_until_pause_marker(
    handle: &SpeakQueueHandle,
    stream: &mut SpeakEventStream,
) -> Vec<SpeakEvent> {
    handle.send(SpeakCommand::Pause).await.unwrap();
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(COMMAND_WAIT, stream.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event, SpeakEvent::Paused) {
            return seen;
        }
        seen.push(event);
    }
}

#[tokio::test(start_paused = true)]
async fn commands_are_answered_while_the_sink_hangs_on_starting_a_clip() {
    let (sink, mut entered) = ScriptedSink::new(vec![(Start::Hang, finishes_at_once())]);
    let (handle, mut stream) = spawn_queue(sink);
    let waiting = request("viewer", "enqueued while the device hangs");
    let waiting_id = waiting.request_id.clone();

    handle
        .send(SpeakCommand::Enqueue(request("viewer", "hangs")))
        .await
        .unwrap();
    entered.recv().await.unwrap();
    let hang_began = tokio::time::Instant::now();
    handle.send(SpeakCommand::Enqueue(waiting)).await.unwrap();
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Enqueued { request_id, .. } if *request_id == waiting_id),
    )
.await;
    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |event| matches!(event, SpeakEvent::Paused)).await;

    assert_eq!(
        hang_began.elapsed(),
        Duration::ZERO,
        "the actor must not wait for the start before answering"
    );
}

#[tokio::test(start_paused = true)]
async fn a_skip_during_a_hung_start_lands_at_once_and_the_abandoned_start_reports_nothing() {
    let (sink, mut entered) = ScriptedSink::new(vec![
        (Start::Hang, finishes_at_once()),
        (Start::Immediately, finishes_at_once()),
    ]);
    let (handle, mut stream) = spawn_queue(sink);
    let hung = request("viewer", "hangs");
    let hung_id = hung.request_id.clone();
    let next = request("viewer", "plays right after the skip");
    let next_id = next.request_id.clone();

    handle.send(SpeakCommand::Enqueue(hung)).await.unwrap();
    entered.recv().await.unwrap();
    handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    let skipped_at = tokio::time::Instant::now();
    handle.send(SpeakCommand::Skip).await.unwrap();
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Skipped { request_id, .. } if *request_id == hung_id),
    )
    .await;
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Finished { request_id } if *request_id == next_id),
    )
    .await;
    assert_eq!(skipped_at.elapsed(), Duration::ZERO);

    tokio::time::sleep(PAST_THE_START_BOUND).await;
    let later = events_until_pause_marker(&handle, &mut stream).await;
    assert!(
        !later.iter().any(
            |event| matches!(event, SpeakEvent::Failed { request_id, .. } if *request_id == hung_id)
        ),
        "a skipped utterance must not also fail when its start times out, got {later:?}"
    );
}

#[tokio::test]
async fn a_start_that_panics_fails_only_its_own_utterance() {
    let (sink, _entered) = ScriptedSink::new(vec![
        (Start::Panics, finishes_at_once()),
        (Start::Immediately, finishes_at_once()),
    ]);
    let (handle, mut stream) = spawn_queue(sink);
    let doomed = request("viewer", "panics");
    let doomed_id = doomed.request_id.clone();
    let next = request("viewer", "still plays");
    let next_id = next.request_id.clone();

    handle.send(SpeakCommand::Enqueue(doomed)).await.unwrap();
    handle.send(SpeakCommand::Enqueue(next)).await.unwrap();

    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Failed { request_id, .. } if *request_id == doomed_id),
    )
    .await;
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Finished { request_id } if *request_id == next_id),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn utterances_never_overlap_and_keep_priority_order_across_a_slow_start() {
    let (open_gate, gate) = oneshot::channel();
    let (finish_first, first_finish) = oneshot::channel();
    let (finish_second, second_finish) = oneshot::channel();
    let (sink, mut entered) = ScriptedSink::new(vec![
        (Start::Gated(gate), first_finish),
        (Start::Immediately, second_finish),
        (Start::Immediately, finishes_at_once()),
    ]);
    let peak = Arc::clone(&sink.peak);
    let (handle, mut stream) = spawn_queue(sink);

    let first = request("first", "slow to start");
    let normal = request("second", "normal priority");
    let mut urgent = request("third", "high priority");
    urgent.priority = Priority::High;
    let expected = [
        first.request_id.clone(),
        urgent.request_id.clone(),
        normal.request_id.clone(),
    ];
    let first_id = first.request_id.clone();

    handle.send(SpeakCommand::Enqueue(first)).await.unwrap();
    entered.recv().await.unwrap();
    handle.send(SpeakCommand::Enqueue(normal)).await.unwrap();
    handle.send(SpeakCommand::Enqueue(urgent)).await.unwrap();
    open_gate.send(()).unwrap();
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Progress { request_id, .. } if *request_id == first_id),
    )
    .await;
    assert!(
        entered.try_recv().is_err(),
        "nothing else may reach the sink while the first clip plays"
    );

    finish_first.send(()).unwrap();
    assert_eq!(entered.recv().await, Some(1));
    finish_second.send(()).unwrap();

    let mut finished: Vec<RequestId> = Vec::new();
    while finished.len() < expected.len() {
        let event = tokio::time::timeout(COMMAND_WAIT, stream.recv())
            .await
            .unwrap()
            .unwrap();
        if let SpeakEvent::Finished { request_id } = event {
            finished.push(request_id);
        }
    }
    assert_eq!(finished, expected);
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

struct GatedPage {
    gates: Mutex<VecDeque<Option<oneshot::Receiver<()>>>>,
    delivered: AtomicUsize,
    arrivals: mpsc::UnboundedSender<String>,
    commands: mpsc::UnboundedSender<(String, RemoteCommand)>,
    settle: Notify,
}

struct Page {
    sink: Arc<dyn AudioSink>,
    page: Arc<GatedPage>,
    arrivals: mpsc::UnboundedReceiver<String>,
    commands: mpsc::UnboundedReceiver<(String, RemoteCommand)>,
}

fn page_with(gates: Vec<Option<oneshot::Receiver<()>>>) -> Page {
    let (arrived, arrivals) = mpsc::unbounded_channel();
    let (pushed, commands) = mpsc::unbounded_channel();
    let page = Arc::new(GatedPage {
        gates: Mutex::new(gates.into()),
        delivered: AtomicUsize::new(0),
        arrivals: arrived,
        commands: pushed,
        settle: Notify::new(),
    });
    let sink: Arc<dyn AudioSink> = Arc::new(RemoteSink::new(
        Arc::clone(&page) as Arc<dyn RemoteAudioDestination>,
        RemoteDestinationId::new(DESTINATION),
    ));
    Page {
        sink,
        page,
        arrivals,
        commands,
    }
}

#[async_trait]
impl RemoteAudioDestination for GatedPage {
    async fn deliver(
        &self,
        _destination: &RemoteDestinationId,
        _clip: RemoteClip,
    ) -> Result<RemoteDelivery, AudioError> {
        let clip = format!("clip-{}", self.delivered.fetch_add(1, Ordering::SeqCst));
        let gate = self.gates.lock().unwrap().pop_front().flatten();
        self.arrivals.send(clip.clone()).unwrap();
        if let Some(gate) = gate {
            gate.await.unwrap();
        }
        Ok(RemoteDelivery {
            clip_id: RemoteClipId::new(clip),
            live_players: 1,
        })
    }

    async fn control(
        &self,
        _destination: &RemoteDestinationId,
        clip_id: &RemoteClipId,
        command: RemoteCommand,
    ) -> Result<(), AudioError> {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.commands.send((clip_id.expose().to_owned(), command));
        Ok(())
    }

    async fn verdict(&self, clip_id: &RemoteClipId) -> Result<RemoteVerdict, AudioError> {
        if clip_id.expose() == "clip-0" {
            return std::future::pending().await;
        }
        self.settle.notified().await;
        Ok(RemoteVerdict::Played)
    }
}

async fn next_command(
    commands: &mut mpsc::UnboundedReceiver<(String, RemoteCommand)>,
) -> Option<(String, RemoteCommand)> {
    tokio::time::timeout(COMMAND_WAIT, commands.recv())
        .await
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn every_way_of_stopping_a_clip_mid_start_stops_it_on_the_page_exactly_once() {
    type StopCommand = fn(RequestId, RequestId) -> SpeakCommand;
    let stops: [StopCommand; 4] = [
        |_, _| SpeakCommand::Skip,
        |active, _| SpeakCommand::Cancel(active),
        |_, _| SpeakCommand::Clear,
        |_, queued| SpeakCommand::PlayNow(queued),
    ];
    for stop in stops {
        let (open_gate, gate) = oneshot::channel();
        let mut page = page_with(vec![Some(gate)]);
        let (handle, mut stream) = spawn_queue(Arc::clone(&page.sink));
        let active = request("troll", "a clip caught while it starts");
        let active_id = active.request_id.clone();
        let queued = request("viewer", "waiting behind it");
        let queued_id = queued.request_id.clone();

        handle.send(SpeakCommand::Enqueue(active)).await.unwrap();
        assert_eq!(page.arrivals.recv().await.as_deref(), Some("clip-0"));
        handle.send(SpeakCommand::Enqueue(queued)).await.unwrap();
        handle
            .send(stop(active_id.clone(), queued_id))
            .await
            .unwrap();
        wait_for(
            &mut stream,
            |event| matches!(event, SpeakEvent::Skipped { request_id, .. } if *request_id == active_id),
        )
.await;
        open_gate.send(()).unwrap();

        assert_eq!(
            next_command(&mut page.commands).await,
            Some(("clip-0".to_owned(), RemoteCommand::Stop))
        );
        let second_stop = tokio::time::timeout(PAST_ONE_CONTROL_POLL, page.commands.recv()).await;
        assert!(
            second_stop.is_err(),
            "the page must get one command, got {second_stop:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_pause_during_the_start_holds_the_clip_as_soon_as_it_lands() {
    let (open_gate, gate) = oneshot::channel();
    let mut page = page_with(vec![Some(gate)]);
    let (handle, mut stream) = spawn_queue(Arc::clone(&page.sink));

    handle
        .send(SpeakCommand::Enqueue(request("viewer", "paused mid start")))
        .await
        .unwrap();
    page.arrivals.recv().await.unwrap();
    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |event| matches!(event, SpeakEvent::Paused)).await;
    open_gate.send(()).unwrap();

    assert_eq!(
        next_command(&mut page.commands).await,
        Some(("clip-0".to_owned(), RemoteCommand::Pause))
    );
    handle.send(SpeakCommand::Resume).await.unwrap();
    assert_eq!(
        next_command(&mut page.commands).await,
        Some(("clip-0".to_owned(), RemoteCommand::Resume))
    );
}

#[tokio::test(start_paused = true)]
async fn a_pause_undone_during_the_start_leaves_the_clip_playing() {
    let (open_gate, gate) = oneshot::channel();
    let mut page = page_with(vec![Some(gate)]);
    let (handle, mut stream) = spawn_queue(Arc::clone(&page.sink));

    handle
        .send(SpeakCommand::Enqueue(request(
            "viewer",
            "paused then resumed",
        )))
        .await
        .unwrap();
    page.arrivals.recv().await.unwrap();
    handle.send(SpeakCommand::Pause).await.unwrap();
    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for(&mut stream, |event| matches!(event, SpeakEvent::Resumed)).await;
    open_gate.send(()).unwrap();
    wait_for(&mut stream, |event| {
        matches!(event, SpeakEvent::Progress { .. })
    })
    .await;
    handle.send(SpeakCommand::Skip).await.unwrap();

    assert_eq!(
        next_command(&mut page.commands).await,
        Some(("clip-0".to_owned(), RemoteCommand::Stop)),
        "a stop must be the only command the page sees"
    );
}

#[tokio::test(start_paused = true)]
async fn a_skipped_start_landing_during_the_next_clip_stops_only_the_skipped_clip() {
    let (open_gate, gate) = oneshot::channel();
    let mut page = page_with(vec![Some(gate), None]);
    let (handle, mut stream) = spawn_queue(Arc::clone(&page.sink));
    let next = request("viewer", "keeps playing");
    let next_id = next.request_id.clone();

    handle
        .send(SpeakCommand::Enqueue(request("troll", "skipped mid start")))
        .await
        .unwrap();
    page.arrivals.recv().await.unwrap();
    handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    handle.send(SpeakCommand::Skip).await.unwrap();
    assert_eq!(page.arrivals.recv().await.as_deref(), Some("clip-1"));
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Progress { request_id, .. } if *request_id == next_id),
    )
    .await;

    open_gate.send(()).unwrap();
    assert_eq!(
        next_command(&mut page.commands).await,
        Some(("clip-0".to_owned(), RemoteCommand::Stop))
    );

    page.page.settle.notify_one();
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Finished { request_id } if *request_id == next_id),
    )
    .await;
    let stray = page.commands.try_recv();
    assert!(
        stray.is_err(),
        "the playing clip must get no command, got {stray:?}"
    );
}
