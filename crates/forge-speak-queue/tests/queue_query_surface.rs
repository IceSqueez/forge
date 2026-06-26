//! Query surface on `SpeakQueueHandle`: `queue_depth()` (atomic pending mirror),
//! `available_voices()` and `engines()` (actor-populated catalog).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent, SpeakQueueHandle};

use common::{make_deps, recording_sink, request, standard_registry, wait_for};
use forge_voice::AssignmentStrategy;

async fn wait_until_depth(handle: &SpeakQueueHandle, expected: usize, max_ms: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    while handle.queue_depth() != expected {
        if std::time::Instant::now() >= deadline {
            panic!(
                "queue_depth never reached {expected}; last saw {}",
                handle.queue_depth()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

async fn wait_until_catalog_populated(handle: &SpeakQueueHandle, max_ms: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    while handle.available_voices().is_empty() {
        if std::time::Instant::now() >= deadline {
            panic!("catalog never populated");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

fn spawn_standard() -> (SpeakQueueHandle, forge_speak_queue::SpeakEventStream) {
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

#[tokio::test]
async fn queue_depth_rises_with_pending_and_falls_to_zero_on_drain() {
    let (handle, mut stream) = spawn_standard();

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Paused { .. }),
        1_000,
    )
    .await;
    assert_eq!(handle.queue_depth(), 0, "empty queue reports zero depth");

    for i in 0..3 {
        handle
            .send(SpeakCommand::Enqueue(request(&format!("v{i}"), "msg")))
            .await
            .unwrap();
    }
    wait_until_depth(&handle, 3, 1_000).await;

    handle.send(SpeakCommand::Resume).await.unwrap();
    wait_until_depth(&handle, 0, 2_000).await;
}

#[tokio::test]
async fn available_voices_reflects_actor_built_catalog() {
    let (handle, _stream) = spawn_standard();
    wait_until_catalog_populated(&handle, 2_000).await;

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

#[tokio::test]
async fn engines_returns_each_engine_once_despite_multiple_voices() {
    let (handle, _stream) = spawn_standard();
    wait_until_catalog_populated(&handle, 2_000).await;

    // `alpha` owns two voices but must appear exactly once — `engines()` de-duplicates.
    let mut engines: Vec<String> = handle.engines().into_iter().map(|e| e.0).collect();
    engines.sort();
    assert_eq!(engines, vec!["alpha".to_string(), "beta".to_string()]);
}
