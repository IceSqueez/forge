#![allow(clippy::unwrap_used)]
mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakQueueHandle, build_config,
};
use forge_speech_types::{AssignmentStrategy, EngineId, PcmBuffer, TtsVoice, VoiceGender, VoiceId};
use forge_storage::{TtsPipelineSettings, UrlMode};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use forge_types::{MessageArg, MessageRef, VerbatimText};

use common::{make_deps, recording_sink, request, request_with_overrides, wait_for};

const ENGINE: &str = "scripted";
const VOICE: &str = "only-voice";

type Failure = fn() -> TtsError;

struct ScriptedEngine {
    id: EngineId,
    voices: Vec<TtsVoice>,
    spoken: Arc<Mutex<Vec<String>>>,
    failure: Option<Failure>,
}

#[async_trait]
impl TtsEngine for ScriptedEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }
    fn capabilities(&self) -> &EngineCapabilities {
        &common::PLAIN_CAPABILITIES
    }
    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(self.voices.clone())
    }
    async fn synthesize(&self, req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if let Some(failure) = self.failure {
            return Err(failure());
        }
        self.spoken.lock().unwrap().push(req.text);
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct ScriptedFactory {
    voices: Vec<TtsVoice>,
    spoken: Arc<Mutex<Vec<String>>>,
    failure: Option<Failure>,
}

impl TtsEngineFactory for ScriptedFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(ScriptedEngine {
            id: EngineId(ENGINE.into()),
            voices: self.voices.clone(),
            spoken: Arc::clone(&self.spoken),
            failure: self.failure,
        }))
    }
}

fn voice_in(locale: &str) -> TtsVoice {
    TtsVoice {
        id: VoiceId(VOICE.into()),
        name: VOICE.into(),
        locale: locale.into(),
        gender: VoiceGender::Neutral,
        engine_id: EngineId(ENGINE.into()),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

fn spawn_queue(
    locale: &str,
    failure: Option<Failure>,
) -> (SpeakQueueHandle, SpeakEventStream, Arc<Mutex<Vec<String>>>) {
    let spoken = Arc::new(Mutex::new(Vec::new()));
    let mut registry = TtsRegistry::new();
    registry.register(
        EngineId(ENGINE.into()),
        Arc::new(ScriptedFactory {
            voices: vec![voice_in(locale)],
            spoken: Arc::clone(&spoken),
            failure,
        }),
    );
    let (sink, _plays) = recording_sink();
    let mut deps = make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    let settings = TtsPipelineSettings {
        output_read_display_name_first: true,
        url_mode: UrlMode::Replace,
        ..TtsPipelineSettings::default()
    };
    deps.pipeline = PipelineConfigHandle::new(build_config(&[], &settings).dropping_invalid().0);
    let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    (handle, stream, spoken)
}

async fn first_failure(stream: &mut SpeakEventStream) -> MessageRef {
    let failure = Mutex::new(None);
    wait_for(stream, |event| {
        if let SpeakEvent::Failed { message, .. } = event {
            *failure.lock().unwrap() = Some(message.clone());
            return true;
        }
        false
    })
    .await;
    failure.into_inner().unwrap().unwrap()
}

#[tokio::test]
async fn the_spoken_name_prefix_and_link_word_follow_the_language_of_the_chosen_voice() {
    let mut spoken_by_locale = Vec::new();
    for locale in ["uk-UA", "en-US", "de-DE"] {
        let (handle, mut stream, spoken) = spawn_queue(locale, None);
        handle
            .send(SpeakCommand::Enqueue(request(
                "nova",
                "look https://example.com",
            )))
            .await
            .unwrap();
        wait_for(&mut stream, |event| {
            matches!(event, SpeakEvent::Finished { .. })
        })
        .await;
        spoken_by_locale.push(spoken.lock().unwrap().clone());
    }
    assert_eq!(
        spoken_by_locale,
        vec![
            vec!["nova каже: look посилання".to_owned()],
            vec!["nova says: look link".to_owned()],
            vec!["nova says: look link".to_owned()],
        ]
    );
}

#[tokio::test]
async fn an_engine_synthesis_failure_reaches_the_feed_as_a_coded_message_with_the_cause_as_detail()
{
    let (handle, mut stream, _spoken) = spawn_queue(
        "en-US",
        Some(|| TtsError::SynthesisFailed {
            id: EngineId(ENGINE.into()),
            detail: "model crashed".into(),
        }),
    );
    handle
        .send(SpeakCommand::Enqueue(request("nova", "hello")))
        .await
        .unwrap();

    let message = first_failure(&mut stream).await;

    assert_eq!(
        (
            message.code().to_string(),
            message.detail().map(|d| d.as_str().to_owned()),
            message.arg("engine").cloned(),
        ),
        (
            "speak.synthesis_failed".to_owned(),
            Some("model crashed".to_owned()),
            Some(MessageArg::Verbatim(VerbatimText::new(ENGINE))),
        )
    );
}

#[tokio::test]
async fn forcing_an_unregistered_engine_fails_with_the_engine_named_in_the_message() {
    let (handle, mut stream, _spoken) = spawn_queue("en-US", None);
    handle
        .send(SpeakCommand::Enqueue(request_with_overrides(
            "nova",
            Some("ghost"),
            Some("any-voice"),
        )))
        .await
        .unwrap();

    let message = first_failure(&mut stream).await;

    assert_eq!(
        (message.code().to_string(), message.arg("engine").cloned()),
        (
            "speak.engine_not_registered".to_owned(),
            Some(MessageArg::Verbatim(VerbatimText::new("ghost"))),
        )
    );
}
