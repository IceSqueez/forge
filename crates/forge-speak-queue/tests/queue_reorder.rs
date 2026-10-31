#![allow(clippy::panic)]
mod common;

use forge_speak_queue::{
    Priority, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakRequest,
};

use common::{first_started_ids, make_deps, recording_sink, request, standard_registry, wait_for};
use forge_speech_types::AssignmentStrategy;

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

    let played = first_started_ids(&mut stream, 3).await;
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

    wait_for(&mut stream, |e| {
        order_is(e, &[(&ids[1], false), (&ids[2], false), (&ids[0], false)])
    })
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

    wait_for(&mut stream, |e| {
        order_is(
            e,
            &[
                (&second_high.request_id, true),
                (&first_high.request_id, true),
                (&normal.request_id, false),
            ],
        )
    })
    .await;
}
