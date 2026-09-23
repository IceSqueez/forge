#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback, PcmBuffer};
use forge_events::{Event, EventPublisher};
use forge_speak_queue::{QueueConfig, RequestId, SpeakCommand, SpeakEvent};
use forge_voice::AssignmentStrategy;

use common::{assert_no_event, make_deps, request, standard_registry};

const DEVICE_LOSS: &str = "output stream disconnected";
const EXPECTED_FAILURE: &str = "cpal host error: output stream disconnected";
const DEVICE_ERROR_REASON: &str = "device_error";
const SPEAK_FAILED_KIND: &str = "speak.failed";
const SETTLE_BUDGET: Duration = Duration::from_secs(5);
const STAYS_QUIET_MS: u64 = 50;

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
                let _ = gate.await;
                Err(AudioError::Host(DEVICE_LOSS.to_owned()))
            })),
        }
    }
}

#[derive(Default)]
struct RecordingBus(Mutex<Vec<Event>>);

impl RecordingBus {
    fn failures(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == SPEAK_FAILED_KIND)
            .map(|e| {
                (
                    e.payload["reason"].as_str().unwrap_or_default().to_owned(),
                    e.payload["error"].as_str().unwrap_or_default().to_owned(),
                )
            })
            .collect()
    }
}

impl EventPublisher for RecordingBus {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

async fn failure_for(
    stream: &mut forge_speak_queue::SpeakEventStream,
    wanted: &RequestId,
) -> String {
    let deadline = std::time::Instant::now() + SETTLE_BUDGET;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!remaining.is_zero(), "the utterance never settled");
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Failed { request_id, error })) if &request_id == wanted => {
                return error;
            }
            Ok(Ok(SpeakEvent::Finished { request_id })) if &request_id == wanted => {
                panic!("a device that died mid-clip must not finish the utterance")
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("the speak event stream closed"),
            Err(_) => panic!("the utterance never settled"),
        }
    }
}

#[tokio::test]
async fn a_device_lost_mid_utterance_fails_it_with_the_device_reason_on_both_feeds() {
    let bus = Arc::new(RecordingBus::default());
    let mut deps = make_deps(
        standard_registry(),
        Arc::new(DyingSink::immediate()),
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    deps.event_bus = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let utterance = request("viewer", "the headset went away");
    let utterance_id = utterance.request_id.clone();

    handle.send(SpeakCommand::Enqueue(utterance)).await.unwrap();

    assert_eq!(
        failure_for(&mut stream, &utterance_id).await,
        EXPECTED_FAILURE
    );
    assert_eq!(
        bus.failures(),
        vec![(DEVICE_ERROR_REASON.to_owned(), EXPECTED_FAILURE.to_owned())],
        "the dashboard reads the drop off the bus, so it must name the device as the cause"
    );
}

#[tokio::test]
async fn a_device_that_dies_while_the_queue_believes_it_is_playing_releases_the_next_utterance() {
    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel();
    let deps = make_deps(
        standard_registry(),
        Arc::new(DyingSink::gated(gate_rx)),
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    let first = request("viewer", "still playing");
    let first_id = first.request_id.clone();
    let second = request("viewer", "waiting its turn");
    let second_id = second.request_id.clone();

    handle.send(SpeakCommand::Enqueue(first)).await.unwrap();
    handle.send(SpeakCommand::Enqueue(second)).await.unwrap();

    let waiting = second_id.clone();
    assert_no_event(
        &mut stream,
        move |event| matches!(event, SpeakEvent::Started { request_id, .. } if request_id == &waiting),
        STAYS_QUIET_MS,
    )
    .await;

    let _ = gate_tx.send(());

    assert_eq!(failure_for(&mut stream, &first_id).await, EXPECTED_FAILURE);
    let started_next = tokio::time::timeout(SETTLE_BUDGET, async {
        loop {
            match stream.recv().await {
                Ok(SpeakEvent::Started { request_id, .. }) if request_id == second_id => return,
                Ok(_) => continue,
                Err(_) => panic!("the speak event stream closed"),
            }
        }
    })
    .await;

    assert!(
        started_next.is_ok(),
        "the queue stayed wedged on the utterance the dead device dropped"
    );
}
