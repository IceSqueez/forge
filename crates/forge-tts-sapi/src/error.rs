use forge_tts_core::{EngineId, TtsError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum SapiError {
    #[error("CoInitializeEx failed: HRESULT 0x{0:08x}")]
    ComInit(i32),

    #[allow(dead_code)]
    #[error("voice catalog is empty")]
    NoCatalog,

    #[error("SAPI Speak failed: HRESULT 0x{0:08x}")]
    Speak(i32),

    #[error("STA worker terminated")]
    WorkerTerminated,

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<SapiError> for TtsError {
    fn from(e: SapiError) -> Self {
        match e {
            SapiError::ComInit(hr) => TtsError::EngineUnavailable {
                id: EngineId("sapi".into()),
                detail: format!("CoInitializeEx 0x{hr:08x}"),
            },
            SapiError::NoCatalog => TtsError::EngineUnavailable {
                id: EngineId("sapi".into()),
                detail: "no SAPI 5 voices found".into(),
            },
            SapiError::Speak(hr) => {
                TtsError::Io(std::io::Error::other(format!("SAPI HRESULT 0x{hr:08x}")))
            }
            SapiError::WorkerTerminated => TtsError::EngineUnavailable {
                id: EngineId("sapi".into()),
                detail: "STA worker terminated".into(),
            },
            SapiError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}
