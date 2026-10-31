#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use std::collections::BTreeSet;

use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent, SpeakQueueHandle};

use common::{recording_sink, request, settle_on_paused_clock, standard_deps, wait_for};
use forge_speech_types::EngineId;
use forge_speech_types::SynthesisDefaults;

fn spawn_standard() -> (SpeakQueueHandle, forge_speak_queue::SpeakEventStream) {
    let (sink, _plays) = recording_sink();
    forge_speak_queue::spawn(QueueConfig::default(), standard_deps(sink))
}

#[tokio::test]
async fn queue_depth_rises_with_pending_and_falls_to_zero_on_drain() {
    let (handle, mut stream) = spawn_standard();

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;
    assert_eq!(handle.queue_depth(), 0, "empty queue reports zero depth");

    for i in 0..3 {
        handle
            .send(SpeakCommand::Enqueue(request(&format!("v{i}"), "msg")))
            .await
            .unwrap();
    }
    quiesce_after(&handle, &mut stream).await;
    assert_eq!(handle.queue_depth(), 3, "every held item counts as pending");

    handle.send(SpeakCommand::Resume).await.unwrap();
    for _ in 0..3 {
        wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;
    }
    quiesce_after(&handle, &mut stream).await;
    assert_eq!(
        handle.queue_depth(),
        0,
        "a drained queue reports zero depth"
    );
}

#[tokio::test(start_paused = true)]
async fn available_voices_reflects_actor_built_catalog() {
    let (handle, _stream) = spawn_standard();
    settle_on_paused_clock().await;

    let ids: BTreeSet<String> = handle
        .available_voices()
        .iter()
        .map(|v| v.id.0.clone())
        .collect();
    let expected: BTreeSet<String> = ["alpha-1", "alpha-2", "beta-1"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(ids, expected);
}

#[tokio::test(start_paused = true)]
async fn engines_returns_each_engine_once_despite_multiple_voices() {
    let (handle, _stream) = spawn_standard();
    settle_on_paused_clock().await;

    let mut engines: Vec<String> = handle.engines().into_iter().map(|e| e.0).collect();
    engines.sort();
    assert_eq!(engines, vec!["alpha".to_string(), "beta".to_string()]);
}

async fn quiesce_after(
    handle: &SpeakQueueHandle,
    stream: &mut forge_speak_queue::SpeakEventStream,
) {
    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(stream, |e| matches!(e, SpeakEvent::Paused)).await;
}

#[tokio::test]
async fn disabled_engines_reader_observes_actor_side_toggle() {
    let (handle, mut stream) = spawn_standard();
    let alpha = EngineId("alpha".into());

    handle
        .send(SpeakCommand::SetEngineEnabled(alpha.clone(), false))
        .await
        .unwrap();
    quiesce_after(&handle, &mut stream).await;

    assert!(handle.disabled_engines().contains(&alpha));
}

#[tokio::test]
async fn engine_gain_reader_observes_actor_side_set_engine_params() {
    let (handle, mut stream) = spawn_standard();
    let beta = EngineId("beta".into());

    handle
        .send(SpeakCommand::SetEngineParams(
            beta.clone(),
            SynthesisDefaults::default(),
            0.5,
        ))
        .await
        .unwrap();
    quiesce_after(&handle, &mut stream).await;

    assert_eq!(handle.engine_gain(&beta), 0.5);
}
