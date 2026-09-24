#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{
    AudioError, AudioSink, RemoteAudioDestination, RemoteClip, RemoteClipId, RemoteCommand,
    RemoteDelivery, RemoteDestinationId, RemoteSink, RemoteVerdict,
};
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent};
use forge_voice::AssignmentStrategy;
use tokio::sync::{Notify, mpsc};

use common::{make_deps, request, standard_registry, wait_for};

const DESTINATION: &str = "stage-audio";
const EVENT_WAIT_MS: u64 = 2_000;
const COMMAND_WAIT: Duration = Duration::from_secs(2);
/// Why: two control-poll ticks; a duplicate Stop from the observer would land inside it.
const SECOND_STOP_WINDOW: Duration = Duration::from_millis(100);

struct SilentPage {
    delivered: Notify,
    commands: mpsc::UnboundedSender<RemoteCommand>,
}

#[async_trait]
impl RemoteAudioDestination for SilentPage {
    async fn deliver(
        &self,
        _destination: &RemoteDestinationId,
        _clip: RemoteClip,
    ) -> Result<RemoteDelivery, AudioError> {
        self.delivered.notify_one();
        Ok(RemoteDelivery {
            clip_id: RemoteClipId::new("clip-on-the-page"),
            live_players: 1,
        })
    }

    async fn control(
        &self,
        _destination: &RemoteDestinationId,
        _clip_id: &RemoteClipId,
        command: RemoteCommand,
    ) -> Result<(), AudioError> {
        let _ = self.commands.send(command);
        Ok(())
    }

    async fn verdict(&self, _clip_id: &RemoteClipId) -> Result<RemoteVerdict, AudioError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn skipping_an_utterance_on_the_overlay_route_stops_it_on_the_page_exactly_once() {
    let (commands, mut pushed) = mpsc::unbounded_channel();
    let page = Arc::new(SilentPage {
        delivered: Notify::new(),
        commands,
    });
    let sink: Arc<dyn AudioSink> = Arc::new(RemoteSink::new(
        Arc::clone(&page) as Arc<dyn RemoteAudioDestination>,
        RemoteDestinationId::new(DESTINATION),
    ));
    let deps = make_deps(
        standard_registry(),
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let (handle, mut stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);

    handle
        .send(SpeakCommand::Enqueue(request(
            "troll",
            "a message to cut short",
        )))
        .await
        .unwrap();
    page.delivered.notified().await;
    handle.send(SpeakCommand::Skip).await.unwrap();
    wait_for(
        &mut stream,
        |e| matches!(e, SpeakEvent::Skipped { .. }),
        EVENT_WAIT_MS,
    )
    .await;

    let first = tokio::time::timeout(COMMAND_WAIT, pushed.recv()).await;
    assert!(
        matches!(first, Ok(Some(RemoteCommand::Stop))),
        "the skipped clip must be stopped on the page, got {first:?}"
    );
    let second = tokio::time::timeout(SECOND_STOP_WINDOW, pushed.recv()).await;
    assert!(
        second.is_err(),
        "the page must receive one command only, got {second:?}"
    );
}
