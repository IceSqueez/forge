use forge_audio::PcmBuffer;
use forge_tts_core::SynthesisRequest;

use crate::credentials::OpenAiCredentials;
use crate::openai::error::OpenAiError;

pub async fn synthesize(
    _creds: &OpenAiCredentials,
    _req: SynthesisRequest,
) -> Result<PcmBuffer, OpenAiError> {
    Err(OpenAiError::Http(
        "OpenAI TTS synthesis not yet implemented".into(),
    ))
}
