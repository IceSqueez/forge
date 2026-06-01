use forge_audio::PcmBuffer;
use forge_tts_core::SynthesisRequest;

use crate::credentials::PollyCredentials;
use crate::polly::error::PollyError;

pub async fn synthesize(
    _creds: &PollyCredentials,
    _req: SynthesisRequest,
) -> Result<PcmBuffer, PollyError> {
    Err(PollyError::Http(
        "AWS Polly synthesis not yet implemented".into(),
    ))
}
