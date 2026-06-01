use forge_tts_core::TtsVoice;

use crate::azure::error::AzureError;
use crate::credentials::AzureCredentials;

pub async fn fetch_voices(_creds: &AzureCredentials) -> Result<Vec<TtsVoice>, AzureError> {
    Err(AzureError::Http(
        "Azure TTS voice listing not yet implemented".into(),
    ))
}
