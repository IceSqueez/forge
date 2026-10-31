mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback};
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream};
use forge_speech_types::AssignmentStrategy;
use forge_speech_types::PcmBuffer;

use common::{make_deps, next_matching, request, standard_registry};

const DEVICE_REFUSAL: &str = "device 'hw:9,9' not found";

struct RefusingSink;

#[async_trait]
impl AudioSink for RefusingSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Err(AudioError::Host(DEVICE_REFUSAL.to_owned()))
    }
}

#[tokio::test]
async fn a_device_that_refuses_the_stream_fails_the_utterance_with_the_device_reason() {
    let deps = make_deps(
        standard_registry(),
        Arc::new(RefusingSink),
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::Enqueue(request(
            "viewer",
            "the device is gone",
        )))
        .await
        .unwrap();

    let outcome = next_matching(&mut stream, |event| {
        matches!(
            event,
            SpeakEvent::Failed { .. } | SpeakEvent::Finished { .. }
        )
    })
    .await;
    let SpeakEvent::Failed { message, .. } = outcome else {
        panic!("a device that never opened must not finish the utterance, got {outcome:?}");
    };
    assert_eq!(
        (
            message.code().to_string(),
            message.detail().map(|d| d.as_str().to_owned()),
        ),
        (
            "speak.audio_host_failed".to_owned(),
            Some(DEVICE_REFUSAL.to_owned()),
        ),
        "the device reason was replaced"
    );
}

const HUNG_START_WAIT: Duration = Duration::from_secs(3_600);

async fn wait_past_the_start_timeout<F>(stream: &mut SpeakEventStream, settled: F)
where
    F: Fn(&SpeakEvent) -> bool,
{
    let settled_in_time = tokio::time::timeout(HUNG_START_WAIT, async {
        while let Ok(event) = stream.recv().await {
            if settled(&event) {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(settled_in_time, Ok(true), "the hung start never settled");
}

struct MarkOnDrop(Arc<AtomicBool>);

impl Drop for MarkOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct HangsOnFirstStart {
    hang_next: AtomicBool,
    start_abandoned: Arc<AtomicBool>,
}

impl HangsOnFirstStart {
    fn new() -> Self {
        Self {
            hang_next: AtomicBool::new(true),
            start_abandoned: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl AudioSink for HangsOnFirstStart {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Ok(())
    }

    async fn play_controlled(&self, _buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        if self.hang_next.swap(false, Ordering::SeqCst) {
            let _abandoned = MarkOnDrop(Arc::clone(&self.start_abandoned));
            std::future::pending::<()>().await;
        }
        Ok(ControlledPlayback::resolved(Ok(())))
    }
}

#[tokio::test(start_paused = true)]
async fn a_start_that_never_returns_fails_the_utterance_and_the_next_one_still_plays() {
    let sink = Arc::new(HangsOnFirstStart::new());
    let abandoned = Arc::clone(&sink.start_abandoned);
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let hung = request("viewer", "the device hangs on this one");
    let hung_id = hung.request_id.clone();
    let next = request("viewer", "this one plays after the hang");
    let next_id = next.request_id.clone();

    handle.send(SpeakCommand::Enqueue(hung)).await.unwrap();
    wait_past_the_start_timeout(&mut stream, |event| {
        matches!(
            event,
            SpeakEvent::Failed { request_id, message, .. }
                if *request_id == hung_id
                    && message.code().to_string() == "speak.audio_start_timed_out"
        )
    })
    .await;
    handle.send(SpeakCommand::Enqueue(next)).await.unwrap();
    wait_past_the_start_timeout(
        &mut stream,
        |event| matches!(event, SpeakEvent::Finished { request_id, .. } if *request_id == next_id),
    )
    .await;

    assert!(
        abandoned.load(Ordering::SeqCst),
        "the timed-out start must be dropped so the sink can stop it"
    );
}
