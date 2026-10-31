use forge_components::tr;
use forge_speak_queue::SpeakQueueHandle;
use forge_speech_types::{TtsVoice, VoiceGender};
use forge_tts_core::{EngineClass, engine_display_name};

pub(crate) struct RosterEngine {
    pub id: String,
    pub name: String,
    pub class: EngineClass,
}

pub(crate) fn load_engine_roster(speak: Option<&SpeakQueueHandle>) -> Vec<RosterEngine> {
    let Some(speak) = speak else {
        return Vec::new();
    };
    speak
        .engine_roster()
        .into_iter()
        .map(|engine| RosterEngine {
            name: engine_display_name(&engine.id.0).to_owned(),
            id: engine.id.0,
            class: engine.class,
        })
        .collect()
}

pub(crate) fn engine_class_label(class: EngineClass) -> String {
    match class {
        EngineClass::Local => tr!("tts_engine_class_local"),
        EngineClass::System => tr!("tts_engine_class_system"),
        EngineClass::Cloud => tr!("tts_engine_class_cloud"),
    }
}

pub(crate) fn voice_descriptor_label(voice: &TtsVoice) -> String {
    match voice.gender {
        VoiceGender::Male => tr!("tts_voice_kind_male"),
        VoiceGender::Female => tr!("tts_voice_kind_female"),
        VoiceGender::Neutral if voice.is_neural => tr!("tts_voice_kind_neural"),
        VoiceGender::Neutral => tr!("tts_voice_kind_standard"),
    }
}
