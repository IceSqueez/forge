use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_audio::{PlaybackCorrelation, RemoteDestinationId};
use forge_registry::CancelSignal;
use forge_runtime::{
    EngineVoiceError, EngineVoiceRef, ShowSpeech, SpeakDispatchError, SpeakDispatcher,
    SpeakingViewer, SpeechOrigin, SpeechStartSignal, VoiceDescriptor,
};
use forge_script::{ScriptSpeakError, SpeakRequester};
use forge_speak_queue::{
    PlaybackTarget, Priority, RequestId, SpeakCommand, SpeakEvent, SpeakQueueHandle, SpeakRequest,
};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::{AliasId, AliasState, VoiceAlias};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

const SPEAK_WAIT_HARD_CAP: Duration = Duration::from_secs(600);
const SPEAK_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SYSTEM_SPEAKER_ID: &str = "system";
const SYSTEM_SPEAKER_NAME: &str = "Forge";

fn queue_identity(viewer: Option<SpeakingViewer>) -> (String, String) {
    match viewer {
        Some(viewer) if viewer.platform.is_empty() => (viewer.id, viewer.name),
        Some(viewer) => (format!("{}:{}", viewer.platform, viewer.id), viewer.name),
        None => (SYSTEM_SPEAKER_ID.to_owned(), SYSTEM_SPEAKER_NAME.to_owned()),
    }
}

pub struct SpeakBridge {
    handle: Arc<SpeakQueueHandle>,
}

impl SpeakBridge {
    pub fn new(handle: Arc<SpeakQueueHandle>) -> Self {
        Self { handle }
    }

    async fn enqueue(
        &self,
        text: String,
        engine_override: Option<EngineId>,
        voice_override: Option<VoiceId>,
        is_reward: bool,
        target: Option<PlaybackTarget>,
        origin: SpeechOrigin,
    ) -> Result<RequestId, String> {
        let request_id = RequestId::new();
        let (viewer_id, viewer_name) = queue_identity(origin.viewer);
        let request = SpeakRequest {
            request_id: request_id.clone(),
            viewer_id,
            viewer_name,
            text,
            priority: Priority::Normal,
            engine_override,
            voice_override,
            source_event_id: origin.caused_by,
            is_reward,
            target,
            message_emotes: origin.message_emotes.into_iter().collect(),
        };
        self.handle
            .send(SpeakCommand::Enqueue(request))
            .await
            .map_err(|e| e.to_string())?;
        Ok(request_id)
    }

