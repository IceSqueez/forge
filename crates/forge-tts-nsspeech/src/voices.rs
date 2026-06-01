use forge_tts_core::{EngineId, TtsVoice, VoiceGender, VoiceId};
use objc2_avf_audio::{
    AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceGender, AVSpeechSynthesisVoiceQuality,
};
use objc2_foundation::NSArray;

use crate::error::NsSpeechError;

pub(crate) fn voice_catalog(engine_id: &EngineId) -> Result<Vec<TtsVoice>, NsSpeechError> {
    // SAFETY: speechVoices is a class method on AVSpeechSynthesisVoice; Apple documents it as
    // safe to call from any thread — it reads a static OS-maintained voice registry. The returned
    // NSArray is autoreleased; we iterate inside the autoreleasepool block before it drains.
    let voices: Vec<TtsVoice> = objc2::rc::autoreleasepool(|_| {
        let raw: objc2::rc::Retained<NSArray<AVSpeechSynthesisVoice>> =
            unsafe { AVSpeechSynthesisVoice::speechVoices() };
        raw.iter().map(|v| map_voice(v, engine_id)).collect()
    });

    if voices.is_empty() {
        return Err(NsSpeechError::NoCatalog);
    }
    Ok(voices)
}

fn map_voice(voice: &AVSpeechSynthesisVoice, engine_id: &EngineId) -> TtsVoice {
    // SAFETY: name(), identifier(), language(), quality(), gender() are all properties on
    // AVSpeechSynthesisVoice. Apple marks them "not atomic" but they are safe to call from
    // any thread when the voice object is not being mutated concurrently. We are inside an
    // autoreleasepool so the returned Retained<NSString> is valid for this scope.
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
