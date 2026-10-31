use forge_speech_types::{EngineId, VoiceId};
use forge_tts_core::TtsError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CloudTtsError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("quota exhausted: {0}")]
    QuotaExceeded(String),
    #[error("rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("voice not found: {0:?}")]
    VoiceNotFound(VoiceId),
    #[error("response could not be decoded: {0}")]
    Decode(String),
    #[error("request rejected: {0}")]
    Rejected(String),
    #[error("invalid endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("request could not be signed: {0}")]
    Signing(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

impl CloudTtsError {
    pub(crate) fn into_tts(self, engine: &str) -> TtsError {
        let id = EngineId(engine.into());
        match self {
            Self::Http(message) => TtsError::NetworkFailed(message),
            Self::Unauthorized(reason) | Self::Signing(reason) => TtsError::AuthFailed { reason },
            Self::QuotaExceeded(detail) => TtsError::QuotaExceeded { id, detail },
            Self::RateLimited { retry_after_secs } => TtsError::RateLimited { retry_after_secs },
            Self::VoiceNotFound(id) => TtsError::InvalidVoice { id },
            Self::Rejected(detail) | Self::Decode(detail) => {
                TtsError::SynthesisFailed { id, detail }
            }
            Self::InvalidEndpoint(detail) => TtsError::EngineUnavailable { id, detail },
            Self::Io(source) => TtsError::Io(source),
        }
    }
}
