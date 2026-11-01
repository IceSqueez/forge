use forge_speech_types::EngineId;
use forge_tts_avspeech::AvSpeechEngineFactory;
use forge_tts_core::TtsEngineFactory;

#[test]
fn factory_keeps_the_id_that_saved_voice_aliases_reference() {
    assert_eq!(
        AvSpeechEngineFactory.engine_id(),
        EngineId("nsspeech".into())
    );
}
