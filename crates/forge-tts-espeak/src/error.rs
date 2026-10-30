use forge_speech_types::EngineId;
use forge_tts_core::{ENGINE_ID_ESPEAK_NG, TtsError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum EspeakError {
    #[error("espeak-ng binary not found on PATH")]
    BinaryNotFound,

    #[error("espeak-ng exited with error: {0}")]
    SubprocessFailed(String),

    #[error("espeak-ng could not list voices: {0}")]
    VoiceListingFailed(String),

    #[error("espeak-ng produced audio that is not 16-bit PCM WAV")]
    UnsupportedAudio,

    #[error("espeak-ng did not finish within {ms} ms")]
    Timeout { ms: u64 },

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<EspeakError> for TtsError {
    fn from(e: EspeakError) -> Self {
        match e {
            EspeakError::BinaryNotFound => TtsError::EngineUnavailable {
                id: EngineId(ENGINE_ID_ESPEAK_NG.into()),
                detail: "espeak-ng binary not found on PATH".into(),
            },
            EspeakError::SubprocessFailed(detail) => TtsError::SynthesisFailed {
                id: EngineId(ENGINE_ID_ESPEAK_NG.into()),
                detail,
            },
            EspeakError::VoiceListingFailed(detail) => TtsError::EngineUnavailable {
                id: EngineId(ENGINE_ID_ESPEAK_NG.into()),
                detail,
            },
            EspeakError::UnsupportedAudio => TtsError::SynthesisFailed {
                id: EngineId(ENGINE_ID_ESPEAK_NG.into()),
                detail: "espeak-ng produced audio that is not 16-bit PCM WAV".into(),
            },
            EspeakError::Timeout { ms } => TtsError::Timeout { ms },
            EspeakError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_maps_to_the_tts_error_that_says_what_went_wrong() {
        for (error, expected) in [
            (EspeakError::BinaryNotFound, "EngineUnavailable"),
            (
                EspeakError::VoiceListingFailed("exit 1".into()),
                "EngineUnavailable",
            ),
            (
                EspeakError::SubprocessFailed("unknown voice".into()),
                "SynthesisFailed",
            ),
            (EspeakError::UnsupportedAudio, "SynthesisFailed"),
            (EspeakError::Timeout { ms: 5_000 }, "Timeout"),
            (EspeakError::Io(std::io::Error::other("pipe")), "Io"),
        ] {
            let mapped = match TtsError::from(error) {
                TtsError::EngineUnavailable { .. } => "EngineUnavailable",
                TtsError::SynthesisFailed { .. } => "SynthesisFailed",
                TtsError::Timeout { .. } => "Timeout",
                TtsError::Io(_) => "Io",
                _ => "other",
            };
            assert_eq!(mapped, expected);
        }
    }

    #[test]
    fn a_subprocess_failure_keeps_the_stderr_text_as_synthesis_detail() {
        let mapped = TtsError::from(EspeakError::SubprocessFailed("unknown voice xx".into()));
        assert!(
            matches!(&mapped, TtsError::SynthesisFailed { detail, .. } if detail == "unknown voice xx"),
            "{mapped:?}"
        );
    }
}
