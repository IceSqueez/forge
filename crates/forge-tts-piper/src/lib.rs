use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use forge_tts_core::{
    EngineCapabilities, EngineId, PcmBuffer, SynthesisRequest, TtsEngine, TtsEngineFactory,
    TtsError, TtsVoice, VoiceGender, VoiceId,
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

pub struct PiperEngine {
    id: EngineId,
    piper_binary: PathBuf,
    voices_dir: PathBuf,
    timeout: Duration,
    voice_cache: Mutex<Option<Vec<TtsVoice>>>,
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
            voice_cache: Mutex::new(None),
        })
    }

    /// Returns the canonical path to the voices directory under `data_dir`.
    pub fn voices_dir(data_dir: &std::path::Path) -> PathBuf {
        data_dir.join("tts").join("piper-voices")
    }
}

#[derive(serde::Deserialize)]
struct VoiceSidecar {
    language: SidecarLanguage,
    audio: SidecarAudio,
    #[serde(default = "default_num_speakers")]
    num_speakers: u32,
}

#[derive(serde::Deserialize)]
struct SidecarLanguage {
    code: String,
}

#[derive(serde::Deserialize)]
struct SidecarAudio {
    sample_rate: u32,
}

fn default_num_speakers() -> u32 {
    1
}

fn bcp47_locale(piper_code: &str) -> String {
    piper_code.replace('_', "-")
}

fn scan_voices_dir(
    voices_dir: &PathBuf,
    engine_id: &EngineId,
) -> Result<Vec<TtsVoice>, std::io::Error> {
    if !voices_dir.exists() {
        return Ok(vec![]);
    }
    let mut voices = Vec::new();
    for entry in std::fs::read_dir(voices_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "onnx").unwrap_or(false) {
            let sidecar = path.with_extension("onnx.json");
            if !sidecar.exists() {
                tracing::debug!(sidecar = %sidecar.display(), "no sidecar, skipping voice");
                continue;
            }
            let raw = std::fs::read_to_string(&sidecar)?;
            let meta: VoiceSidecar = match serde_json::from_str(&raw) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(sidecar = %sidecar.display(), "malformed sidecar: {e}");
                    continue;
                }
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let locale = bcp47_locale(&meta.language.code);
            let num = meta.num_speakers.max(1);
            for speaker in 0..num {
                let voice_id = if num == 1 {
                    stem.clone()
                } else {
                    format!("{}#{}", stem, speaker)
                };
                voices.push(TtsVoice {
                    id: VoiceId(voice_id.clone()),
                    name: voice_id,
                    locale: locale.clone(),
                    gender: VoiceGender::Neutral,
                    engine_id: engine_id.clone(),
                    is_neural: false,
                    sample_rate_hint: meta.audio.sample_rate,
                });
            }
        }
    }
    voices.sort_by(|a, b| a.locale.cmp(&b.locale).then(a.name.cmp(&b.name)));
    Ok(voices)
}

