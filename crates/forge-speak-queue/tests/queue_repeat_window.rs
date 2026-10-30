#![allow(clippy::panic, clippy::unwrap_used)]
mod common;

use std::num::NonZeroUsize;

use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream, SpeakQueueHandle,
};
use forge_speech_types::AssignmentStrategy;

use common::{make_deps, neutral_pipeline, recording_sink, request, standard_registry};

const HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);

fn window(size: usize) -> NonZeroUsize {
    NonZeroUsize::new(size).unwrap()
}

fn spawn_skipping_repeats(repeat_window: NonZeroUsize) -> (SpeakQueueHandle, SpeakEventStream) {
    let (sink, _plays) = recording_sink();
    let mut deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let mut pipeline = neutral_pipeline();
    pipeline.skip_rules.repeat_of_recent = true;
    deps.pipeline = PipelineConfigHandle::new(pipeline);
    forge_speak_queue::spawn(
        QueueConfig {
            repeat_window,
            ..QueueConfig::default()
        },
        deps,
    )
}

async fn is_spoken(handle: &SpeakQueueHandle, stream: &mut SpeakEventStream, text: &str) -> bool {
    let req = request("viewer", text);
    let id = req.request_id.clone();
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    loop {
        match tokio::time::timeout(HANG_GUARD, stream.recv()).await {
            Ok(Ok(SpeakEvent::Finished { request_id })) if request_id == id => return true,
            Ok(Ok(SpeakEvent::Skipped { request_id, .. })) if request_id == id => return false,
            Ok(Ok(SpeakEvent::Failed {
                request_id,
                message,
            })) if request_id == id => {
                panic!("{text:?} failed: {message:?}")
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("event stream closed"),
            Err(_) => panic!("no outcome for {text:?}"),
        }
    }
}

async fn third_message_spoken(handle: &SpeakQueueHandle, stream: &mut SpeakEventStream) -> bool {
    assert!(is_spoken(handle, stream, "first").await);
    assert!(is_spoken(handle, stream, "second").await);
    is_spoken(handle, stream, "first").await
}

#[tokio::test]
async fn a_repeat_is_skipped_only_while_the_earlier_message_is_inside_the_window() {
    for (size, expected_spoken) in [(1, true), (2, false), (3, false)] {
        let (handle, mut stream) = spawn_skipping_repeats(window(size));

        let spoken = third_message_spoken(&handle, &mut stream).await;

        assert_eq!(spoken, expected_spoken, "window {size}");
    }
}

#[tokio::test]
async fn set_repeat_window_changes_the_window_used_for_later_messages() {
    for (initial, updated, expected_spoken) in [(3, 1, true), (1, 2, false)] {
        let (handle, mut stream) = spawn_skipping_repeats(window(initial));
        handle
            .send(SpeakCommand::SetRepeatWindow(window(updated)))
            .await
            .unwrap();

        let spoken = third_message_spoken(&handle, &mut stream).await;

        assert_eq!(spoken, expected_spoken, "window {initial} -> {updated}");
    }
}

#[tokio::test]
async fn shrinking_the_repeat_window_mid_stream_applies_to_the_very_next_message() {
    let (handle, mut stream) = spawn_skipping_repeats(window(3));
    assert!(is_spoken(&handle, &mut stream, "first").await);
    assert!(is_spoken(&handle, &mut stream, "second").await);
    handle
        .send(SpeakCommand::SetRepeatWindow(window(1)))
        .await
        .unwrap();

    let spoken = is_spoken(&handle, &mut stream, "first").await;

    assert!(spoken, "only \"second\" is inside a window of one");
}
