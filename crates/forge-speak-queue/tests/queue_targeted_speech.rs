#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_audio::{
    AudioSink, PcmBuffer, PlaybackCorrelation, RemoteDestinationId, TargetedSinkFactory,
};
use forge_speak_queue::{
    PlaybackTarget, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakQueueHandle, SpeakRequest,
};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice,
};
use forge_voice::AssignmentStrategy;
use tokio::sync::Notify;

use common::{make_deps, recording_sink, request, standard_registry, voice, wait_for};

const WAIT_MS: u64 = 2_000;
const QUIET_MS: u64 = 300;

struct RecordingLegs {
    targets: Mutex<Vec<PlaybackTarget>>,
    legs: Arc<dyn AudioSink>,
    plays: Arc<Mutex<usize>>,
}

impl RecordingLegs {
    fn new() -> Arc<Self> {
        let (legs, plays) = recording_sink();
        Arc::new(Self {
            targets: Mutex::new(Vec::new()),
            legs,
            plays,
        })
    }

    fn plays(&self) -> usize {
        *self.plays.lock().unwrap()
    }
}

impl TargetedSinkFactory for RecordingLegs {
    fn sink_for(&self, target: &PlaybackTarget) -> Arc<dyn AudioSink> {
        self.targets.lock().unwrap().push(target.clone());
        Arc::clone(&self.legs)
    }
}

fn target() -> PlaybackTarget {
    PlaybackTarget {
        destination: RemoteDestinationId::new("stage-alert"),
        correlation: PlaybackCorrelation::new("01J9ZC4W6R7Q2N3M4K5P6S7T8V"),
    }
}

fn targeted(text: &str) -> SpeakRequest {
    SpeakRequest {
        target: Some(target()),
        ..request("donor", text)
    }
}

fn is_terminal_for(event: &SpeakEvent, id: &RequestId) -> bool {
    match event {
        SpeakEvent::Finished { request_id }
        | SpeakEvent::Failed { request_id, .. }
        | SpeakEvent::Skipped { request_id, .. }
        | SpeakEvent::Removed { request_id }
        | SpeakEvent::Rejected { request_id, .. } => request_id == id,
        _ => false,
    }
}

async fn terminal_of(stream: &mut SpeakEventStream, id: &RequestId) -> SpeakEvent {
    let deadline = std::time::Duration::from_millis(WAIT_MS);
    tokio::time::timeout(deadline, async {
        loop {
            match stream.recv().await {
                Ok(event) if is_terminal_for(&event, id) => return event,
                Ok(_) => continue,
                Err(e) => panic!("the event stream ended before the request did: {e:?}"),
            }
        }
    })
    .await
    .expect("the request never reached a terminal outcome")
}

