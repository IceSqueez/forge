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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Kind {
        EngineUnavailable,
        InvalidVoice,
        SynthesisFailed,
        Io,
        Unexpected,
    }

    fn kind_and_detail(error: TtsError) -> (Kind, String) {
        match error {
            TtsError::EngineUnavailable { id, detail } if id.0 == ENGINE_ID_SAPI => {
                (Kind::EngineUnavailable, detail)
            }
            TtsError::InvalidVoice { id } => (Kind::InvalidVoice, id.0),
            TtsError::SynthesisFailed { id, detail } if id.0 == ENGINE_ID_SAPI => {
                (Kind::SynthesisFailed, detail)
            }
            TtsError::Io(io_error) => (Kind::Io, io_error.to_string()),
            other => (Kind::Unexpected, other.to_string()),
        }
    }

    #[test]
    fn sapi_errors_map_to_the_matching_tts_error_keeping_call_context() {
        for (error, expected_kind, expected_detail) in [
            (
                SapiError::VoiceNotFound(VoiceId("TTS_MS_EN-US_ZIRA".into())),
                Kind::InvalidVoice,
                "TTS_MS_EN-US_ZIRA",
            ),
            (
                SapiError::Synthesis {
                    call: "ISpVoice::Speak",
                    hr: 0x8004_5003_u32 as i32,
                },
                Kind::SynthesisFailed,
                "ISpVoice::Speak",
            ),
            (
                SapiError::Catalog {
                    call: "SpEnumTokens",
                    hr: 0x8004_5039_u32 as i32,
                },
                Kind::EngineUnavailable,
                "SpEnumTokens",
            ),
            (
                SapiError::InvalidTokenString {
                    call: "ISpObjectToken::GetId",
                },
                Kind::EngineUnavailable,
                "ISpObjectToken::GetId",
            ),
            (
                SapiError::ComInit(0x8001_0106_u32 as i32),
                Kind::EngineUnavailable,
                "80010106",
            ),
            (SapiError::NoCatalog, Kind::EngineUnavailable, "voices"),
            (
                SapiError::WorkerTerminated,
                Kind::EngineUnavailable,
                "worker",
            ),
            (
                SapiError::Io(std::io::Error::other("pipe closed")),
                Kind::Io,
                "pipe closed",
            ),
        ] {
            let (kind, detail) = kind_and_detail(TtsError::from(error));
            assert_eq!(kind, expected_kind, "detail={detail:?}");
            assert!(
                detail.contains(expected_detail),
                "{detail:?} lacks {expected_detail:?}"
            );
        }
    }
}
