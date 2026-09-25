use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{PlaybackCorrelation, RemoteDestinationId};
use forge_registry::CancelSignal;
use forge_runtime::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeechStartSignal, VoiceDescriptor,
};
use forge_script::SpeakRequester;
use forge_speak_queue::{
    PlaybackTarget, Priority, RequestId, SpeakCommand, SpeakEvent, SpeakQueueHandle, SpeakRequest,
};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::{AliasId, AliasState, VoiceAlias};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

const SPEAK_WAIT_HARD_CAP: Duration = Duration::from_secs(600);
const SPEAK_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub struct SpeakBridge {
    handle: Arc<SpeakQueueHandle>,
}

impl SpeakBridge {
    pub fn new(handle: Arc<SpeakQueueHandle>) -> Self {
        Self { handle }
    }

    #[allow(clippy::too_many_arguments)]
    async fn enqueue(
        &self,
        text: String,
        alias_override: Option<AliasId>,
        engine_override: Option<EngineId>,
        voice_override: Option<VoiceId>,
        is_reward: bool,
        target: Option<PlaybackTarget>,
    ) -> Result<RequestId, String> {
        let request_id = RequestId::new();
        let request = SpeakRequest {
            request_id: request_id.clone(),
            viewer_id: "system".to_owned(),
            viewer_name: "Forge".to_owned(),
            text,
            priority: Priority::Normal,
            alias_override,
            engine_override,
            voice_override,
            source_event_id: None,
            is_reward,
            target,
        };
        self.handle
            .send(SpeakCommand::Enqueue(request))
            .await
            .map_err(|e| e.to_string())?;
        Ok(request_id)
    }

    async fn dispatch(&self, cmd: SpeakCommand) -> Result<(), SpeakDispatchError> {
        self.handle
            .send(cmd)
            .await
            .map_err(|e| SpeakDispatchError::Dispatch(e.to_string()))
    }
}

async fn wait_for_terminal(
    events: &mut broadcast::Receiver<SpeakEvent>,
    request_id: &RequestId,
    cancel: CancelSignal,
) -> Result<(), SpeakDispatchError> {
    wait_marking_start(events, request_id, cancel, None).await
}

async fn wait_marking_start(
    events: &mut broadcast::Receiver<SpeakEvent>,
    request_id: &RequestId,
    cancel: CancelSignal,
    started: Option<&SpeechStartSignal>,
) -> Result<(), SpeakDispatchError> {
    let mut events_lost = false;
    let wait = async {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(SPEAK_WAIT_POLL_INTERVAL) => {
                    if cancel.is_cancelled() {
                        return Err(SpeakDispatchError::Dispatch(
                            "speak wait cancelled".to_owned(),
                        ));
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(SpeakEvent::Started { request_id: rid, voice_id, .. })
                            if &rid == request_id && !voice_id.0.is_empty() =>
                        {
                            if let Some(started) = started {
                                started.mark_started();
                            }
                        }
                        Ok(SpeakEvent::Finished { request_id: rid }) if &rid == request_id => {
                            return Ok(());
                        }
                        Ok(SpeakEvent::Failed { request_id: rid, error }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(error));
                        }
                        Ok(SpeakEvent::Skipped { request_id: rid, reason }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(format!(
                                "speak skipped: {reason}"
                            )));
                        }
                        Ok(SpeakEvent::Removed { request_id: rid }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(
                                "speak removed from the queue".to_owned(),
                            ));
                        }
                        Ok(SpeakEvent::Rejected { request_id: rid, reason }) if &rid == request_id => {
                            return Err(SpeakDispatchError::Dispatch(format!(
                                "speak rejected: {reason}"
                            )));
                        }
                        Ok(_) => continue,
                        Err(RecvError::Lagged(missed)) => {
                            events_lost = true;
                            tracing::warn!(
                                request = %request_id.0,
                                missed,
                                "speak wait fell behind the queue's events; still waiting for this speech to end"
                            );
                        }
                        Err(RecvError::Closed) => {
                            return Err(SpeakDispatchError::Dispatch(
                                "speak event stream closed".to_owned(),
                            ));
                        }
                    }
                }
            }
        }
    };
    match tokio::time::timeout(SPEAK_WAIT_HARD_CAP, wait).await {
        Ok(result) => result,
        Err(_) if events_lost => Err(SpeakDispatchError::Dispatch(
            "speak outcome unknown: queue events were missed while waiting and the end of this speech was never seen".to_owned(),
        )),
        Err(_) => Err(SpeakDispatchError::Dispatch(
            "speak wait timed out".to_owned(),
        )),
    }
}

