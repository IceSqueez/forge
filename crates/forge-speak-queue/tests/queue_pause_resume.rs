#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use std::sync::Arc;

use forge_audio::AudioSink;
use forge_speak_queue::{QueueConfig, QueueDeps, SpeakCommand, SpeakEvent, SpeakRequest};
use forge_speech_types::AssignmentStrategy;

use common::{
    enqueue_proving_nothing_started, make_deps, recording_sink, request, standard_registry,
    wait_for,
};

fn deps(sink: Arc<dyn AudioSink>) -> QueueDeps {
    make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    )
}

fn speak_req(text: &str) -> SpeakRequest {
    request("viewer", text)
}

async fn wait_for_two_finished(stream: &mut forge_speak_queue::SpeakEventStream) {
    for _ in 0..2 {
        wait_for(stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;
    }
}

#[tokio::test]
async fn pause_prevents_synthesis_until_resume() {
    let (sink, play_count) = recording_sink();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    handle
        .send(SpeakCommand::Enqueue(speak_req("paused message")))
        .await
        .unwrap();

    enqueue_proving_nothing_started(&handle, &mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 0);

    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for_two_finished(&mut stream).await;

    assert_eq!(*play_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn manual_resume_does_not_release_a_voice_gate_hold() {
    let (sink, play_count) = recording_sink();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, deps(sink));

    handle.send(SpeakCommand::VoiceGateActivated).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::VoiceGateHeld)).await;

    handle
        .send(SpeakCommand::Enqueue(speak_req("mic is hot")))
        .await
        .unwrap();
    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Resumed)).await;

    enqueue_proving_nothing_started(&handle, &mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 0);

    handle
        .send(SpeakCommand::VoiceGateDeactivated)
        .await
        .unwrap();
    wait_for_two_finished(&mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn voice_gate_release_does_not_clear_a_manual_pause() {
    let (sink, play_count) = recording_sink();
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;
    handle.send(SpeakCommand::VoiceGateActivated).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::VoiceGateHeld)).await;

    handle
        .send(SpeakCommand::Enqueue(speak_req("still paused")))
        .await
        .unwrap();
    handle
        .send(SpeakCommand::VoiceGateDeactivated)
        .await
        .unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::VoiceGateReleased)).await;

    enqueue_proving_nothing_started(&handle, &mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 0);

    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_for_two_finished(&mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 2);
}
