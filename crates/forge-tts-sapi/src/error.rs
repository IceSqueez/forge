use forge_tts_core::{ENGINE_ID_SAPI, EngineId, TtsError, VoiceId};

#[derive(Debug, thiserror::Error)]
pub(crate) enum SapiError {
    #[error("COM apartment initialization failed: HRESULT 0x{0:08x}")]
    ComInit(i32),

    #[error("voice catalog call {call} failed: HRESULT 0x{hr:08x}")]
    Catalog { call: &'static str, hr: i32 },

    #[error("voice token string from {call} is not valid UTF-16")]
    InvalidTokenString { call: &'static str },

    #[error("no SAPI 5 voices found")]
    NoCatalog,

    #[error("voice {0:?} is not installed")]
    VoiceNotFound(VoiceId),

    #[error("synthesis call {call} failed: HRESULT 0x{hr:08x}")]
    Synthesis { call: &'static str, hr: i32 },

    #[error("STA worker terminated")]
    WorkerTerminated,

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl SapiError {
    pub(crate) fn catalog_call(call: &'static str) -> impl FnOnce(windows::core::Error) -> Self {
        move |error| Self::Catalog {
            call,
            hr: error.code().0,
        }
    }

    pub(crate) fn synthesis_call(call: &'static str) -> impl FnOnce(windows::core::Error) -> Self {
        move |error| Self::Synthesis {
            call,
            hr: error.code().0,
        }
    }
}

impl From<SapiError> for TtsError {
    fn from(error: SapiError) -> Self {
        let id = EngineId(ENGINE_ID_SAPI.into());
        match error {
            SapiError::VoiceNotFound(voice_id) => TtsError::InvalidVoice { id: voice_id },
            SapiError::Synthesis { .. } => TtsError::SynthesisFailed {
                id,
                detail: error.to_string(),
            },
            SapiError::Io(io_error) => TtsError::Io(io_error),
            SapiError::ComInit(_)
            | SapiError::Catalog { .. }
            | SapiError::InvalidTokenString { .. }
            | SapiError::NoCatalog
            | SapiError::WorkerTerminated => TtsError::EngineUnavailable {
                id,
                detail: error.to_string(),
            },
        }
    }
}