#[async_trait]
impl SpeakDispatcher for SpeakBridge {
    async fn speak(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(
            text,
            voice_id_override.map(AliasId),
            None,
            None,
            false,
            None,
        )
        .await
        .map(|_| ())
        .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_reward_sourced(
        &self,
        text: String,
        voice_id_override: Option<String>,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, voice_id_override.map(AliasId), None, None, true, None)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_alias(
        &self,
        text: String,
        alias_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, Some(AliasId(alias_id)), None, None, false, None)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_engine(
        &self,
        text: String,
        engine_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, None, Some(EngineId(engine_id)), None, false, None)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_voice(
        &self,
        text: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, None, None, Some(VoiceId(voice_id)), false, None)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_and_wait(
        &self,
        text: String,
        voice_id_override: Option<String>,
        is_reward: bool,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(
                text,
                voice_id_override.map(AliasId),
                None,
                None,
                is_reward,
                None,
            )
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        wait_for_terminal(&mut events, &request_id, cancel).await
    }

    async fn speak_with_engine_and_wait(
        &self,
        text: String,
        engine_id: String,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(text, None, Some(EngineId(engine_id)), None, false, None)
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        wait_for_terminal(&mut events, &request_id, cancel).await
    }

    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        cancel: CancelSignal,
        started: SpeechStartSignal,
    ) -> Result<(), SpeakDispatchError> {
        let target = PlaybackTarget {
            destination: RemoteDestinationId::new(speech.overlay),
            correlation: PlaybackCorrelation::new(speech.show),
        };
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(
                speech.text,
                speech.voice_alias.map(AliasId),
                None,
                None,
                false,
                Some(target),
            )
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        let outcome =
            wait_marking_start(&mut events, &request_id, cancel.clone(), Some(&started)).await;
        if cancel.is_cancelled() {
            self.dispatch(SpeakCommand::Cancel(request_id)).await?;
        }
        outcome
    }

    async fn stop_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Skip).await
    }

    async fn pause(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Pause).await
    }

    async fn resume(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Resume).await
    }

    async fn skip_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::Skip).await
    }

    async fn clear_keep_current(&self) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::ClearPending).await
    }

    async fn get_queue_depth(&self) -> usize {
        self.handle.queue_depth()
    }

    async fn get_available_voices(&self) -> Vec<VoiceDescriptor> {
        self.handle
            .available_voices()
            .iter()
            .map(|v| VoiceDescriptor {
                id: v.id.0.clone(),
                name: v.name.clone(),
                locale: v.locale.clone(),
                engine_id: v.engine_id.0.clone(),
            })
            .collect()
    }

    async fn get_engines(&self) -> Vec<String> {
        self.handle.engines().into_iter().map(|e| e.0).collect()
    }

    async fn alias_set(
        &self,
        viewer_id: String,
        viewer_name: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        let alias = VoiceAlias {
            id: AliasId::new(),
            viewer_id,
            viewer_name,
            engine_id: EngineId(engine_id),
            voice_id: VoiceId(voice_id),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        };
        self.dispatch(SpeakCommand::SetAlias(alias)).await
    }

    async fn alias_switch(
        &self,
        viewer_id: String,
        engine_id: String,
        voice_id: String,
    ) -> Result<(), SpeakDispatchError> {
        self.dispatch(SpeakCommand::SwitchAlias {
            viewer_id,
            engine_id: EngineId(engine_id),
            voice_id: VoiceId(voice_id),
        })
        .await
    }
}

#[async_trait]
impl SpeakRequester for SpeakBridge {
    async fn speak(&self, text: String, voice_id_override: Option<String>) {
        if let Err(e) = self
            .enqueue(
                text,
                voice_id_override.map(AliasId),
                None,
                None,
                false,
                None,
            )
            .await
        {
            tracing::warn!(error = %e, "forge::tts::speak failed");
        }
    }

    async fn skip(&self) {
        if let Err(e) = self.handle.send(SpeakCommand::Skip).await {
            tracing::warn!(error = %e, "forge::tts::skip failed");
        }
    }

