use forge_speech_types::EngineId;
use forge_tts_core::{ENGINE_ID_NSSPEECH, TtsError};

use crate::synth::SYNTHESIS_WAIT_TIMEOUT;

#[derive(Debug, thiserror::Error)]
pub(crate) enum NsSpeechError {
    #[error("no AVSpeech voices installed")]
    NoCatalog,

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
                id: EngineId(ENGINE_ID_NSSPEECH.into()),
                detail: "no AVSpeech voices installed".into(),
            },
            NsSpeechError::Timeout => TtsError::Timeout {
                ms: u64::try_from(SYNTHESIS_WAIT_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
            },
            NsSpeechError::NoAudio => TtsError::SynthesisFailed {
                id: EngineId(ENGINE_ID_NSSPEECH.into()),
                detail: e.to_string(),
            },
            NsSpeechError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_maps_to_the_tts_error_that_says_what_went_wrong() {
        for (error, expected) in [
            (NsSpeechError::NoCatalog, "EngineUnavailable"),
            (NsSpeechError::Timeout, "Timeout"),
            (NsSpeechError::NoAudio, "SynthesisFailed"),
            (NsSpeechError::Io(std::io::Error::other("pipe")), "Io"),
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
    fn a_timeout_reports_the_wait_that_elapsed() {
        let mapped = TtsError::from(NsSpeechError::Timeout);
        let expected_ms = u64::try_from(SYNTHESIS_WAIT_TIMEOUT.as_millis()).unwrap_or(u64::MAX);
        assert!(
            matches!(mapped, TtsError::Timeout { ms } if ms == expected_ms),
            "{mapped:?}"
        );
    }
}
