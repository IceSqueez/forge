#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use forge_speak_queue::{QueueConfig, QueueDeps, SpeakCommand, SpeakEvent, SpeakEventStream};

use common::{next_matching, recording_sink, request, standard_deps, wait_for};

fn silent_deps() -> QueueDeps {
    let (sink, _plays) = recording_sink();
    standard_deps(sink)
}

async fn admissions(stream: &mut SpeakEventStream, count: usize) -> Vec<SpeakEvent> {
    let mut admitted = Vec::with_capacity(count);
    for _ in 0..count {
        admitted.push(
            next_matching(stream, |event| {
                matches!(
                    event,
                    SpeakEvent::Enqueued { .. } | SpeakEvent::Rejected { .. }
                )
            })
            .await,
        );
    }
    admitted
}

#[tokio::test]
async fn sixth_request_rejected_when_limit_is_five() {
    let config = QueueConfig {
        per_user_limit: 5,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, silent_deps());

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    let viewer = "heavy-user";
    for i in 0..5 {
        handle
            .send(SpeakCommand::Enqueue(request(viewer, &format!("msg {i}"))))
            .await
            .unwrap();
    }

    handle
        .send(SpeakCommand::Enqueue(request(viewer, "over the limit")))
        .await
        .unwrap();

    let events = admissions(&mut stream, 6).await;
    let rejected_count = events
        .iter()
        .filter(|e| matches!(e, SpeakEvent::Rejected { .. }))
        .count();

    assert_eq!(rejected_count, 1, "exactly one Rejected event expected");
}

#[tokio::test]
async fn different_viewers_are_tracked_independently() {
    let config = QueueConfig {
        per_user_limit: 2,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, silent_deps());
    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    for viewer in ["viewer-a", "viewer-b"] {
        for i in 0..2 {
            handle
                .send(SpeakCommand::Enqueue(request(viewer, &format!("msg {i}"))))
                .await
                .unwrap();
        }
    }

    let events = admissions(&mut stream, 4).await;
    let rejected_count = events
        .iter()
        .filter(|e| matches!(e, SpeakEvent::Rejected { .. }))
        .count();

    assert_eq!(
        rejected_count, 0,
        "no rejections expected when each viewer is within their own limit"
    );
}
