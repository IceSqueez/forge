#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use forge_speak_queue::{
    QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakRequest,
};

use common::{make_deps, recording_sink, request, standard_registry};
use forge_voice::AssignmentStrategy;

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

/// Dedupes: the actor emits `Started` twice per request - once at dispatch, once once
/// synthesis resolves a voice.
async fn first_started_ids(
    stream: &mut SpeakEventStream,
    count: usize,
    max_ms: u64,
) -> Vec<RequestId> {
    let mut seen: Vec<RequestId> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
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

    // Commands share one FIFO channel, so staging needs no synchronisation - only the
    // pause, which keeps all three items pending instead of dispatching the first.
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
