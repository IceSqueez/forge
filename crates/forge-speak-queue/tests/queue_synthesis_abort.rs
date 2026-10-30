#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use std::sync::Arc;

use async_trait::async_trait;
use forge_speak_queue::{QueueConfig, RequestId, SpeakCommand, SpeakEvent};
use forge_speech_types::{AssignmentStrategy, EngineId, PcmBuffer, TtsVoice};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use tokio::sync::Notify;

use common::{HANG_GUARD_MS, make_deps, recording_sink, request, voice, wait_for};

struct DropSignal(Arc<Notify>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

struct StuckEngine {
    entered: Arc<Notify>,
    dropped: Arc<Notify>,
}

#[async_trait]
impl TtsEngine for StuckEngine {
    fn engine_id(&self) -> &EngineId {
        static ID: std::sync::OnceLock<EngineId> = std::sync::OnceLock::new();
        ID.get_or_init(|| EngineId("stuck".into()))
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
        Ok(vec![voice("s-voice", "stuck")])
    }
    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let _guard = DropSignal(Arc::clone(&self.dropped));
        self.entered.notify_one();
        std::future::pending::<()>().await;
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct StuckFactory {
    entered: Arc<Notify>,
    dropped: Arc<Notify>,
}

impl TtsEngineFactory for StuckFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(StuckEngine {
            entered: Arc::clone(&self.entered),
            dropped: Arc::clone(&self.dropped),
        }))
    }
}

struct Harness {
    handle: forge_speak_queue::SpeakQueueHandle,
    stream: forge_speak_queue::SpeakEventStream,
    request_id: RequestId,
    dropped: Arc<Notify>,
}

async fn with_synthesis_in_flight() -> Harness {
    let entered = Arc::new(Notify::new());
    let dropped = Arc::new(Notify::new());
    let mut registry = TtsRegistry::new();
    registry.register(
        EngineId("stuck".into()),
        Arc::new(StuckFactory {
            entered: Arc::clone(&entered),
            dropped: Arc::clone(&dropped),
        }),
    );
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let req = request("viewer", "never finishes synthesizing");
    let request_id = req.request_id.clone();
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    wait_for_entered(&entered).await;
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Enqueued { .. })).await;
    Harness {
        handle,
        stream,
        request_id,
        dropped,
    }
}

async fn wait_for_entered(entered: &Notify) {
    tokio::time::timeout(
        std::time::Duration::from_millis(HANG_GUARD_MS),
        entered.notified(),
    )
    .await
    .expect("the engine never started synthesizing");
}

async fn assert_synthesis_future_dropped(dropped: &Notify) {
    tokio::time::timeout(
        std::time::Duration::from_millis(HANG_GUARD_MS),
        dropped.notified(),
    )
    .await
    .expect("the in-flight synthesis kept running after the item was abandoned");
}

#[tokio::test]
async fn skipping_aborts_the_in_flight_synthesis() {
    let mut h = with_synthesis_in_flight().await;

    h.handle.send(SpeakCommand::Skip).await.unwrap();

    assert_synthesis_future_dropped(&h.dropped).await;
    wait_for(&mut h.stream, |e| matches!(e, SpeakEvent::Skipped { .. })).await;
}

#[tokio::test]
async fn clearing_aborts_the_in_flight_synthesis() {
    let h = with_synthesis_in_flight().await;

    h.handle.send(SpeakCommand::Clear).await.unwrap();

    assert_synthesis_future_dropped(&h.dropped).await;
}

#[tokio::test]
async fn cancelling_the_active_request_aborts_its_synthesis() {
    let h = with_synthesis_in_flight().await;

    h.handle
        .send(SpeakCommand::Cancel(h.request_id.clone()))
        .await
        .unwrap();

    assert_synthesis_future_dropped(&h.dropped).await;
}
