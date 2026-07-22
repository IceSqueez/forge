#![allow(unsafe_code)]

use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};
use objc2_avf_audio::{
    AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesisVoiceQuality,
};
use objc2_foundation::NSArray;

use crate::error::NsSpeechError;

pub(crate) fn voice_catalog(engine_id: &EngineId) -> Result<Vec<TtsVoice>, NsSpeechError> {
    // SAFETY: speechVoices is documented safe to call from any thread; the returned NSArray
    // is autoreleased, so we iterate inside this autoreleasepool before it drains.
    let voices: Vec<TtsVoice> = objc2::rc::autoreleasepool(|_| {
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
    // SAFETY: not-atomic properties safe to read when the voice is not mutated concurrently;
    // returned Retained<NSString> values are valid within this autoreleasepool.
    let name = unsafe { voice.name() }.to_string();
    let identifier = unsafe { voice.identifier() }.to_string();
    let locale = unsafe { voice.language() }.to_string();
    let quality = unsafe { voice.quality() };
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
