use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice,
};

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: false,
    streaming: false,
    custom_lexicons: false,
};

fn piper_engine_id() -> EngineId {
    EngineId("piper".into())
}

/// Metadata parsed from a Piper `.onnx.json` sidecar file.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PiperVoice {
    pub model_path: PathBuf,
    pub voice_id: forge_tts_core::VoiceId,
    pub name: String,
    pub locale: String,
    pub sample_rate: u32,
}

pub struct PiperEngine {
    id: EngineId,
    piper_binary: PathBuf,
    voices_dir: PathBuf,
    timeout: Duration,
}

impl PiperEngine {
    /// `piper_binary` — path to the `piper` executable on disk.
    ///
    /// Caller is responsible for locating the binary (bundled asset or PATH lookup).
    /// Returns `TtsError::EngineUnavailable` if the binary is absent.
    pub fn new(
        piper_binary: PathBuf,
        voices_dir: PathBuf,
        timeout: Duration,
    ) -> Result<Self, TtsError> {
        if !piper_binary.exists() {
            return Err(TtsError::EngineUnavailable {
                id: piper_engine_id(),
                detail: format!("binary not found at {}", piper_binary.display()),
            });
        }
        Ok(Self {
            id: piper_engine_id(),
            piper_binary,
            voices_dir,
            timeout,
        })
    }

    /// Returns the canonical path to the voices directory under `data_dir`.
    pub fn voices_dir(data_dir: &std::path::Path) -> PathBuf {
        data_dir.join("tts").join("piper-voices")
    }
}

#[async_trait]
impl TtsEngine for PiperEngine {
    fn engine_id(&self) -> &EngineId {
        &self.id
    }

    fn capabilities(&self) -> &EngineCapabilities {
        &CAPABILITIES
    }

    async fn list_voices(&self) -> Result<Vec<TtsVoice>, TtsError> {
        tracing::debug!(dir = %self.voices_dir.display(), "piper voice catalog not yet wired");
        Ok(vec![])
    }

    async fn synthesize(&self, _request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        tracing::warn!(
            binary = %self.piper_binary.display(),
            timeout_ms = self.timeout.as_millis(),
            "piper synthesis not yet wired"
        );
        Err(TtsError::EngineUnavailable {
            id: piper_engine_id(),
            detail: "not yet wired".into(),
        })
    }
}

pub struct PiperEngineFactory {
    pub piper_binary: PathBuf,
    pub voices_dir: PathBuf,
    pub timeout: Duration,
}

impl TtsEngineFactory for PiperEngineFactory {
    fn create(&self) -> Result<Box<dyn TtsEngine>, TtsError> {
        PiperEngine::new(
            self.piper_binary.clone(),
            self.voices_dir.clone(),
            self.timeout,
        )
        .map(|e| Box::new(e) as Box<dyn TtsEngine>)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_error_for_missing_binary() {
        let result = PiperEngine::new(
            PathBuf::from("/nonexistent/piper"),
            PathBuf::from("/tmp"),
            Duration::from_secs(30),
        );
        assert!(matches!(result, Err(TtsError::EngineUnavailable { .. })));
    }

    #[test]
    fn voices_dir_path() {
        let data = std::path::Path::new("/home/user/.local/share/forge");
        let dir = PiperEngine::voices_dir(data);
        assert_eq!(
            dir.to_str().unwrap(),
            "/home/user/.local/share/forge/tts/piper-voices"
        );
    }
}