    fn voice_descriptors(&self) -> Vec<VoiceDescriptor> {
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

    fn installed_voice_overrides(
        &self,
        voice: Option<&str>,
    ) -> Result<(Option<EngineId>, Option<VoiceId>), EngineVoiceError> {
        let Some(encoded) = voice else {
            return Ok((None, None));
        };
        let voice = EngineVoiceRef::installed(encoded, &self.voice_descriptors())?;
        Ok((
            Some(EngineId(voice.engine_id)),
            Some(VoiceId(voice.voice_id)),
        ))
    }

    fn dispatch_voice_overrides(
        &self,
        voice: Option<&str>,
    ) -> Result<(Option<EngineId>, Option<VoiceId>), SpeakDispatchError> {
        self.installed_voice_overrides(voice)
            .map_err(|e| SpeakDispatchError::Dispatch(e.to_string()))
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
        voice: Option<String>,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        let (engine, voice) = self.dispatch_voice_overrides(voice.as_deref())?;
        self.enqueue(text, engine, voice, false, None, origin)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_reward_sourced(
        &self,
        text: String,
        voice: Option<String>,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        let (engine, voice) = self.dispatch_voice_overrides(voice.as_deref())?;
        self.enqueue(text, engine, voice, true, None, origin)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_with_engine(
        &self,
        text: String,
        engine_id: String,
        origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        self.enqueue(text, Some(EngineId(engine_id)), None, false, None, origin)
            .await
            .map(|_| ())
            .map_err(SpeakDispatchError::Dispatch)
    }

    async fn speak_and_wait(
        &self,
        text: String,
        voice: Option<String>,
        is_reward: bool,
        origin: SpeechOrigin,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let (engine, voice) = self.dispatch_voice_overrides(voice.as_deref())?;
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(text, engine, voice, is_reward, None, origin)
            .await
            .map_err(SpeakDispatchError::Dispatch)?;
        wait_for_terminal(&mut events, &request_id, cancel).await
    }

    async fn speak_with_engine_and_wait(
        &self,
        text: String,
        engine_id: String,
        origin: SpeechOrigin,
        cancel: CancelSignal,
    ) -> Result<(), SpeakDispatchError> {
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(text, Some(EngineId(engine_id)), None, false, None, origin)
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
        let (engine, voice) = self.dispatch_voice_overrides(speech.voice.as_deref())?;
        let mut events = self.handle.subscribe();
        let request_id = self
            .enqueue(
                speech.text,
                engine,
                voice,
                false,
                Some(target),
                speech.origin,
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
        self.voice_descriptors()
    }

    fn check_voice(&self, encoded: &str) -> Result<(), EngineVoiceError> {
        self.installed_voice_overrides(Some(encoded)).map(|_| ())
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
    async fn speak(&self, text: String, voice: Option<String>) -> Result<(), ScriptSpeakError> {
        let (engine, voice) = self
            .installed_voice_overrides(voice.as_deref())
            .map_err(|e| ScriptSpeakError::UnknownVoice(e.to_string()))?;
        self.enqueue(text, engine, voice, false, None, SpeechOrigin::default())
            .await
            .map(|_| ())
            .map_err(ScriptSpeakError::QueueUnavailable)
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

    struct Harness {
        per_user_limit: usize,
        aliases: Vec<forge_voice::VoiceAlias>,
        pipeline: forge_tts_pipeline::PipelineConfig,
        bus: Arc<dyn forge_events::EventPublisher>,
        engines: forge_tts_core::TtsRegistry,
    }

    impl Default for Harness {
        fn default() -> Self {
            Self {
                per_user_limit: forge_speak_queue::QueueConfig::default().per_user_limit,
                aliases: Vec::new(),
                pipeline: forge_tts_pipeline::PipelineConfig::default(),
                bus: Arc::new(NullPublisher),
                engines: forge_tts_core::TtsRegistry::new(),
            }
        }
    }

    impl Harness {
        fn spawn(self) -> (Arc<SpeakQueueHandle>, forge_speak_queue::SpeakEventStream) {
            let resolver = forge_voice::VoiceAliasResolver::new(
                self.aliases,
                forge_voice::AssignmentStrategy::DeterministicByName,
                forge_voice::IgnoreProfile::default(),
                forge_voice::SynthesisDefaults::default(),
            );
            let deps = forge_speak_queue::QueueDeps {
                registry: Arc::new(std::sync::RwLock::new(self.engines)),
                resolver: Arc::new(std::sync::RwLock::new(resolver)),
                pipeline: forge_speak_queue::PipelineConfigHandle::new(self.pipeline),
                audio_sink: Arc::new(forge_audio::NullSink),
                event_bus: self.bus,
                disabled_engines: std::collections::HashSet::new(),
                engine_gains: std::collections::HashMap::new(),
            };
            let config = forge_speak_queue::QueueConfig {
                per_user_limit: self.per_user_limit,
                ..forge_speak_queue::QueueConfig::default()
            };
            let (handle, stream) = forge_speak_queue::spawn(config, deps);
            (Arc::new(handle), stream)
        }
    }

    fn voiceless_queue() -> (Arc<SpeakQueueHandle>, forge_speak_queue::SpeakEventStream) {
        Harness::default().spawn()
    }

    fn viewer(platform: &str, id: &str, name: &str) -> SpeakingViewer {
        SpeakingViewer {
            platform: platform.to_owned(),
            id: id.to_owned(),
            name: name.to_owned(),
        }
    }

    fn from_viewer(viewer: SpeakingViewer) -> SpeechOrigin {
        SpeechOrigin {
            viewer: Some(viewer),
            caused_by: None,
            ..Default::default()
        }
    }

    async fn next_matching<T>(
        stream: &mut forge_speak_queue::SpeakEventStream,
        mut pick: impl FnMut(SpeakEvent) -> Option<T>,
    ) -> T {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(event) = stream.recv().await
                    && let Some(found) = pick(event)
                {
                    return found;
                }
            }
        })
        .await
        .expect("the queue never emitted the expected event")
    }

    async fn admissions(
        stream: &mut forge_speak_queue::SpeakEventStream,
        count: usize,
    ) -> Vec<bool> {
        let mut admitted = Vec::new();
        while admitted.len() < count {
            admitted.push(
                next_matching(stream, |event| match event {
                    SpeakEvent::Enqueued { .. } => Some(true),
                    SpeakEvent::Rejected { .. } => Some(false),
                    _ => None,
                })
                .await,
            );
        }
        admitted
    }

    async fn skip_reason(
        bridge: &SpeakBridge,
        stream: &mut forge_speak_queue::SpeakEventStream,
        origin: SpeechOrigin,
    ) -> String {
        SpeakDispatcher::speak(bridge, "hello chat".to_owned(), None, origin)
            .await
            .unwrap();
        next_matching(stream, |event| match event {
            SpeakEvent::Skipped { reason, .. } => Some(reason),
            _ => None,
        })
        .await
    }

    #[test]
    fn the_queue_identity_is_platform_qualified_or_the_system_speaker() {
        for (origin_viewer, expected) in [
            (
                Some(viewer("twitch", "141981764", "NovaFox")),
                ("twitch:141981764", "NovaFox"),
            ),
            (
                Some(viewer("", "141981764", "NovaFox")),
                ("141981764", "NovaFox"),
            ),
            (None, ("system", "Forge")),
        ] {
            let (id, name) = queue_identity(origin_viewer.clone());

            assert_eq!((id.as_str(), name.as_str()), expected, "{origin_viewer:?}");
        }
    }

    struct CapturingBus(std::sync::Mutex<Vec<forge_events::Event>>);

    impl forge_events::EventPublisher for CapturingBus {
        fn publish(&self, event: forge_events::Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn a_chat_speech_is_queued_as_caused_by_its_triggering_event() {
        let bus = Arc::new(CapturingBus(std::sync::Mutex::new(Vec::new())));
        let (handle, mut stream) = Harness {
            bus: bus.clone(),
            ..Harness::default()
        }
        .spawn();
        handle.send(SpeakCommand::Pause).await.unwrap();
        let bridge = SpeakBridge::new(handle);
        let trigger = forge_types::EventId::new();

        SpeakDispatcher::speak(
            &bridge,
            "hello chat".to_owned(),
            None,
            SpeechOrigin {
                viewer: Some(viewer("twitch", "141981764", "NovaFox")),
                caused_by: Some(trigger),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        admissions(&mut stream, 1).await;

        let enqueued: Vec<_> = bus
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind == "speak.enqueued")
            .map(|event| event.caused_by)
            .collect();
        assert_eq!(enqueued, vec![Some(trigger)]);
    }

    struct CapturingEngine {
        id: EngineId,
        heard: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl forge_tts_core::TtsEngine for CapturingEngine {
        fn engine_id(&self) -> &EngineId {
            &self.id
        }

        fn capabilities(&self) -> &forge_tts_core::EngineCapabilities {
            static CAPS: forge_tts_core::EngineCapabilities = forge_tts_core::EngineCapabilities {
                ssml: false,
                neural_voices: false,
                streaming: false,
                custom_lexicons: false,
            };
            &CAPS
        }

        async fn list_voices(
            &self,
        ) -> Result<Vec<forge_tts_core::TtsVoice>, forge_tts_core::TtsError> {
            Ok(vec![forge_tts_core::TtsVoice {
                id: VoiceId("cap-1".to_owned()),
                name: "cap-1".to_owned(),
                locale: "en-US".to_owned(),
                gender: forge_tts_core::VoiceGender::Neutral,
                engine_id: self.id.clone(),
                is_neural: false,
                sample_rate_hint: 22_050,
            }])
        }

        async fn synthesize(
            &self,
            request: forge_tts_core::SynthesisRequest,
        ) -> Result<forge_audio::PcmBuffer, forge_tts_core::TtsError> {
            self.heard.lock().unwrap().push(request.text);
            Ok(forge_audio::PcmBuffer::new(vec![0i16; 4], 22_050, 1))
        }
    }

    struct CapturingFactory(Arc<std::sync::Mutex<Vec<String>>>);

    impl forge_tts_core::TtsEngineFactory for CapturingFactory {
        fn create(&self) -> Result<Box<dyn forge_tts_core::TtsEngine>, forge_tts_core::TtsError> {
            Ok(Box::new(CapturingEngine {
                id: EngineId("cap".to_owned()),
                heard: Arc::clone(&self.0),
            }))
        }
    }

    fn twitch_chat_with_emote(code: &str) -> forge_events::Event {
        forge_events::Event::new(
            forge_events::EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({
                forge_types::ChatPayload::KEY: {
                    "platform_msg_id": "m-1",
                    "author": "NovaFox",
                    "author_color": null,
                    "segments": [
                        { "type": "emote", "id": "425618", "name": code },
                        { "type": "text", "text": " that was funny" },
                    ],
                    "badges": [],
                    "is_event": false,
                    "event_detail": null,
                }
            }),
        )
    }

    #[tokio::test]
    async fn an_emote_seen_in_twitch_chat_is_not_read_out_of_a_later_reward() {
        use forge_registry::{RunContext, SubActionRunner};
        use forge_types::{ArgStack, SubActionConfig, Variant};

        let bus = forge_runtime::EventBus::new(Arc::new(forge_runtime::NullEventLogRepo));
        let lexicon = forge_runtime::TwitchEmoteLexicon::default();
        forge_runtime::spawn_twitch_emote_learning(&bus, lexicon.clone());
        bus.publish(twitch_chat_with_emote("LUL"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while lexicon.codes_in("LUL").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the chat emote was never learned");

        let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut engines = forge_tts_core::TtsRegistry::new();
        engines.register(
            EngineId("cap".to_owned()),
            Arc::new(CapturingFactory(Arc::clone(&heard))),
        );
        let (handle, mut stream) = Harness {
            pipeline: forge_tts_pipeline::PipelineConfig {
                strip_reward_emotes: true,
                ..Default::default()
            },
            engines,
            ..Harness::default()
        }
        .spawn();
        let runner =
            forge_runtime::audio_runners::SpeakRunner::new(Arc::new(SpeakBridge::new(handle)))
                .with_reward_emotes(lexicon);
        let stack = ArgStack::new()
            .set(
                "user_id".to_owned(),
                Variant::String("141981764".to_owned()),
            )
            .set(
                "user_name".to_owned(),
                Variant::String("NovaFox".to_owned()),
            )
            .set(
                "user_platform".to_owned(),
                Variant::String("twitch".to_owned()),
            )
            .set("reward.id".to_owned(), Variant::String("r-1".to_owned()));
        let mut config = SubActionConfig::new();
        config.insert("text".to_owned(), Variant::String("LUL hi".to_owned()));
        config.insert("wait_for_completion".to_owned(), Variant::Bool(false));
        let ctx = RunContext::leaf(&stack, 0, forge_types::EventId::new(), &NullPublisher);

        runner.execute(&config, &ctx).await;
        next_matching(&mut stream, |event| match event {
            SpeakEvent::Started { .. } => Some(()),
            _ => None,
        })
        .await;

        assert_eq!(*heard.lock().unwrap(), vec!["hi".to_owned()]);
    }

    #[tokio::test]
    async fn the_per_user_limit_counts_each_viewer_separately() {
        let nova = || from_viewer(viewer("twitch", "1", "NovaFox"));
        let aurora = || from_viewer(viewer("twitch", "2", "Aurora"));
        let kick_one = || from_viewer(viewer("kick", "1", "NovaFox"));
        for (label, origins, expected) in [
            ("two viewers", vec![nova(), aurora()], vec![true, true]),
            (
                "one viewer bursting",
                vec![nova(), nova()],
                vec![true, false],
            ),
            (
                "the same id on two platforms",
                vec![nova(), kick_one()],
                vec![true, true],
            ),
        ] {
            let (handle, mut stream) = Harness {
                per_user_limit: 1,
                ..Harness::default()
            }
            .spawn();
            handle.send(SpeakCommand::Pause).await.unwrap();
            let bridge = SpeakBridge::new(handle);
            let count = origins.len();

            for origin in origins {
                SpeakDispatcher::speak(&bridge, "hi".to_owned(), None, origin)
                    .await
                    .unwrap();
            }

            assert_eq!(admissions(&mut stream, count).await, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn a_bot_viewers_speech_is_dropped_by_the_bot_filter() {
        let mut pipeline = forge_tts_pipeline::PipelineConfig::default();
        pipeline.skip_rules.from_bot_accounts = true;
        pipeline.skip_rules.bot_accounts = vec!["relaybot".to_owned()];
        let (handle, mut stream) = Harness {
            pipeline,
            ..Harness::default()
        }
        .spawn();
        let bridge = SpeakBridge::new(handle);

        let reason = skip_reason(
            &bridge,
            &mut stream,
            from_viewer(viewer("twitch", "5", "RelayBot")),
        )
        .await;

        assert!(reason.contains("bot"), "skipped for {reason:?}");
    }

    #[tokio::test]
    async fn a_blocked_viewers_speech_is_dropped() {
        for key in ["twitch:66", "grumpy"] {
            let (handle, mut stream) = Harness {
                aliases: vec![forge_voice::VoiceAlias {
                    id: forge_voice::AliasId::new(),
                    viewer_id: key.to_owned(),
                    viewer_name: key.to_owned(),
                    engine_id: EngineId("piper".to_owned()),
                    voice_id: VoiceId("amy".to_owned()),
                    pitch_semitones: None,
                    rate_multiplier: None,
                    state: forge_voice::AliasState::Blocked,
                }],
                ..Harness::default()
            }
            .spawn();
            let bridge = SpeakBridge::new(handle);

            let reason = skip_reason(
                &bridge,
                &mut stream,
                from_viewer(viewer("twitch", "66", "Grumpy")),
            )
            .await;

            assert!(
                reason.contains("blocked"),
                "block keyed {key:?} skipped for {reason:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_donation_show_speaks_with_the_donors_alias() {
        let (handle, mut stream) = Harness {
            aliases: vec![forge_voice::VoiceAlias {
                id: forge_voice::AliasId::new(),
                viewer_id: "aurora".to_owned(),
                viewer_name: "Aurora".to_owned(),
                engine_id: EngineId("piper".to_owned()),
                voice_id: VoiceId("donor-voice".to_owned()),
                pitch_semitones: None,
                rate_multiplier: None,
                state: forge_voice::AliasState::Active,
            }],
            ..Harness::default()
        }
        .spawn();
        handle.send(SpeakCommand::Pause).await.unwrap();
        let bridge = SpeakBridge::new(handle);
        let cancel = CancelSignal::new();
        let speech = ShowSpeech {
            text: "thanks for the five".to_owned(),
            voice: None,
            overlay: "stage-alert".to_owned(),
            show: "01J9ZC4W6R7Q2N3M4K5P6S7T8V".to_owned(),
            origin: from_viewer(viewer("youtube", "UC7x", "Aurora")),
        };

        let (_, preview) = tokio::join!(
            bridge.speak_for_show(speech, cancel.clone(), SpeechStartSignal::new()),
            async {
                let preview = next_matching(&mut stream, |event| match event {
                    SpeakEvent::Enqueued { voice_preview, .. } => Some(voice_preview),
                    _ => None,
                })
                .await;
                cancel.cancel();
                preview
            }
        );

        assert!(
            preview.contains("donor-voice"),
            "the show speech was voiced as {preview:?}"
        );
    }

    #[tokio::test]
    async fn a_cancelled_show_speech_is_taken_out_of_the_queue_rather_than_left_to_play_later() {
        let (handle, mut stream) = voiceless_queue();
        handle.send(SpeakCommand::Pause).await.unwrap();
        let bridge = SpeakBridge::new(Arc::clone(&handle));
        let cancel = CancelSignal::new();
        let speech = ShowSpeech {
            text: "thanks for the five".to_owned(),
            voice: None,
            overlay: "stage-alert".to_owned(),
            show: "01J9ZC4W6R7Q2N3M4K5P6S7T8V".to_owned(),
            origin: SpeechOrigin::default(),
        };

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
                None,
            )
            .await
            .unwrap();
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

    struct ListedVoicesEngine {
        id: EngineId,
    }

    const LISTED_VOICES: [(&str, &str); 2] = [("cap-a", "Alpha"), ("en/b", "Beta")];

    #[async_trait]
    impl forge_tts_core::TtsEngine for ListedVoicesEngine {
        fn engine_id(&self) -> &EngineId {
            &self.id
        }

        fn capabilities(&self) -> &forge_tts_core::EngineCapabilities {
            static CAPS: forge_tts_core::EngineCapabilities = forge_tts_core::EngineCapabilities {
                ssml: false,
                neural_voices: false,
                streaming: false,
                custom_lexicons: false,
            };
            &CAPS
        }

        async fn list_voices(
            &self,
        ) -> Result<Vec<forge_tts_core::TtsVoice>, forge_tts_core::TtsError> {
            Ok(LISTED_VOICES
                .iter()
                .map(|(id, name)| forge_tts_core::TtsVoice {
                    id: VoiceId((*id).to_owned()),
                    name: (*name).to_owned(),
                    locale: "en-US".to_owned(),
                    gender: forge_tts_core::VoiceGender::Neutral,
                    engine_id: self.id.clone(),
                    is_neural: false,
                    sample_rate_hint: 22_050,
                })
                .collect())
        }

        async fn synthesize(
            &self,
            _: forge_tts_core::SynthesisRequest,
        ) -> Result<forge_audio::PcmBuffer, forge_tts_core::TtsError> {
            Ok(forge_audio::PcmBuffer::new(vec![0i16; 4], 22_050, 1))
        }
    }

    struct ListedVoicesFactory;

    impl forge_tts_core::TtsEngineFactory for ListedVoicesFactory {
        fn create(&self) -> Result<Box<dyn forge_tts_core::TtsEngine>, forge_tts_core::TtsError> {
            Ok(Box::new(ListedVoicesEngine {
                id: EngineId("cap".to_owned()),
            }))
        }
    }

    async fn queue_with_listed_voices()
    -> (Arc<SpeakQueueHandle>, forge_speak_queue::SpeakEventStream) {
        let mut engines = forge_tts_core::TtsRegistry::new();
        engines.register(EngineId("cap".to_owned()), Arc::new(ListedVoicesFactory));
        let (handle, stream) = Harness {
            engines,
            ..Harness::default()
        }
        .spawn();
        tokio::time::timeout(Duration::from_secs(5), async {
            while handle.available_voices().len() < LISTED_VOICES.len() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the engine's voices never reached the catalog");
        handle.send(SpeakCommand::Pause).await.unwrap();
        (handle, stream)
    }

    #[derive(Debug, Clone, Copy)]
    enum VoicedPath {
        Step,
        Reward,
        Script,
        Show,
    }

    async fn enqueued_preview(stream: &mut forge_speak_queue::SpeakEventStream) -> String {
        next_matching(stream, |event| match event {
            SpeakEvent::Enqueued { voice_preview, .. } => Some(voice_preview),
            _ => None,
        })
        .await
    }

    async fn enqueue_with_voice(
        bridge: &SpeakBridge,
        stream: &mut forge_speak_queue::SpeakEventStream,
        path: VoicedPath,
        voice: &str,
    ) -> String {
        let text = "hello chat".to_owned();
        let voice = Some(voice.to_owned());
        match path {
            VoicedPath::Step => {
                SpeakDispatcher::speak(bridge, text, voice, SpeechOrigin::default())
                    .await
                    .unwrap();
                enqueued_preview(stream).await
            }
            VoicedPath::Reward => {
                bridge
                    .speak_reward_sourced(text, voice, SpeechOrigin::default())
                    .await
                    .unwrap();
                enqueued_preview(stream).await
            }
            VoicedPath::Script => {
                SpeakRequester::speak(bridge, text, voice).await.unwrap();
                enqueued_preview(stream).await
            }
            VoicedPath::Show => {
                let cancel = CancelSignal::new();
                let speech = ShowSpeech {
                    text,
                    voice,
                    overlay: "stage-alert".to_owned(),
                    show: "01J9ZC4W6R7Q2N3M4K5P6S7T8V".to_owned(),
                    origin: SpeechOrigin::default(),
                };
                let (_, shown) = tokio::join!(
                    bridge.speak_for_show(speech, cancel.clone(), SpeechStartSignal::new()),
                    async {
                        let shown = enqueued_preview(stream).await;
                        cancel.cancel();
                        shown
                    }
                );
                shown
            }
        }
    }

    #[tokio::test]
    async fn a_chosen_engine_voice_overrides_voice_resolution_on_every_voiced_path() {
        for path in [
            VoicedPath::Step,
            VoicedPath::Reward,
            VoicedPath::Script,
            VoicedPath::Show,
        ] {
            for (voice_id, name) in LISTED_VOICES {
                let (handle, mut stream) = queue_with_listed_voices().await;
                let bridge = SpeakBridge::new(handle);

                let preview =
                    enqueue_with_voice(&bridge, &mut stream, path, &format!("cap/{voice_id}"))
                        .await;

                assert!(
                    preview.ends_with(name),
                    "{path:?} asked for cap/{voice_id} but was voiced as {preview:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_voice_missing_from_the_catalog_is_refused_before_it_is_queued() {
        let (handle, _stream) = queue_with_listed_voices().await;
        let bridge = SpeakBridge::new(handle);

        let step = SpeakDispatcher::speak(
            &bridge,
            "hi".to_owned(),
            Some("cap/ghost".to_owned()),
            SpeechOrigin::default(),
        )
        .await;
        let script =
            SpeakRequester::speak(&bridge, "hi".to_owned(), Some("cap-a".to_owned())).await;

        assert!(
            matches!(
                &step,
                Err(SpeakDispatchError::Dispatch(reason))
                    if reason.contains("voice \"ghost\" of engine \"cap\" is not installed")
            ),
            "a step with an uninstalled voice got {step:?}"
        );
        assert!(
            matches!(
                &script,
                Err(ScriptSpeakError::UnknownVoice(reason)) if reason.contains("is not an engine voice")
            ),
            "a script with a bare voice id got {script:?}"
        );
    }

    #[tokio::test]
    async fn check_voice_accepts_only_an_installed_engine_voice() {
        let (handle, _stream) = queue_with_listed_voices().await;
        let bridge = SpeakBridge::new(handle);

        for (encoded, expected) in [
            ("cap/cap-a", Ok(())),
            ("cap/en/b", Ok(())),
            (
                "cap/ghost",
                Err(EngineVoiceError::NotInstalled {
                    engine_id: "cap".to_owned(),
                    voice_id: "ghost".to_owned(),
                }),
            ),
            (
                "cap-a",
                Err(EngineVoiceError::NotAnEngineVoice("cap-a".to_owned())),
            ),
        ] {
            assert_eq!(bridge.check_voice(encoded), expected, "{encoded:?}");
        }
    }
}
