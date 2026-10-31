#![allow(clippy::panic)]
mod common;

use std::sync::Arc;

use async_trait::async_trait;
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream};
use forge_speech_types::AssignmentStrategy;
use forge_speech_types::PcmBuffer;
use forge_speech_types::{EngineId, TtsVoice};
use forge_tts_core::{
    EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry,
};
use forge_tts_pipeline::{OutputConfig, PipelineConfig};

use common::{make_deps, next_event, recording_sink, request, voice};

const SAMPLE_RATE: u32 = 22_050;
const CLIP_SECS: u32 = 5;

struct FixedLengthEngine {
    id: EngineId,
    voices: Vec<TtsVoice>,
}

#[async_trait]
impl TtsEngine for FixedLengthEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &common::PLAIN_CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(self.voices.clone())
    }

    async fn synthesize(&self, _req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        let samples = (CLIP_SECS * SAMPLE_RATE) as usize;
        Ok(PcmBuffer::new(vec![0i16; samples], SAMPLE_RATE, 1))
    }
}

struct FixedLengthFactory {
    id: EngineId,
    voices: Vec<TtsVoice>,
}

impl TtsEngineFactory for FixedLengthFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(FixedLengthEngine {
            id: self.id.clone(),
            voices: self.voices.clone(),
        }))
    }
}

fn spawn_with_cap(
    max_duration_secs: Option<u32>,
) -> (forge_speak_queue::SpeakQueueHandle, SpeakEventStream) {
    let engine_id = EngineId("fixed".into());
    let voices = vec![voice("fixed-1", "fixed")];
    let mut registry = TtsRegistry::new();
    registry.register(
        engine_id.clone(),
        Arc::new(FixedLengthFactory {
            id: engine_id,
            voices,
        }),
    );

    let (sink, _plays) = recording_sink();
    let mut deps = make_deps(
        registry,
        sink,
        AssignmentStrategy::DeterministicByName,
        vec![],
    );
    deps.pipeline = forge_speak_queue::PipelineConfigHandle::new(PipelineConfig {
        output: OutputConfig {
            max_duration_secs,
            ..common::neutral_output()
        },
        ..common::neutral_pipeline()
    });

    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

async fn resolved_duration_secs(stream: &mut SpeakEventStream) -> u32 {
    loop {
        if let SpeakEvent::Started {
            voice_id: Some(_),
            duration_secs,
            ..
        } = next_event(stream).await
        {
            return duration_secs;
        }
    }
}

#[tokio::test]
async fn started_reports_the_capped_duration_not_the_synthesised_one() {
    for (cap, expected) in [
        (None, CLIP_SECS),
        (Some(2), 2),
        (Some(CLIP_SECS), CLIP_SECS),
        (Some(CLIP_SECS + 1), CLIP_SECS),
    ] {
        let (handle, mut stream) = spawn_with_cap(cap);
        handle
            .send(SpeakCommand::Enqueue(request("nova", "cap me")))
            .await
            .unwrap();

        let secs = resolved_duration_secs(&mut stream).await;
        assert_eq!(
            secs, expected,
            "cap {cap:?} applied to a {CLIP_SECS}s clip must be reflected in the Started duration"
        );
    }
}
