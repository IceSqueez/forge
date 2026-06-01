use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};

const ENGINE: fn() -> EngineId = || EngineId("openai".into());

pub fn static_voices() -> Vec<TtsVoice> {
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
        sample_rate_hint: 24_000,
    })
    .collect()
}