    async fn clear(&self) {
        if let Err(e) = self.handle.send(SpeakCommand::Clear).await {
            tracing::warn!(error = %e, "forge::tts::clear failed");
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    const CAPACITY: usize = 2;
    const OVERFLOW: usize = 5;

    fn unrelated() -> SpeakEvent {
        SpeakEvent::Progress {
            request_id: RequestId::new(),
            elapsed_secs: 1,
        }
    }

    fn lag(events: &broadcast::Sender<SpeakEvent>) {
        for _ in 0..OVERFLOW {
            events.send(unrelated()).unwrap();
        }
    }

    fn reason(result: Result<(), SpeakDispatchError>) -> String {
        match result {
            Err(SpeakDispatchError::Dispatch(reason)) => reason,
            Ok(()) => panic!("the wait resolved as a finished speech"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_lagged_wait_still_resolves_when_the_end_of_its_speech_arrives() {
        let (events, mut rx) = broadcast::channel(CAPACITY);
        let request_id = RequestId::new();
        lag(&events);
        events
            .send(SpeakEvent::Finished {
                request_id: request_id.clone(),
            })
            .unwrap();

        let result = wait_for_terminal(&mut rx, &request_id, CancelSignal::new()).await;

        assert!(
            result.is_ok(),
            "missing some unrelated queue events ended the wait: {result:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_that_never_sees_its_speech_end_reports_why_at_the_hard_cap() {
        for (lagged, expected) in [(true, "outcome unknown"), (false, "timed out")] {
            let (events, mut rx) = broadcast::channel(CAPACITY);
            if lagged {
                lag(&events);
            }
            let started = tokio::time::Instant::now();

            let reason =
                reason(wait_for_terminal(&mut rx, &RequestId::new(), CancelSignal::new()).await);

            assert!(
                reason.contains(expected),
                "lagged={lagged}: the wait ended with {reason:?}"
            );
            assert!(
                started.elapsed() >= SPEAK_WAIT_HARD_CAP,
                "lagged={lagged}: the wait gave up after {:?}, before the hard cap",
                started.elapsed()
            );
            drop(events);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_event_stream_reports_closed_even_after_a_lag() {
        for lagged in [false, true] {
            let (events, mut rx) = broadcast::channel(CAPACITY);
            if lagged {
                lag(&events);
            }
            drop(events);

            let reason =
                reason(wait_for_terminal(&mut rx, &RequestId::new(), CancelSignal::new()).await);

            assert!(
                reason.contains("closed"),
                "lagged={lagged}: a closed stream ended the wait with {reason:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_a_lagged_wait_ends_it_within_one_poll() {
        let (events, mut rx) = broadcast::channel(CAPACITY);
        lag(&events);
        let cancel = CancelSignal::new();
        cancel.cancel();
        let started = tokio::time::Instant::now();

        let reason = reason(wait_for_terminal(&mut rx, &RequestId::new(), cancel).await);

        assert!(
            reason.contains("cancelled"),
            "the wait ended with {reason:?}"
        );
        assert!(
            started.elapsed() <= SPEAK_WAIT_POLL_INTERVAL,
            "a cancelled wait ran for {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_ends_at_once_when_its_speech_is_removed_from_the_queue() {
        let (events, mut rx) = broadcast::channel(CAPACITY);
        let request_id = RequestId::new();
        events
            .send(SpeakEvent::Removed {
                request_id: request_id.clone(),
            })
            .unwrap();
        let started = tokio::time::Instant::now();

        let result = wait_for_terminal(&mut rx, &request_id, CancelSignal::new()).await;

        assert!(
            result.is_err() && started.elapsed() < SPEAK_WAIT_POLL_INTERVAL,
            "a removed speech left its waiter with {result:?} after {:?}",
            started.elapsed()
        );
    }

    struct NullPublisher;

    impl forge_events::EventPublisher for NullPublisher {
        fn publish(&self, _: forge_events::Event) {}
    }

    fn voiceless_queue() -> (Arc<SpeakQueueHandle>, forge_speak_queue::SpeakEventStream) {
        let resolver = forge_voice::VoiceAliasResolver::new(
            vec![],
            forge_voice::AssignmentStrategy::DeterministicByName,
            forge_voice::IgnoreProfile::default(),
            forge_voice::SynthesisDefaults::default(),
        );
        let deps = forge_speak_queue::QueueDeps {
            registry: Arc::new(std::sync::RwLock::new(forge_tts_core::TtsRegistry::new())),
            resolver: Arc::new(std::sync::RwLock::new(resolver)),
            pipeline: forge_speak_queue::PipelineConfigHandle::new(
                forge_tts_pipeline::PipelineConfig::default(),
            ),
            audio_sink: Arc::new(forge_audio::NullSink),
            event_bus: Arc::new(NullPublisher),
            disabled_engines: std::collections::HashSet::new(),
            engine_gains: std::collections::HashMap::new(),
        };
        let (handle, stream) =
            forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
        (Arc::new(handle), stream)
    }

    #[tokio::test]
    async fn a_cancelled_show_speech_is_taken_out_of_the_queue_rather_than_left_to_play_later() {
        let (handle, mut stream) = voiceless_queue();
        handle.send(SpeakCommand::Pause).await.unwrap();
        let bridge = SpeakBridge::new(Arc::clone(&handle));
        let cancel = CancelSignal::new();
        let speech = ShowSpeech {
            text: "thanks for the five".to_owned(),
            voice_alias: None,
            overlay: "stage-alert".to_owned(),
            show: "01J9ZC4W6R7Q2N3M4K5P6S7T8V".to_owned(),
        };

        // Why: without the cancel reaching the queue no removal ever arrives, so the test must
        // fail rather than stall.
        let (outcome, removed) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                bridge.speak_for_show(speech, cancel.clone(), SpeechStartSignal::new()),
                async {
                    let queued = loop {
                        if let Ok(SpeakEvent::Enqueued { request_id, .. }) = stream.recv().await {
                            break request_id;
                        }
                    };
                    cancel.cancel();
                    loop {
                        if let Ok(SpeakEvent::Removed { request_id }) = stream.recv().await
                            && request_id == queued
                        {
                            return handle.queue_depth();
                        }
                    }
                }
            )
        })
        .await
        .expect("the cancelled speech never left the queue");

        assert!(
            outcome.is_err() && removed == 0,
            "a show speech cut at the ceiling was still waiting in the queue ({removed} left, \
             outcome {outcome:?})"
        );
    }

    struct RecordingPages {
        pushed: std::sync::Mutex<Vec<serde_json::Value>>,
        arrived: tokio::sync::Notify,
    }

    #[async_trait]
    impl forge_runtime::OverlayFrameSink for RecordingPages {
        async fn deliver_content(
            &self,
            _: &forge_storage::OverlayId,
            content: serde_json::Value,
            _: Option<u64>,
        ) -> forge_runtime::OverlayReceivers {
            self.pushed.lock().unwrap().push(content);
            self.arrived.notify_waiters();
            forge_runtime::OverlayReceivers {
                sources: 1,
                preview_tabs: 0,
            }
        }

        async fn deliver_reload(&self, _: &forge_storage::OverlayId) {}

        async fn revoke(&self, _: &forge_storage::OverlayId) {}
    }

    impl RecordingPages {
        fn reveal(&self) -> Option<serde_json::Value> {
            self.pushed
                .lock()
                .unwrap()
                .iter()
                .find(|frame| frame["command"].as_str() == Some("reveal"))
                .cloned()
        }
    }

    #[tokio::test]
    async fn a_show_whose_speech_is_dropped_before_it_plays_is_revealed_silently_at_once() {
        use forge_overlay::kinds::alert::KIND_ID as ALERT;
        use forge_storage::{MockOverlayRepo, OverlayConfig, OverlayId, OverlayRepo, SettingsRepo};
        use forge_types::Variant;

        let (handle, _stream) = voiceless_queue();
        let (backend, _writes) = crate::test_support::test_backend();
        let mut repo = MockOverlayRepo::new();
        repo.expect_get()
            .returning(|id| Ok(Some(crate::test_support::overlay_named(id.as_str(), ALERT))));
        let mut kinds = forge_overlay::OverlayKindRegistry::new();
        forge_overlay::register_builtin_kinds(&mut kinds).unwrap();
        let pages = Arc::new(RecordingPages {
            pushed: std::sync::Mutex::new(Vec::new()),
            arrived: tokio::sync::Notify::new(),
        });
        let overlays = forge_runtime::OverlayServiceHandle::new(
            Arc::new(repo) as Arc<dyn OverlayRepo>,
            backend as Arc<dyn SettingsRepo>,
            Arc::new(kinds),
            forge_runtime::EventBus::new(Arc::new(crate::test_support::StubEventLog)),
            Some(Arc::clone(&pages) as Arc<dyn forge_runtime::OverlayFrameSink>),
        )
        .with_speech(Arc::new(SpeakBridge::new(handle)));
        let content = OverlayConfig::from([
            (
                "headline".to_owned(),
                Variant::String("new donation".to_owned()),
            ),
            (
                forge_overlay::config::SPEECH.to_owned(),
                Variant::String("thanks for the five".to_owned()),
            ),
        ]);

        overlays
            .send_to(
                &OverlayId::new("stage-alert"),
                &content,
                &forge_types::ArgStack::new(),
                None,
            )
            .await
            .unwrap();
        // Why: with no voice the queue drops the speech right after taking it, so the reveal is
        // due within milliseconds; the bound only turns a missing reveal into a failure.
        let revealed = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let arrived = pages.arrived.notified();
                if let Some(frame) = pages.reveal() {
                    return frame;
                }
                arrived.await;
            }
        })
        .await;

        assert!(
            revealed.is_ok(),
            "speech the queue took and then dropped (no voice, a TTS filter) was treated as \
             started, so the page is never told to show the alert: {:?}",
            pages.pushed.lock().unwrap()
        );
    }
}
