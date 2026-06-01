use forge_tts_core::TtsVoice;

use crate::credentials::ElevenLabsCredentials;
use crate::elevenlabs::error::ElevenLabsError;

pub async fn fetch_voices(
    _creds: &ElevenLabsCredentials,
) -> Result<Vec<TtsVoice>, ElevenLabsError> {
    Err(ElevenLabsError::Http(
        "ElevenLabs voice listing not yet implemented".into(),
    ))
}
