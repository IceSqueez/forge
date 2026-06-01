use forge_tts_core::{EngineId, TtsError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum NsSpeechError {
    #[error("no AVSpeech voices installed")]
    NoCatalog,

    #[error("AVFoundation worker terminated")]
    WorkerTerminated,

    #[error("synthesis timed out after 30s without audio")]
    Timeout,

    #[error("AVFoundation synth produced no audio")]
    NoAudio,

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<NsSpeechError> for TtsError {
    fn from(e: NsSpeechError) -> Self {
        match e {
            NsSpeechError::NoCatalog => TtsError::EngineUnavailable {
                id: EngineId("nsspeech".into()),
                detail: "no AVSpeech voices installed".into(),
            },
            NsSpeechError::WorkerTerminated => TtsError::EngineUnavailable {
                id: EngineId("nsspeech".into()),
                detail: "AVFoundation worker terminated".into(),
            },
            NsSpeechError::Timeout | NsSpeechError::NoAudio => {
                TtsError::Io(std::io::Error::other(e.to_string()))
            }
            NsSpeechError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}
