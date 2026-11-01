#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_audio::{AudioError, AudioSink, ControlledPlayback, PlaybackControl};
use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, RequestId, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::{AssignmentStrategy, EngineId, PcmBuffer, TtsVoice, VoiceId};
use forge_tts_core::{SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError, TtsRegistry};
use forge_tts_pipeline::{BlocklistMode, PipelineConfig};

use common::{alias, make_deps, neutral_pipeline, next_matching, outcome_of, request, voice};

const ENGINE: &str = "recording";
const VOICE: &str = "recording-1";
const HANGING_TEXT: &str = "this synthesis never returns";
const SAMPLE_RATE: u32 = 22_050;
const CLIP_SECS: u32 = 5;
const CLIP_SAMPLES: usize = (SAMPLE_RATE * CLIP_SECS) as usize;
const TEST_TEXT: &str = "Hello @viewer, check https://example.com 25 times. Blocked word.";

type Log<T> = Arc<Mutex<Vec<T>>>;

struct RecordingEngine {
    id: EngineId,
    spoken: Log<String>,
}

#[async_trait]
impl TtsEngine for RecordingEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        Ok(vec![voice(VOICE, ENGINE)])
    }

    async fn synthesize(&self, req: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if req.text.as_str() == HANGING_TEXT {
            std::future::pending::<()>().await;
        }
        self.spoken
            .lock()
            .unwrap()
            .push(req.text.as_str().to_owned());
        Ok(PcmBuffer::new(vec![0i16; CLIP_SAMPLES], SAMPLE_RATE, 1))
    }
}

struct RecordingFactory {
    spoken: Log<String>,
}

impl TtsEngineFactory for RecordingFactory {
    fn engine_id(&self) -> EngineId {
        EngineId(ENGINE.into())
    }

    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        Ok(Box::new(RecordingEngine {
            id: EngineId(ENGINE.into()),
            spoken: Arc::clone(&self.spoken),
        }))
    }
}

struct MeasuringSink {
    played: Log<usize>,
}

#[async_trait]
impl AudioSink for MeasuringSink {
    async fn start(&self, buf: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.played.lock().unwrap().push(buf.samples.len());
        Ok(ControlledPlayback::new(
            PlaybackControl::new(),
            std::future::ready(Ok(())),
        ))
    }
}

struct Rig {
    handle: SpeakQueueHandle,
    stream: SpeakEventStream,
    spoken: Log<String>,
    played: Log<usize>,
}

impl Rig {
    fn over(pipeline: PipelineConfig) -> Self {
        let spoken = Log::default();
        let played = Log::default();
        let mut registry = TtsRegistry::new();
        registry.register(Arc::new(RecordingFactory {
            spoken: Arc::clone(&spoken),
        }));
        let sink: Arc<dyn AudioSink> = Arc::new(MeasuringSink {
            played: Arc::clone(&played),
        });
        let mut deps = make_deps(
            registry,
            sink,
            AssignmentStrategy::DeterministicByName,
            vec![alias("known", ENGINE, VOICE)],
        );
        deps.pipeline = PipelineConfigHandle::new(pipeline);
        let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
        Self {
            handle,
            stream,
            spoken,
            played,
        }
    }

    async fn command(&self, command: SpeakCommand) {
        self.handle.send(command).await.unwrap();
    }

    async fn enqueue(&self, request: SpeakRequest) -> RequestId {
        let id = request.request_id.clone();
        self.command(SpeakCommand::Enqueue(request)).await;
        id
    }

    async fn preview(&self, text: &str) -> RequestId {
        let request = preview_request(text);
        let id = request.request_id.clone();
        self.command(SpeakCommand::Preview(request)).await;
        id
    }

    async fn outcome(&mut self, id: &RequestId) -> SpeakEvent {
        outcome_of(&mut self.stream, id).await
    }

    async fn started(&mut self, id: &RequestId) {
        next_matching(
            &mut self.stream,
            |event| matches!(event, SpeakEvent::Started { request_id, .. } if request_id == id),
        )
        .await;
    }

    fn spoken(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }
}

fn preview_request(text: &str) -> SpeakRequest {
    SpeakRequest {
        engine_override: Some(EngineId(ENGINE.into())),
        voice_override: Some(VoiceId(VOICE.into())),
        ..request("", text)
    }
}

fn hostile_pipeline() -> PipelineConfig {
    let mut pipeline = neutral_pipeline();
    pipeline.skip_rules.max_chars = Some(3);
    pipeline.skip_rules.repeat_of_recent = true;
    pipeline.skip_rules.contains_url = true;
    pipeline.word_blocklist = vec!["blocked".to_owned()];
    pipeline.blocklist_mode = BlocklistMode::SkipMessage;
    pipeline.output.read_display_name_first = true;
    pipeline.output.sanitize_punctuation = true;
    pipeline.output.max_duration_secs = Some(1);
    pipeline
}

fn is_finished(event: &SpeakEvent) -> bool {
    matches!(event, SpeakEvent::Finished { .. })
}

