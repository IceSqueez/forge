use forge_tts_core::TtsVoice;

use crate::credentials::PollyCredentials;
use crate::polly::error::PollyError;

pub async fn fetch_voices(_creds: &PollyCredentials) -> Result<Vec<TtsVoice>, PollyError> {
    Err(PollyError::Http(
        "AWS Polly voice listing not yet implemented".into(),
    ))
}
