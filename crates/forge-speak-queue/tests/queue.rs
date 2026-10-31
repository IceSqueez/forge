#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_types::EventId;

use common::{recording_bus, recording_sink, request, standard_deps, wait_for};

#[tokio::test]
async fn enqueue_and_finish() {
    let (sink, play_count) = recording_sink();
    let (handle, mut stream) =
        forge_speak_queue::spawn(QueueConfig::default(), standard_deps(sink));

    let req = request("viewer1", "hello world");
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;

    assert_eq!(*play_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn pause_holds_synthesis() {
    let (sink, play_count) = recording_sink();
    let (handle, mut stream) =
        forge_speak_queue::spawn(QueueConfig::default(), standard_deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    let req = request("viewer1", "paused message");
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    common::enqueue_proving_nothing_started(&handle, &mut stream).await;
    assert_eq!(*play_count.lock().unwrap(), 0);

    handle.send(SpeakCommand::Resume).await.unwrap();
    for _ in 0..2 {
        wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;
    }
    assert_eq!(*play_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn clear_empties_queue() {
    let (sink, play_count) = recording_sink();
    let config = QueueConfig {
        max_queue_len: 50,
        per_user_limit: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, standard_deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    for i in 0..5 {
        let req = request("viewer1", &format!("message {i}"));
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    handle.send(SpeakCommand::Clear).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Cleared)).await;

    handle.send(SpeakCommand::Resume).await.unwrap();
    let survivor = request("viewer2", "after the clear");
    handle.send(SpeakCommand::Enqueue(survivor)).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;

    assert_eq!(*play_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn per_user_limit_rejects_excess() {
    let (sink, _plays) = recording_sink();
    let config = QueueConfig {
        per_user_limit: 2,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, standard_deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    for _ in 0..3 {
        let req = request("same_user", "test");
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Rejected { .. })).await;
}

#[tokio::test]
async fn queue_full_rejects_when_at_capacity() {
    let (sink, _plays) = recording_sink();
    let config = QueueConfig {
        max_queue_len: 2,
        per_user_limit: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, standard_deps(sink));

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    for i in 0..3 {
        let req = request(&format!("user{i}"), "test");
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Rejected { .. })).await;
}

#[tokio::test]
async fn started_bus_event_emitted_at_dequeue_and_after_synth_with_superset_schema() {
    let (sink, _plays) = recording_sink();
    let (deps, bus) = recording_bus(standard_deps(sink));
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    let src = EventId::new();
    let mut req = request("nova", "hello chat");
    req.source_event_id = Some(src);
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();

    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Finished { .. })).await;

    let started = bus.of_kind("speak.started");
    assert_eq!(
        started.len(),
        2,
        "speak.started fires once at dequeue and once after synth resolves"
    );

    assert!(
        started[0].payload["voice_id"].is_null(),
        "dequeue emission has no resolved voice yet"
    );
    assert!(started[0].payload["engine_id"].is_null());
    assert!(
        started[1].payload["voice_id"].is_string(),
        "post-synth emission carries the resolved voice"
    );
    assert!(started[1].payload["engine_id"].is_string());

    for ev in &started {
        assert_eq!(ev.payload["viewer_name"].as_str(), Some("nova"));
        assert_eq!(ev.payload["text"].as_str(), Some("hello chat"));
        assert!(
            ev.payload.get("queue_len").is_some_and(|v| v.is_number()),
            "both emissions share the superset schema key queue_len"
        );
        for key in ["detected_language", "language_confidence"] {
            assert!(
                ev.payload.get(key).is_some_and(|v| v.is_null()),
                "{key} is part of the superset schema and null while detection is off"
            );
        }
        assert_eq!(ev.caused_by, Some(src));
    }
}
