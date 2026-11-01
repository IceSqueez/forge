use forge_speech_types::EngineId;
use forge_tts_core::{ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH, ENGINE_SYNTHESIS_BUDGET, TtsError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum AvSpeechError {
    #[error("no AVSpeech voices installed")]
    NoCatalog,

    #[error("synthesis timed out without audio")]
    Timeout,

    #[error("AVSpeech produced no audio")]
    NoAudio,

    #[error("voice {0:?} is not installed")]
    InvalidVoice(forge_speech_types::VoiceId),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<AvSpeechError> for TtsError {
    fn from(e: AvSpeechError) -> Self {
        match e {
            AvSpeechError::NoCatalog => TtsError::EngineUnavailable {
                id: EngineId(ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH.into()),
                detail: "no AVSpeech voices installed".into(),
            },
            AvSpeechError::Timeout => TtsError::timeout_after(ENGINE_SYNTHESIS_BUDGET),
            AvSpeechError::NoAudio => TtsError::SynthesisFailed {
                id: EngineId(ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH.into()),
                detail: e.to_string(),
            },
            AvSpeechError::InvalidVoice(id) => TtsError::InvalidVoice { id },
            AvSpeechError::Io(io_err) => TtsError::Io(io_err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_maps_to_the_tts_error_that_says_what_went_wrong() {
        for (error, expected) in [
            (AvSpeechError::NoCatalog, "EngineUnavailable"),
            (AvSpeechError::Timeout, "Timeout"),
            (AvSpeechError::NoAudio, "SynthesisFailed"),
            (AvSpeechError::Io(std::io::Error::other("pipe")), "Io"),
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
        let mapped = TtsError::from(AvSpeechError::Timeout);
        let expected_ms = u64::try_from(ENGINE_SYNTHESIS_BUDGET.as_millis()).unwrap_or(u64::MAX);
        assert!(
            matches!(mapped, TtsError::Timeout { ms } if ms == expected_ms),
            "{mapped:?}"
        );
    }
}