#[tokio::test]
async fn targeted_speech_plays_only_through_its_target_legs_never_the_queue_sink() {
    let (global, global_plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        global,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let legs = RecordingLegs::new();
    handle.install_targeted_legs(legs.clone());

    let speech = targeted("thanks for the five");
    let id = speech.request_id.clone();
    handle.send(SpeakCommand::Enqueue(speech)).await.unwrap();

    assert!(
        matches!(
            terminal_of(&mut stream, &id).await,
            SpeakEvent::Finished { .. }
        ),
        "a targeted request with legs installed did not play to the end"
    );
    assert_eq!(
        (
            *global_plays.lock().unwrap(),
            legs.plays(),
            legs.targets.lock().unwrap().clone()
        ),
        (0, 1, vec![target()]),
        "show speech reached the global receiver too, or its legs were built for another target"
    );
}

#[tokio::test]
async fn targeted_speech_without_legs_fails_and_never_falls_back_to_the_queue_sink() {
    let (global, global_plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        global,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    let speech = targeted("thanks for the five");
    let id = speech.request_id.clone();
    handle.send(SpeakCommand::Enqueue(speech)).await.unwrap();

    let outcome = terminal_of(&mut stream, &id).await;
    assert!(
        matches!(outcome, SpeakEvent::Failed { .. }) && *global_plays.lock().unwrap() == 0,
        "a targeted request with no legs ended as {outcome:?} with {} plays on the global sink",
        global_plays.lock().unwrap()
    );
}

#[tokio::test]
async fn untargeted_speech_keeps_the_queue_sink_while_targeted_legs_are_installed() {
    let (global, global_plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        global,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let legs = RecordingLegs::new();
    handle.install_targeted_legs(legs.clone());

    let chat = request("viewer", "hello chat");
    let id = chat.request_id.clone();
    handle.send(SpeakCommand::Enqueue(chat)).await.unwrap();

    assert!(matches!(
        terminal_of(&mut stream, &id).await,
        SpeakEvent::Finished { .. }
    ));
    assert_eq!(
        (*global_plays.lock().unwrap(), legs.plays()),
        (1, 0),
        "ordinary speech left the global route once show legs were installed"
    );
}

#[tokio::test]
async fn a_replayed_show_speech_plays_on_its_overlay_again_never_on_the_global_receiver() {
    let (global, global_plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        global,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let legs = RecordingLegs::new();
    handle.install_targeted_legs(legs.clone());
    let speech = targeted("thanks for the five");
    let id = speech.request_id.clone();
    handle.send(SpeakCommand::Enqueue(speech)).await.unwrap();
    terminal_of(&mut stream, &id).await;

    handle.send(SpeakCommand::Replay).await.unwrap();
    let replayed = loop {
        match stream.recv().await {
            Ok(SpeakEvent::Enqueued { request_id, .. }) => break request_id,
            Ok(_) => continue,
            Err(e) => panic!("the replay was never admitted: {e:?}"),
        }
    };
    let outcome = terminal_of(&mut stream, &replayed).await;

    assert_eq!(
        (
            matches!(outcome, SpeakEvent::Finished { .. }),
            *global_plays.lock().unwrap(),
            legs.targets.lock().unwrap().clone()
        ),
        (true, 0, vec![target(), target()]),
        "a replayed show speech lost its overlay target and reached the global receiver"
    );
}

struct GatedEngine {
    gate: Arc<Notify>,
}

#[async_trait]
impl TtsEngine for GatedEngine {
    fn engine_id(&self) -> &EngineId {
        static ID: std::sync::OnceLock<EngineId> = std::sync::OnceLock::new();
        ID.get_or_init(|| EngineId("gate".into()))
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
        Ok(vec![voice("g-voice", "gate")])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        self.gate.notified().await;
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct GatedFactory {
    gate: Arc<Notify>,
}

impl TtsEngineFactory for GatedFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(GatedEngine {
            gate: self.gate.clone(),
        }))
    }
}

/// One request held mid-synthesis at the head; everything enqueued after it waits.
async fn held_queue() -> (SpeakQueueHandle, SpeakEventStream, Arc<Notify>, RequestId) {
    let gate = Arc::new(Notify::new());
    let mut registry = TtsRegistry::new();
    registry.register(
        EngineId("gate".into()),
        Arc::new(GatedFactory { gate: gate.clone() }),
    );
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let head = request("viewer", "on screen now");
    let head_id = head.request_id.clone();
    handle.send(SpeakCommand::Enqueue(head)).await.unwrap();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Started { request_id, .. } if *request_id == head_id),
        WAIT_MS,
    )
    .await;
    (handle, stream, gate, head_id)
}

async fn enqueue_waiting(handle: &SpeakQueueHandle, count: usize) -> Vec<RequestId> {
    let mut ids = Vec::new();
    for index in 0..count {
        let waiting = targeted(&format!("waiting {index}"));
        ids.push(waiting.request_id.clone());
        handle.send(SpeakCommand::Enqueue(waiting)).await.unwrap();
    }
    ids
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Finished,
    Failed,
    Skipped,
    Removed,
    Rejected,
}

/// Every terminal outcome seen per request until the queue has been quiet for a while.
async fn terminals_until_quiet(stream: &mut SpeakEventStream) -> HashMap<RequestId, Vec<Terminal>> {
    let mut seen: HashMap<RequestId, Vec<Terminal>> = HashMap::new();
    let quiet = std::time::Duration::from_millis(QUIET_MS);
    while let Ok(Ok(event)) = tokio::time::timeout(quiet, stream.recv()).await {
        let (id, terminal) = match event {
            SpeakEvent::Finished { request_id } => (request_id, Terminal::Finished),
            SpeakEvent::Failed { request_id, .. } => (request_id, Terminal::Failed),
            SpeakEvent::Skipped { request_id, .. } => (request_id, Terminal::Skipped),
            SpeakEvent::Removed { request_id } => (request_id, Terminal::Removed),
            SpeakEvent::Rejected { request_id, .. } => (request_id, Terminal::Rejected),
            _ => continue,
        };
        seen.entry(id).or_default().push(terminal);
    }
    seen
}

#[tokio::test]
async fn cancel_ends_a_waiting_request_as_removed_and_the_active_one_as_skipped_exactly_once() {
    let (handle, mut stream, gate, head) = held_queue().await;
    let waiting = enqueue_waiting(&handle, 1).await.remove(0);

    handle
        .send(SpeakCommand::Cancel(waiting.clone()))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Cancel(head.clone()))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::Cancel(head.clone()))
        .await
        .unwrap();
    gate.notify_one();

    let seen = terminals_until_quiet(&mut stream).await;
    assert_eq!(
        (seen.get(&waiting).cloned(), seen.get(&head).cloned()),
        (Some(vec![Terminal::Removed]), Some(vec![Terminal::Skipped])),
        "a cancelled request did not end exactly once, or a repeated cancel reached it again"
    );
}

#[tokio::test]
async fn clearing_ends_every_dropped_request_as_removed_so_no_waiter_hangs() {
    for (command, head_ends_as) in [
        (SpeakCommand::Clear, Terminal::Skipped),
        (SpeakCommand::ClearPending, Terminal::Finished),
    ] {
        let label = format!("{command:?}");
        let (handle, mut stream, gate, head) = held_queue().await;
        let waiting = enqueue_waiting(&handle, 2).await;

        handle.send(command).await.unwrap();
        gate.notify_one();

        let seen = terminals_until_quiet(&mut stream).await;
        for id in &waiting {
            assert_eq!(
                seen.get(id).cloned(),
                Some(vec![Terminal::Removed]),
                "{label}: a dropped request never reached exactly one terminal outcome"
            );
        }
        assert_eq!(
            seen.get(&head).cloned(),
            Some(vec![head_ends_as]),
            "{label}: the request at the head ended the wrong way or more than once"
        );
    }
}
