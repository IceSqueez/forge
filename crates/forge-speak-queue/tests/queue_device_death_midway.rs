#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback};
use forge_speak_queue::{QueueConfig, RequestId, SpeakCommand, SpeakEvent};
use forge_speech_types::PcmBuffer;

use common::{
    RecordingPublisher, assert_no_event_once_settled_on_paused_clock, outcome_of, recording_bus,
    request, standard_deps, wait_for,
};

const DEVICE_LOSS: &str = "output stream disconnected";
const EXPECTED_CODE: &str = "speak.audio_host_failed";
const DEVICE_ERROR_REASON: &str = "device_error";
const SPEAK_FAILED_KIND: &str = "speak.failed";

struct DyingSink {
    gate: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

impl DyingSink {
    fn immediate() -> Self {
        Self {
            gate: Mutex::new(None),
        }
    }

    fn gated(gate: tokio::sync::oneshot::Receiver<()>) -> Self {
        Self {
            gate: Mutex::new(Some(gate)),
        }
    }
}

#[async_trait]
impl AudioSink for DyingSink {
    async fn play(&self, _buffer: PcmBuffer) -> Result<(), AudioError> {
        Err(AudioError::Host(DEVICE_LOSS.to_owned()))
    }

    async fn play_controlled(&self, _buffer: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        match self.gate.lock().unwrap().take() {
            None => Ok(ControlledPlayback::resolved(Err(AudioError::Host(
                DEVICE_LOSS.to_owned(),
            )))),
            Some(gate) => Ok(ControlledPlayback::from_future(async move {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "only completion matters here"
                )]
                let _ = gate.await;
                Err(AudioError::Host(DEVICE_LOSS.to_owned()))
            })),
        }
    }
}

fn failures_on(bus: &RecordingPublisher) -> Vec<(String, String, String)> {
    bus.of_kind(SPEAK_FAILED_KIND)
        .iter()
        .map(|e| {
            (
                e.payload["reason"].as_str().unwrap_or_default().to_owned(),
                e.payload["message"]["code"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                e.payload["message"]["detail"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

async fn failure_for(
    stream: &mut forge_speak_queue::SpeakEventStream,
    wanted: &RequestId,
) -> (String, Option<String>) {
    match outcome_of(stream, wanted).await {
        SpeakEvent::Failed { message, .. } => (
            message.code().to_string(),
            message.detail().map(|d| d.as_str().to_owned()),
        ),
        other => panic!("a device that died mid-clip must fail the utterance, got {other:?}"),
    }
}

#[tokio::test]
async fn a_device_lost_mid_utterance_fails_it_with_the_device_reason_on_both_feeds() {
    let (deps, bus) = recording_bus(standard_deps(Arc::new(DyingSink::immediate())));
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let utterance = request("viewer", "the headset went away");
    let utterance_id = utterance.request_id.clone();

    handle.send(SpeakCommand::Enqueue(utterance)).await.unwrap();

    assert_eq!(
        failure_for(&mut stream, &utterance_id).await,
        (EXPECTED_CODE.to_owned(), Some(DEVICE_LOSS.to_owned()))
    );
    assert_eq!(
        failures_on(&bus),
        vec![(
            DEVICE_ERROR_REASON.to_owned(),
            EXPECTED_CODE.to_owned(),
            DEVICE_LOSS.to_owned()
        )],
        "the dashboard reads the drop off the bus, so it must name the device as the cause"
    );
}

#[tokio::test(start_paused = true)]
async fn a_device_that_dies_while_the_queue_believes_it_is_playing_releases_the_next_utterance() {
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
    let deps = standard_deps(Arc::new(DyingSink::gated(gate_rx)));
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let first = request("viewer", "still playing");
    let first_id = first.request_id.clone();
    let second = request("viewer", "waiting its turn");
    let second_id = second.request_id.clone();

    handle.send(SpeakCommand::Enqueue(first)).await.unwrap();
    handle.send(SpeakCommand::Enqueue(second)).await.unwrap();

    let waiting = second_id.clone();
    assert_no_event_once_settled_on_paused_clock(
        &mut stream,
        move |event| matches!(event, SpeakEvent::Started { request_id, .. } if request_id == &waiting),
    )
    .await;

    #[expect(
        clippy::let_underscore_must_use,
        reason = "the receiver may already be gone"
    )]
    let _ = gate_tx.send(());

    assert_eq!(
        failure_for(&mut stream, &first_id).await,
        (EXPECTED_CODE.to_owned(), Some(DEVICE_LOSS.to_owned()))
    );
    wait_for(
        &mut stream,
        |event| matches!(event, SpeakEvent::Started { request_id, .. } if *request_id == second_id),
    )
    .await;
}
