use std::fmt;

use crate::speak_dispatcher::VoiceDescriptor;

const ENGINE_VOICE_SEPARATOR: char = '/';

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineVoiceError {
    #[error("\"{0}\" is not an engine voice")]
    NotAnEngineVoice(String),
    #[error("voice \"{voice_id}\" of engine \"{engine_id}\" is not installed")]
    NotInstalled { engine_id: String, voice_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineVoiceRef {
    pub engine_id: String,
    pub voice_id: String,
}

impl EngineVoiceRef {
    pub fn new(engine_id: impl Into<String>, voice_id: impl Into<String>) -> Self {
        Self {
            engine_id: engine_id.into(),
            voice_id: voice_id.into(),
        }
    }

    fn parse(encoded: &str) -> Option<Self> {
        let (engine_id, voice_id) = encoded.trim().split_once(ENGINE_VOICE_SEPARATOR)?;
        if engine_id.is_empty() || voice_id.is_empty() {
            return None;
        }
        Some(Self::new(engine_id, voice_id))
    }

    pub fn installed(encoded: &str, voices: &[VoiceDescriptor]) -> Result<Self, EngineVoiceError> {
        let voice = Self::parse(encoded)
            .ok_or_else(|| EngineVoiceError::NotAnEngineVoice(encoded.to_owned()))?;
        if !voice.is_installed_in(voices) {
            return Err(EngineVoiceError::NotInstalled {
                engine_id: voice.engine_id,
                voice_id: voice.voice_id,
            });
        }
        Ok(voice)
    }

    fn is_installed_in(&self, voices: &[VoiceDescriptor]) -> bool {
        voices
            .iter()
            .any(|voice| voice.engine_id == self.engine_id && voice.id == self.voice_id)
    }
}

impl fmt::Display for EngineVoiceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{ENGINE_VOICE_SEPARATOR}{}",
            self.engine_id, self.voice_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(engine_id: &str, voice_id: &str) -> VoiceDescriptor {
        VoiceDescriptor {
            id: voice_id.to_owned(),
            name: voice_id.to_owned(),
            locale: "en-US".to_owned(),
            engine_id: engine_id.to_owned(),
        }
    }

    #[test]
    fn a_displayed_voice_reads_back_as_the_same_installed_voice() {
        for (engine_id, voice_id) in [
            ("piper", "en_US-amy-medium"),
            ("piper", "en/US/amy"),
            ("sapi", "Microsoft Zira Desktop"),
            ("nsspeech", "com.apple.voice.compact.uk-UA.Lesya"),
        ] {
            let voice = EngineVoiceRef::new(engine_id, voice_id);
            let catalog = [installed("piper", "decoy"), installed(engine_id, voice_id)];

            let read_back = EngineVoiceRef::installed(&voice.to_string(), &catalog);

            assert_eq!(read_back, Ok(voice), "{engine_id} / {voice_id}");
        }
    }

    #[test]
    fn surrounding_whitespace_is_ignored_when_reading_a_voice() {
        let catalog = [installed("piper", "amy")];

        let read_back = EngineVoiceRef::installed("  piper/amy \t", &catalog);

        assert_eq!(read_back, Ok(EngineVoiceRef::new("piper", "amy")));
    }

    #[test]
    fn a_value_without_both_an_engine_and_a_voice_is_not_an_engine_voice() {
        let catalog = [installed("piper", "amy")];
        for encoded in ["", "   ", "amy", "/", "/amy", "piper/", " /amy"] {
            let result = EngineVoiceRef::installed(encoded, &catalog);

            assert_eq!(
                result,
                Err(EngineVoiceError::NotAnEngineVoice(encoded.to_owned())),
                "{encoded:?}"
            );
        }
    }

    #[test]
    fn a_voice_absent_from_the_catalog_is_not_installed() {
        let catalog = [installed("piper", "amy"), installed("sapi", "zira")];
        for (encoded, engine_id, voice_id) in [
            ("piper/zira", "piper", "zira"),
            ("espeak-ng/amy", "espeak-ng", "amy"),
            ("piper/Amy", "piper", "Amy"),
            ("piper/amy/x", "piper", "amy/x"),
        ] {
            let result = EngineVoiceRef::installed(encoded, &catalog);

            assert_eq!(
                result,
                Err(EngineVoiceError::NotInstalled {
                    engine_id: engine_id.to_owned(),
                    voice_id: voice_id.to_owned(),
                }),
                "{encoded:?}"
            );
        }
    }

    #[test]
    fn no_voice_is_installed_while_the_catalog_is_empty() {
        let result = EngineVoiceRef::installed("piper/amy", &[]);

        assert!(matches!(result, Err(EngineVoiceError::NotInstalled { .. })));
    }
}
