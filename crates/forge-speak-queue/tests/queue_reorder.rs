#![allow(clippy::panic)]
mod common;

use forge_speak_queue::{
    Priority, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakRequest,
};

use common::{make_deps, recording_sink, request, standard_registry, wait_for};
use forge_speech_types::AssignmentStrategy;

const HANG_GUARD_MS: u64 = 30_000;

fn spawn_standard() -> (forge_speak_queue::SpeakQueueHandle, SpeakEventStream) {
    let (sink, _plays) = recording_sink();
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

async fn first_started_ids(
    stream: &mut SpeakEventStream,
    count: usize,
    max_ms: u64,
) -> Vec<RequestId> {
    let mut seen: Vec<RequestId> = Vec::new();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_millis(max_ms.max(HANG_GUARD_MS));
    while seen.len() < count {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("only saw {} of {count} Started request ids", seen.len());
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Started { request_id, .. })) => {
                if !seen.contains(&request_id) {
                    seen.push(request_id);
                }
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for Started events"),
        }
    }
    seen
}

#[tokio::test]
async fn reordering_ahead_of_an_earlier_item_changes_playback_order() {
    let (handle, mut stream) = spawn_standard();

    handle.send(SpeakCommand::Pause).await.unwrap();
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let req: SpeakRequest = request(name, name);
        ids.push(req.request_id.clone());
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    handle
        .send(SpeakCommand::Reorder {
            request_id: ids[2].clone(),
            before: Some(ids[0].clone()),
        })
        .await
        .unwrap();
    handle.send(SpeakCommand::Resume).await.unwrap();

    let played = first_started_ids(&mut stream, 3, 5_000).await;
    assert_eq!(
        played,
        vec![ids[2].clone(), ids[0].clone(), ids[1].clone()],
        "the reordered item must play ahead of its anchor, not after it",
    );
}

fn high_priority(viewer: &str, text: &str) -> SpeakRequest {
    SpeakRequest {
        priority: Priority::High,
        ..request(viewer, text)
    }
}

fn order_is(event: &SpeakEvent, expected: &[(&RequestId, bool)]) -> bool {
    let SpeakEvent::QueueChanged { order, .. } = event else {
        return false;
    };
    order.len() == expected.len()
        && order
            .iter()
            .zip(expected)
            .all(|(entry, (id, high))| &entry.request_id == *id && entry.is_high_priority == *high)
}

#[tokio::test]
async fn reordering_without_an_anchor_moves_a_normal_item_to_the_back() {
    let (handle, mut stream) = spawn_standard();
    handle.send(SpeakCommand::Pause).await.unwrap();
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let req = request(name, name);
        ids.push(req.request_id.clone());
        handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    }

    handle
        .send(SpeakCommand::Reorder {
            request_id: ids[0].clone(),
            before: None,
        })
        .await
        .unwrap();

    wait_for(
        &mut stream,
        |e| order_is(e, &[(&ids[1], false), (&ids[2], false), (&ids[0], false)]),
        5_000,
    )
    .await;
}

#[tokio::test]
async fn reordering_a_high_priority_item_without_an_anchor_keeps_it_in_the_high_queue() {
    let (handle, mut stream) = spawn_standard();
    handle.send(SpeakCommand::Pause).await.unwrap();
    let normal = request("n", "normal");
    let first_high = high_priority("h1", "first high");
    let second_high = high_priority("h2", "second high");
    for req in [&normal, &first_high, &second_high] {
        handle
            .send(SpeakCommand::Enqueue(req.clone()))
            .await
            .unwrap();
    }

    handle
        .send(SpeakCommand::Reorder {
            request_id: first_high.request_id.clone(),
            before: None,
        })
        .await
        .unwrap();

    wait_for(
        &mut stream,
        |e| {
            order_is(
                e,
                &[
                    (&second_high.request_id, true),
                    (&first_high.request_id, true),
                    (&normal.request_id, false),
                ],
            )
        },
        5_000,
    )
    .await;
}
