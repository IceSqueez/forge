use forge_tts_core::{EngineId, TtsError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum EspeakError {
    #[error("espeak-ng binary not found on PATH")]
    BinaryNotFound,

    #[error("espeak-ng exited with error: {0}")]
    SubprocessFailed(String),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<EspeakError> for TtsError {
    fn from(e: EspeakError) -> Self {
        match e {
            EspeakError::BinaryNotFound => TtsError::EngineUnavailable {
                id: EngineId("espeak-ng".into()),
                detail: "espeak-ng binary not found on PATH".into(),
            },
            EspeakError::SubprocessFailed(msg) => TtsError::Io(std::io::Error::other(msg)),
            EspeakError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}
