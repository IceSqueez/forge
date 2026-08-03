#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use forge_audio::PcmBuffer;
use forge_speak_queue::{QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream};
use forge_tts_core::{
    EngineCapabilities, EngineId, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
    TtsRegistry, TtsVoice,
};
use forge_tts_pipeline::{OutputConfig, PipelineConfig};
use forge_voice::AssignmentStrategy;

use common::{make_deps, recording_sink, request, voice};

const SAMPLE_RATE: u32 = 22_050;
const CLIP_SECS: u32 = 5;

/// Synthesises a clip long enough that a cap below `CLIP_SECS` changes the reported duration -
/// the shared fake engine's 8-sample buffer rounds to 0 s and hides any truncation.
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
        static CAPS: EngineCapabilities = EngineCapabilities {
            ssml: false,
            neural_voices: false,
            streaming: false,
            custom_lexicons: false,
        };
        &CAPS
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
            ..OutputConfig::default()
        },
        ..PipelineConfig::default()
    });

    forge_speak_queue::spawn(QueueConfig::default(), deps)
}

/// The actor emits `Started` twice per request; only the second one - the one carrying a
/// resolved voice - reports a synthesised duration.
async fn resolved_duration_secs(stream: &mut SpeakEventStream, max_ms: u64) -> u32 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(max_ms);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timeout waiting for a resolved Started event");
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Started {
                voice_id,
                duration_secs,
                ..
            })) if !voice_id.0.is_empty() => return duration_secs,
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for a resolved Started event"),
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

        let secs = resolved_duration_secs(&mut stream, 5_000).await;
        assert_eq!(
            secs, expected,
            "cap {cap:?} applied to a {CLIP_SECS}s clip must be reflected in the Started duration"
        );
    }
}
