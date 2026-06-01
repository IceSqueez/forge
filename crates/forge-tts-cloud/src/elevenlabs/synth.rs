use forge_audio::PcmBuffer;
use forge_tts_core::SynthesisRequest;

use crate::credentials::ElevenLabsCredentials;
use crate::elevenlabs::error::ElevenLabsError;

pub async fn synthesize(
    _creds: &ElevenLabsCredentials,
    _req: SynthesisRequest,
) -> Result<PcmBuffer, ElevenLabsError> {
    Err(ElevenLabsError::Http(
        "ElevenLabs synthesis not yet implemented".into(),
    ))
}
