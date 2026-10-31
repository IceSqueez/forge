use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::ENGINE_ID_OPENAI;

const OUTPUT_SAMPLE_RATE_HZ: u32 = 24_000;
const ENGINE: fn() -> EngineId = || EngineId(ENGINE_ID_OPENAI.into());

pub(super) fn static_voices() -> Vec<TtsVoice> {
    [
        ("alloy", "Alloy", "en-US", VoiceGender::Neutral),
        ("echo", "Echo", "en-US", VoiceGender::Male),
        ("fable", "Fable", "en-US", VoiceGender::Neutral),
        ("onyx", "Onyx", "en-US", VoiceGender::Male),
        ("nova", "Nova", "en-US", VoiceGender::Female),
        ("shimmer", "Shimmer", "en-US", VoiceGender::Female),
    ]
    .into_iter()
    .map(|(id, name, locale, gender)| TtsVoice {
        id: VoiceId(id.into()),
        name: name.into(),
        locale: locale.into(),
        gender,
        engine_id: ENGINE(),
        is_neural: true,
        sample_rate_hint: OUTPUT_SAMPLE_RATE_HZ,
    })
    .collect()
}
