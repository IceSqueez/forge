#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use forge_events::{Event, EventPublisher};
use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakQueueHandle, SpeakRequest,
};
use forge_tts_core::{EngineId, TtsRegistry, TtsVoice, VoiceGender, VoiceId};
use forge_tts_pipeline::{OutputConfig, PipelineConfig};
use forge_voice::AssignmentStrategy;

use common::{FakeFactory, make_deps, recording_sink, request};

const ENGINE: &str = "poly";

/// A confidently Ukrainian and a confidently English sentence. The same viewer resolves to a
/// single voice under `DeterministicByName`, so two different answers can only come from the
/// detected language narrowing the pool.
const UKRAINIAN: &str = "добрий вечір, як ваші справи сьогодні";
const ENGLISH: &str = "good evening everyone, how is the stream going tonight";

struct RecordingPublisher {
    events: Arc<std::sync::Mutex<Vec<Event>>>,
}

impl EventPublisher for RecordingPublisher {
    fn publish(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

fn localized_voice(id: &str, locale: &str) -> TtsVoice {
    TtsVoice {
        id: VoiceId(id.into()),
        name: id.into(),
        locale: locale.into(),
        gender: VoiceGender::Neutral,
        engine_id: EngineId(ENGINE.into()),
        is_neural: false,
        sample_rate_hint: 22_050,
    }
}

fn config_with(language_aware_voice: bool) -> PipelineConfig {
    PipelineConfig {
        output: OutputConfig {
            language_aware_voice,
            ..OutputConfig::default()
        },
        ..PipelineConfig::default()
    }
}

fn spawn_bilingual_queue(
    language_aware_voice: bool,
) -> (
    SpeakQueueHandle,
    SpeakEventStream,
    Arc<std::sync::Mutex<Vec<Event>>>,
    PipelineConfigHandle,
) {
    let voices = vec![
        localized_voice("poly-en", "en-US"),
        localized_voice("poly-uk", "uk-UA"),
    ];
    let mut registry = TtsRegistry::new();
    registry.register(
        EngineId(ENGINE.into()),
        Arc::new(FakeFactory {
            id: ENGINE.into(),
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
    deps.pipeline = PipelineConfigHandle::new(config_with(language_aware_voice));
    let pipeline = deps.pipeline.clone();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    deps.event_bus = Arc::new(RecordingPublisher {
        events: Arc::clone(&events),
    });

    let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    (handle, stream, events, pipeline)
}

fn language_of_last_started(events: &Arc<std::sync::Mutex<Vec<Event>>>, text: &str) -> Event {
    events
        .lock()
        .unwrap()
        .iter()
        .rfind(|e| {
            e.kind == "speak.started"
                && e.payload["text"].as_str() == Some(text)
                && e.payload["voice_id"].is_string()
        })
        .cloned()
        .unwrap_or_else(|| panic!("no resolved speak.started for {text:?}"))
}

async fn speak_and_settle(
    handle: &SpeakQueueHandle,
    stream: &mut SpeakEventStream,
    req: SpeakRequest,
) {
    let id = req.request_id.clone();
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(5_000);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            panic!("timeout waiting for the request to settle");
        }
        match tokio::time::timeout(remaining, stream.recv()).await {
            Ok(Ok(SpeakEvent::Finished { request_id })) if request_id == id => return,
            Ok(Ok(SpeakEvent::Skipped { request_id, reason })) if request_id == id => {
                panic!("request was skipped: {reason}")
            }
            Ok(Ok(SpeakEvent::Failed { request_id, error })) if request_id == id => {
                panic!("request failed: {error}")
            }
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => panic!("stream closed"),
            Err(_) => panic!("timeout waiting for the request to settle"),
        }
    }
}

/// The detector is rebuilt off the utterance path, so the first message after start can land
/// before it is ready and legitimately reports no language. Repeat until one lands.
async fn narrowed_voice_for(
    handle: &SpeakQueueHandle,
    stream: &mut SpeakEventStream,
    events: &Arc<std::sync::Mutex<Vec<Event>>>,
    text: &str,
) -> (String, String, f64) {
    for _ in 0..5 {
        speak_and_settle(handle, stream, request("nova", text)).await;
        let found = events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "speak.started")
            .filter(|e| e.payload["text"].as_str() == Some(text))
            .find_map(|e| {
                Some((
                    e.payload["detected_language"].as_str()?.to_owned(),
                    e.payload["voice_id"].as_str()?.to_owned(),
                    e.payload["language_confidence"].as_f64()?,
                ))
            });
        if let Some(found) = found {
            return found;
        }
    }
    panic!("no speak.started for {text:?} ever reported a detected language");
}

#[tokio::test]
async fn a_detected_language_picks_the_voice_that_speaks_it_for_the_same_viewer() {
    let (handle, mut stream, events, _pipeline) = spawn_bilingual_queue(true);

    let (uk_lang, uk_voice, uk_confidence) =
        narrowed_voice_for(&handle, &mut stream, &events, UKRAINIAN).await;
    assert_eq!(uk_lang, "uk");
    assert_eq!(uk_voice, "poly-uk");
    assert!(
        uk_confidence >= 0.65,
        "a reported detection must clear the confidence floor, got {uk_confidence}"
    );

    let (en_lang, en_voice, _) = narrowed_voice_for(&handle, &mut stream, &events, ENGLISH).await;
    assert_eq!(en_lang, "en");
    assert_eq!(
        en_voice, "poly-en",
        "the same viewer must switch voices with the message language"
    );
}

#[tokio::test]
async fn toggling_the_preset_on_and_back_off_starts_and_stops_reporting_a_language() {
    let (handle, mut stream, events, pipeline) = spawn_bilingual_queue(false);

    speak_and_settle(&handle, &mut stream, request("nova", UKRAINIAN)).await;
    assert!(
        language_of_last_started(&events, UKRAINIAN).payload["detected_language"].is_null(),
        "the preset is off, so nothing may be detected"
    );

    pipeline.swap(config_with(true));
    let (lang, _, _) = narrowed_voice_for(&handle, &mut stream, &events, UKRAINIAN).await;
    assert_eq!(lang, "uk", "flipping the preset on must build a detector");

    pipeline.swap(config_with(false));
    speak_and_settle(&handle, &mut stream, request("nova", ENGLISH)).await;
    assert!(
        language_of_last_started(&events, ENGLISH).payload["detected_language"].is_null(),
        "flipping the preset back off must stop reporting a language"
    );
}
