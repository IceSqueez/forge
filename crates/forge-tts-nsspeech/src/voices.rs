#![expect(unsafe_code, reason = "reads the AppKit voice catalog")]

use forge_speech_types::{EngineId, TtsVoice, VoiceGender, VoiceId};
use objc2_avf_audio::{
    AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesisVoiceQuality,
};
use objc2_foundation::NSArray;

use crate::error::NsSpeechError;

pub(crate) fn voice_catalog(engine_id: &EngineId) -> Result<Vec<TtsVoice>, NsSpeechError> {
    let voices: Vec<TtsVoice> = objc2::rc::autoreleasepool(|_| {
        // SAFETY: speechVoices takes no arguments and returns a retained NSArray owned here.
        let raw: objc2::rc::Retained<NSArray<AVSpeechSynthesisVoice>> =
            unsafe { AVSpeechSynthesisVoice::speechVoices() };
        raw.iter().map(|v| map_voice(&v, engine_id)).collect()
    });

    if voices.is_empty() {
        return Err(NsSpeechError::NoCatalog);
    }
    Ok(voices)
}

fn map_voice(voice: &AVSpeechSynthesisVoice, engine_id: &EngineId) -> TtsVoice {
    // SAFETY: `voice` is a live immutable catalog entry and name is a plain property read.
    let name = unsafe { voice.name() }.to_string();
    // SAFETY: `voice` is a live immutable catalog entry and identifier is a plain property read.
    let identifier = unsafe { voice.identifier() }.to_string();
    // SAFETY: `voice` is a live immutable catalog entry and language is a plain property read.
    let locale = unsafe { voice.language() }.to_string();
    // SAFETY: `voice` is a live immutable catalog entry and quality is a plain property read.
    let quality = unsafe { voice.quality() };
    // SAFETY: `voice` is a live immutable catalog entry and gender is a plain property read.
    let gender = unsafe { voice.gender() };

    TtsVoice {
        id: VoiceId(identifier),
        name,
        locale,
        gender: map_gender(gender),
        engine_id: engine_id.clone(),
        is_neural: quality == AVSpeechSynthesisVoiceQuality::Enhanced
            || quality == AVSpeechSynthesisVoiceQuality::Premium,
        sample_rate_hint: 22_050,
    }
}

fn map_gender(g: AVSpeechSynthesisVoiceGender) -> VoiceGender {
    match g {
        AVSpeechSynthesisVoiceGender::Male => VoiceGender::Male,
        AVSpeechSynthesisVoiceGender::Female => VoiceGender::Female,
        _ => VoiceGender::Neutral,
    }
}