fn parse_voice_id(voice_id: &str) -> (&str, Option<u32>) {
    if let Some(idx) = voice_id.rfind('#') {
        let stem = &voice_id[..idx];
        let speaker = voice_id[idx + 1..].parse().ok();
        (stem, speaker)
    } else {
        (voice_id, None)
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
        {
            let cache = self.voice_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(voices) = cache.as_ref() {
                return Ok(voices.clone());
            }
        }
        let voices_dir = self.voices_dir.clone();
        let engine_id = self.id.clone();
        let voices = tokio::task::spawn_blocking(move || scan_voices_dir(&voices_dir, &engine_id))
            .await
            .map_err(|e| TtsError::Io(std::io::Error::other(e.to_string())))??;
        {
            let mut cache = self.voice_cache.lock().unwrap_or_else(|e| e.into_inner());
            *cache = Some(voices.clone());
        }
        Ok(voices)
    }

    /// Pitch adjustment via `pitch_semitones` is deferred to a `forge-audio` post-processing pass
    /// (planned for beta-4 polish). Piper has no native pitch flag; the subprocess synthesizes at
    /// the model's native rate and pitch. The `rate_multiplier` field is honoured via `--length_scale`.
    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: piper_engine_id(),
            });
        }
        let voices = self.list_voices().await?;
        let sample_rate = voices
            .iter()
            .find(|v| v.id == request.voice_id)
            .map(|v| v.sample_rate_hint)
            .ok_or_else(|| TtsError::InvalidVoice {
                id: request.voice_id.clone(),
            })?;

        let (stem, speaker_idx) = parse_voice_id(&request.voice_id.0);
        let model_path = self.voices_dir.join(format!("{}.onnx", stem));

        if !model_path.exists() {
            return Err(TtsError::InvalidVoice {
                id: request.voice_id.clone(),
            });
        }

        let mut args: Vec<std::ffi::OsString> =
            vec!["--model".into(), model_path.into(), "--output_raw".into()];

        if (request.rate_multiplier - 1.0_f32).abs() > f32::EPSILON {
            let length_scale = 1.0_f32 / request.rate_multiplier;
            args.push("--length_scale".into());
            args.push(format!("{length_scale:.3}").into());
        }

        if let Some(speaker) = speaker_idx {
            args.push("--speaker".into());
            args.push(speaker.to_string().into());
        }

        let piper_binary = self.piper_binary.clone();
        let text = request.text.clone();
        let timeout = self.timeout;

        let mut child = tokio::process::Command::new(&piper_binary)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(TtsError::Io)?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let mut text_bytes = text.into_bytes();
            if text_bytes.last() != Some(&b'\n') {
                text_bytes.push(b'\n');
            }
            stdin.write_all(&text_bytes).await.map_err(TtsError::Io)?;
        }

        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| TtsError::Timeout {
                ms: timeout.as_millis() as u64,
            })?
            .map_err(TtsError::Io)?;

        if !output.status.success() {
            return Err(TtsError::EngineUnavailable {
                id: piper_engine_id(),
                detail: format!("piper exited with {}", output.status),
            });
        }

        let samples: Vec<i16> = output
            .stdout
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();

        Ok(PcmBuffer::new(samples, sample_rate, 1))
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

    #[test]
    fn scan_empty_dir_returns_empty() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let engine_id = EngineId("piper".into());
        let voices = scan_voices_dir(&tmp.path().to_path_buf(), &engine_id).expect("scan");
        assert!(voices.is_empty());
    }

    #[test]
    fn scan_nonexistent_dir_returns_empty() {
        let engine_id = EngineId("piper".into());
        let voices =
            scan_voices_dir(&PathBuf::from("/nonexistent/voices"), &engine_id).expect("scan");
        assert!(voices.is_empty());
    }

    #[test]
    fn scan_parses_sidecar_single_speaker() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let onnx = tmp.path().join("uk_UA-test-medium.onnx");
        std::fs::write(&onnx, b"fake model").expect("write onnx");
        let sidecar = tmp.path().join("uk_UA-test-medium.onnx.json");
        std::fs::write(
            &sidecar,
            r#"{"language":{"code":"uk_UA","family":"uk","name_native":"Українська"},"audio":{"sample_rate":22050},"num_speakers":1}"#,
        )
        .expect("write sidecar");

        let engine_id = EngineId("piper".into());
        let voices = scan_voices_dir(&tmp.path().to_path_buf(), &engine_id).expect("scan");
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id.0, "uk_UA-test-medium");
        assert_eq!(voices[0].locale, "uk-UA");
        assert_eq!(voices[0].sample_rate_hint, 22_050);
    }

    #[test]
    fn scan_parses_multi_speaker_model() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let onnx = tmp.path().join("en_US-multi.onnx");
        std::fs::write(&onnx, b"fake model").expect("write onnx");
        let sidecar = tmp.path().join("en_US-multi.onnx.json");
        std::fs::write(
            &sidecar,
            r#"{"language":{"code":"en_US"},"audio":{"sample_rate":16000},"num_speakers":2}"#,
        )
        .expect("write sidecar");

        let engine_id = EngineId("piper".into());
        let voices = scan_voices_dir(&tmp.path().to_path_buf(), &engine_id).expect("scan");
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].id.0, "en_US-multi#0");
        assert_eq!(voices[1].id.0, "en_US-multi#1");
    }

    #[test]
    fn parse_voice_id_single() {
        let (stem, speaker) = parse_voice_id("uk_UA-test-medium");
        assert_eq!(stem, "uk_UA-test-medium");
        assert!(speaker.is_none());
    }

    #[test]
    fn parse_voice_id_multi() {
        let (stem, speaker) = parse_voice_id("en_US-multi#1");
        assert_eq!(stem, "en_US-multi");
        assert_eq!(speaker, Some(1u32));
    }

    #[tokio::test]
    #[ignore = "requires real piper binary and voice model; set PIPER_BIN and PIPER_MODEL env vars"]
    async fn synthesize_produces_pcm() {
        let piper = std::env::var("PIPER_BIN").expect("PIPER_BIN env var");
        let model_dir = std::env::var("PIPER_MODEL_DIR").expect("PIPER_MODEL_DIR env var");
        let voice_id = std::env::var("PIPER_VOICE_ID").expect("PIPER_VOICE_ID env var");
        let engine = PiperEngine::new(
            PathBuf::from(&piper),
            PathBuf::from(&model_dir),
            Duration::from_secs(30),
        )
        .expect("engine");
        let req = SynthesisRequest {
            text: "hello world".into(),
            voice_id: VoiceId(voice_id),
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            ssml: false,
        };
        let buf = engine.synthesize(req).await.expect("synthesize");
        assert!(!buf.samples.is_empty());
        assert_eq!(buf.channels, 1);
    }
}
