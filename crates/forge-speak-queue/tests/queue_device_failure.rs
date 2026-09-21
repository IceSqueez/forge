#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, PcmBuffer};
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_voice::AssignmentStrategy;

use common::{make_deps, request, standard_registry};

const DEVICE_REFUSAL: &str = "device 'hw:9,9' not found";
const SETTLE_BUDGET: Duration = Duration::from_secs(5);

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

    let deadline = std::time::Instant::now() + SETTLE_BUDGET;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "the utterance never settled after the device refused the stream"
        );
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Failed { error, .. })) => {
                assert!(
                    error.contains(DEVICE_REFUSAL),
                    "the device reason was replaced by {error:?}"
                );
                return;
            }
            Ok(Ok(SpeakEvent::Finished { .. })) => {
                panic!("a device that never opened must not finish the utterance")
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("the speak event stream closed"),
            Err(_) => panic!("the utterance never settled after the device refused the stream"),
        }
    }
}
