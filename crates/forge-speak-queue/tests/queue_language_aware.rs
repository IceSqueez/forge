#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_events::Event;
use forge_speak_queue::{
    PipelineConfigHandle, QueueConfig, SpeakCommand, SpeakEvent, SpeakEventStream,
    SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::AssignmentStrategy;
use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::TtsRegistry;
use forge_tts_pipeline::{OutputConfig, PipelineConfig};

use common::{
    FakeFactory, RecordingPublisher, make_deps, outcome_of, recording_bus, recording_sink, request,
};

const ENGINE: &str = "poly";
const SPEAK_STARTED: &str = "speak.started";
const REPORTED_CONFIDENCE_FLOOR: f64 = 0.65;
const DETECTOR_BUILD_HANG_GUARD: Duration = Duration::from_secs(120);
const DELAY_BETWEEN_PROBES: Duration = Duration::from_millis(20);

const UKRAINIAN: &str = "добрий вечір, як ваші справи сьогодні";
const ENGLISH: &str = "good evening everyone, how is the stream going tonight";

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
            ..common::neutral_output()
        },
        ..common::neutral_pipeline()
    }
}

fn spawn_bilingual_queue(
    language_aware_voice: bool,
) -> (
    SpeakQueueHandle,
    SpeakEventStream,
    Arc<RecordingPublisher>,
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
    let (deps, bus) = recording_bus(deps);

    let (handle, stream) = forge_speak_queue::spawn(QueueConfig::default(), deps);
    (handle, stream, bus, pipeline)
}

fn language_of_last_started(bus: &RecordingPublisher, text: &str) -> Event {
    bus.of_kind(SPEAK_STARTED)
        .into_iter()
        .rfind(|e| e.payload["text"].as_str() == Some(text) && e.payload["voice_id"].is_string())
        .unwrap_or_else(|| panic!("no resolved speak.started for {text:?}"))
}

async fn speak_and_settle(
    handle: &SpeakQueueHandle,
    stream: &mut SpeakEventStream,
    req: SpeakRequest,
) {
    let id = req.request_id.clone();
    handle.send(SpeakCommand::Enqueue(req)).await.unwrap();
    let outcome = outcome_of(stream, &id).await;
    assert!(
        matches!(outcome, SpeakEvent::Finished { .. }),
        "the request did not finish: {outcome:?}"
    );
}

async fn narrowed_voice_for(
    handle: &SpeakQueueHandle,
    stream: &mut SpeakEventStream,
    bus: &RecordingPublisher,
    text: &str,
) -> (String, String, f64) {
    let hang_guard = Instant::now() + DETECTOR_BUILD_HANG_GUARD;
    loop {
        speak_and_settle(handle, stream, request("nova", text)).await;
        let reported = bus
            .of_kind(SPEAK_STARTED)
            .iter()
            .filter(|e| e.payload["text"].as_str() == Some(text))
            .find_map(|e| {
                Some((
                    e.payload["detected_language"].as_str()?.to_owned(),
                    e.payload["voice_id"].as_str()?.to_owned(),
                    e.payload["language_confidence"].as_f64()?,
                ))
            });
        if let Some(reported) = reported {
            return reported;
        }
        assert!(
            Instant::now() < hang_guard,
            "no speak.started for {text:?} ever reported a detected language"
        );
        tokio::time::sleep(DELAY_BETWEEN_PROBES).await;
    }
}

#[tokio::test]
async fn a_detected_language_picks_the_voice_that_speaks_it_for_the_same_viewer() {
    let (handle, mut stream, bus, _pipeline) = spawn_bilingual_queue(true);

    let (uk_lang, uk_voice, uk_confidence) =
        narrowed_voice_for(&handle, &mut stream, &bus, UKRAINIAN).await;
    assert_eq!(uk_lang, "uk");
    assert_eq!(uk_voice, "poly-uk");
    assert!(
        uk_confidence >= REPORTED_CONFIDENCE_FLOOR,
        "a reported detection must clear the confidence floor, got {uk_confidence}"
    );

    let (en_lang, en_voice, _) = narrowed_voice_for(&handle, &mut stream, &bus, ENGLISH).await;
    assert_eq!(en_lang, "en");
    assert_eq!(
        en_voice, "poly-en",
        "the same viewer must switch voices with the message language"
    );
}

#[tokio::test]
async fn toggling_the_preset_on_and_back_off_starts_and_stops_reporting_a_language() {
    let (handle, mut stream, bus, pipeline) = spawn_bilingual_queue(false);

    speak_and_settle(&handle, &mut stream, request("nova", UKRAINIAN)).await;
    assert!(
        language_of_last_started(&bus, UKRAINIAN).payload["detected_language"].is_null(),
        "the preset is off, so nothing may be detected"
    );

    pipeline.swap(config_with(true));
    let (lang, _, _) = narrowed_voice_for(&handle, &mut stream, &bus, UKRAINIAN).await;
    assert_eq!(lang, "uk", "flipping the preset on must build a detector");

    pipeline.swap(config_with(false));
    speak_and_settle(&handle, &mut stream, request("nova", ENGLISH)).await;
    assert!(
        language_of_last_started(&bus, ENGLISH).payload["detected_language"].is_null(),
        "flipping the preset back off must stop reporting a language"
    );
}
