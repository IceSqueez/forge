use forge_audio::PcmBuffer;
use forge_tts_core::SynthesisRequest;

use crate::azure::error::AzureError;
use crate::credentials::AzureCredentials;

pub async fn synthesize(
    _creds: &AzureCredentials,
    _req: SynthesisRequest,
) -> Result<PcmBuffer, AzureError> {
    Err(AzureError::Http(
        "Azure TTS synthesis not yet implemented".into(),
    ))
}