fn is_skipped(event: &SpeakEvent) -> bool {
    matches!(event, SpeakEvent::Skipped { .. })
}

#[tokio::test]
async fn a_preview_reads_the_exact_text_at_full_length_past_every_filter() {
    let mut rig = Rig::over(hostile_pipeline());

    for _ in 0..3 {
        let id = rig.preview(TEST_TEXT).await;
        let outcome = rig.outcome(&id).await;
        assert!(is_finished(&outcome), "{outcome:?}");
    }

    assert_eq!(rig.spoken(), vec![TEST_TEXT; 3]);
    assert_eq!(*rig.played.lock().unwrap(), vec![CLIP_SAMPLES; 3]);
}

#[tokio::test]
async fn the_same_text_from_a_viewer_stays_filtered() {
    let mut rig = Rig::over(hostile_pipeline());

    let id = rig.enqueue(request("viewer", TEST_TEXT)).await;
    let outcome = rig.outcome(&id).await;

    assert!(is_skipped(&outcome), "{outcome:?}");
    assert!(rig.spoken().is_empty());
}

#[tokio::test]
async fn an_alias_preview_reads_verbatim_in_the_alias_voice() {
    let mut rig = Rig::over(hostile_pipeline());
    let request = request("known", TEST_TEXT);
    let id = request.request_id.clone();

    rig.command(SpeakCommand::Preview(request)).await;
    let outcome = rig.outcome(&id).await;

    assert!(is_finished(&outcome), "{outcome:?}");
    assert_eq!(rig.spoken(), vec![TEST_TEXT]);
}

#[tokio::test]
async fn a_preview_stays_out_of_the_repeat_window() {
    let mut pipeline = neutral_pipeline();
    pipeline.skip_rules.repeat_of_recent = true;
    let mut rig = Rig::over(pipeline);

    let preview = rig.preview("same words").await;
    rig.outcome(&preview).await;
    let viewer = rig.enqueue(request("viewer", "same words")).await;
    let outcome = rig.outcome(&viewer).await;

    assert!(is_finished(&outcome), "{outcome:?}");
    assert_eq!(rig.spoken(), vec!["same words", "same words"]);
}

#[tokio::test]
async fn replay_repeats_the_last_viewer_message_not_the_preview_after_it() {
    let mut rig = Rig::over(neutral_pipeline());

    let viewer = rig.enqueue(request("viewer", "from a viewer")).await;
    rig.outcome(&viewer).await;
    let preview = rig.preview("a preview").await;
    rig.outcome(&preview).await;
    rig.command(SpeakCommand::Replay).await;
    next_matching(&mut rig.stream, is_finished).await;

    assert_eq!(
        rig.spoken(),
        vec!["from a viewer", "a preview", "from a viewer"]
    );
}

#[tokio::test]
async fn a_second_preview_replaces_one_still_being_read() {
    let mut rig = Rig::over(neutral_pipeline());

    let replaced = rig.preview(HANGING_TEXT).await;
    rig.started(&replaced).await;
    let replacement = rig.preview("second voice").await;

    let outcome = rig.outcome(&replaced).await;
    assert!(is_skipped(&outcome), "{outcome:?}");
    let outcome = rig.outcome(&replacement).await;
    assert!(is_finished(&outcome), "{outcome:?}");
    assert_eq!(rig.spoken(), vec!["second voice"]);
}

#[tokio::test]
async fn previews_wait_for_the_viewer_on_air_then_only_the_last_one_plays_first() {
    let mut rig = Rig::over(neutral_pipeline());

    let on_air = rig.enqueue(request("viewer", HANGING_TEXT)).await;
    rig.started(&on_air).await;
    let waiting = rig.enqueue(request("viewer", "next viewer")).await;
    rig.preview("first voice").await;
    let last = rig.preview("last voice").await;
    rig.command(SpeakCommand::Skip).await;

    let outcome = rig.outcome(&last).await;
    assert!(is_finished(&outcome), "{outcome:?}");
    rig.outcome(&waiting).await;
    assert_eq!(rig.spoken(), vec!["last voice", "next viewer"]);
}

#[tokio::test]
async fn a_preview_plays_through_a_hold_and_held_viewers_stay_held() {
    for (hold, release) in [
        (SpeakCommand::Pause, SpeakCommand::Resume),
        (
            SpeakCommand::VoiceGateActivated,
            SpeakCommand::VoiceGateDeactivated,
        ),
    ] {
        let mut rig = Rig::over(neutral_pipeline());
        rig.command(hold).await;
        let held = rig.enqueue(request("viewer", "held viewer")).await;

        let preview = rig.preview("through the hold").await;
        let outcome = rig.outcome(&preview).await;

        assert!(is_finished(&outcome), "{outcome:?}");
        assert_eq!(rig.spoken(), vec!["through the hold"]);
        rig.command(release).await;
        rig.outcome(&held).await;
        assert_eq!(rig.spoken(), vec!["through the hold", "held viewer"]);
    }
}
