use forge_runtime::EngineVoiceRef;
use forge_tts_core::TtsVoice;

pub(crate) fn engine_voice_options(voices: &[TtsVoice]) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = voices
        .iter()
        .map(|voice| {
            let encoded = EngineVoiceRef::new(voice.engine_id.0.clone(), voice.id.0.clone());
            let engine = voice.engine_id.display_name();
            let label = if voice.locale.is_empty() {
                format!("{engine} - {}", voice.name)
            } else {
                format!("{engine} - {} ({})", voice.name, voice.locale)
            };
            (encoded.to_string(), label)
        })
        .collect();
    options.sort_by(|a, b| a.1.cmp(&b.1));
    options
}
