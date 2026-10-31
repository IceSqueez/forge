#![allow(clippy::panic)]
mod common;

use forge_speak_queue::{Priority, QueueConfig, RequestId, SpeakCommand, SpeakEvent};

use common::{first_started_ids, recording_sink, request_with_priority, standard_deps, wait_for};

#[tokio::test]
async fn every_high_priority_item_plays_before_every_normal_item_in_fifo_order_per_tier() {
    let (sink, _plays) = recording_sink();
    let deps = standard_deps(sink);
    let config = QueueConfig {
        per_user_limit: 10,
        max_queue_len: 50,
        ..QueueConfig::default()
    };
    let (handle, mut stream) = forge_speak_queue::spawn(config, deps);

    handle.send(SpeakCommand::Pause).await.unwrap();
    wait_for(&mut stream, |e| matches!(e, SpeakEvent::Paused)).await;

    let enqueue = async |viewer: &str, priority| -> RequestId {
        let item = request_with_priority(viewer, "message", priority);
        let id = item.request_id.clone();
        handle.send(SpeakCommand::Enqueue(item)).await.unwrap();
        id
    };
    let n0 = enqueue("n0", Priority::Normal).await;
    let h0 = enqueue("h0", Priority::High).await;
    let n1 = enqueue("n1", Priority::Normal).await;
    let h1 = enqueue("h1", Priority::High).await;
    let n2 = enqueue("n2", Priority::Normal).await;

    handle.send(SpeakCommand::Resume).await.unwrap();

    assert_eq!(
        first_started_ids(&mut stream, 5).await,
        vec![h0, h1, n0, n1, n2]
    );
}
