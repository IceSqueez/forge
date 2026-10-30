use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use forge_speech_types::{EngineId, PcmBuffer, TtsVoice, VoiceGender, VoiceId};
use forge_tts_core::{
    ENGINE_ID_PIPER, EngineCapabilities, SynthesisRequest, TtsEngine, TtsEngineFactory, TtsError,
};

mod locate;
mod subprocess;

const MODEL_FLAG: &str = "--model";
const MIN_RATE: f32 = 0.25;
const MAX_RATE: f32 = 4.0;
const NEUTRAL_RATE: f32 = 1.0;

static CAPABILITIES: EngineCapabilities = EngineCapabilities {
    ssml: false,
    neural_voices: true,
    streaming: false,
    custom_lexicons: false,
};

fn piper_engine_id() -> EngineId {
    EngineId(ENGINE_ID_PIPER.into())
}

pub struct PiperEngine {
    id: EngineId,
    piper_binary: PathBuf,
    voices_dir: PathBuf,
    timeout: Duration,
    voice_cache: Mutex<Option<Vec<PiperEntry>>>,
}

impl PiperEngine {
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

    pub fn voices_dir(data_dir: &std::path::Path) -> PathBuf {
        data_dir.join("tts").join("piper-voices")
    }
}

#[derive(Clone)]
struct PiperEntry {
    voice: TtsVoice,
    model_path: PathBuf,
    speaker: Option<u32>,
}

#[derive(serde::Deserialize)]
struct VoiceSidecar {
    language: SidecarLanguage,
    audio: SidecarAudio,
    #[serde(default = "default_num_speakers")]
    num_speakers: u32,
    #[serde(default)]
    speaker_id_map: std::collections::BTreeMap<String, u32>,
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
) -> Result<Vec<PiperEntry>, std::io::Error> {
    if !voices_dir.exists() {
        return Ok(vec![]);
    }
    let mut voices = Vec::new();
    for entry in std::fs::read_dir(voices_dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!(dir = %voices_dir.display(), "unreadable voice directory entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.extension().map(|e| e == "onnx").unwrap_or(false) {
            let sidecar = path.with_extension("onnx.json");
            if !sidecar.exists() {
                tracing::debug!(sidecar = %sidecar.display(), "no sidecar, skipping voice");
                continue;
            }
            let raw = match std::fs::read_to_string(&sidecar) {
                Ok(raw) => raw,
                Err(e) => {
                    tracing::warn!(sidecar = %sidecar.display(), "unreadable sidecar: {e}");
                    continue;
                }
            };
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
                let name = meta
                    .speaker_id_map
                    .iter()
                    .find(|(_, idx)| **idx == speaker)
                    .map(|(speaker_name, _)| capitalize(speaker_name))
                    .unwrap_or_else(|| voice_id.clone());
                voices.push(PiperEntry {
                    voice: TtsVoice {
                        id: VoiceId(voice_id.clone()),
                        name,
                        locale: locale.clone(),
                        gender: VoiceGender::Neutral,
                        engine_id: engine_id.clone(),
                        is_neural: true,
                        sample_rate_hint: meta.audio.sample_rate,
                    },
                    model_path: path.clone(),
                    speaker: (num > 1).then_some(speaker),
                });
            }
        }
    }
    voices.sort_by(|a, b| {
        a.voice
            .locale
            .cmp(&b.voice.locale)
            .then(a.voice.name.cmp(&b.voice.name))
    });
    Ok(voices)
}

fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn bounded_rate(rate_multiplier: f32) -> f32 {
    if rate_multiplier.is_finite() {
        rate_multiplier.clamp(MIN_RATE, MAX_RATE)
    } else {
        NEUTRAL_RATE
    }
}

impl PiperEngine {
    async fn catalog(&self) -> Result<Vec<PiperEntry>, TtsError> {
        {
            let cache = self.voice_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entries) = cache.as_ref() {
                return Ok(entries.clone());
            }
        }
        let voices_dir = self.voices_dir.clone();
        let engine_id = self.id.clone();
        let entries = tokio::task::spawn_blocking(move || scan_voices_dir(&voices_dir, &engine_id))
            .await
            .map_err(|e| TtsError::Io(std::io::Error::other(e.to_string())))??;
        {
            let mut cache = self.voice_cache.lock().unwrap_or_else(|e| e.into_inner());
            *cache = Some(entries.clone());
        }
        Ok(entries)
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
        Ok(self
            .catalog()
            .await?
            .into_iter()
            .map(|entry| entry.voice)
            .collect())
    }

    async fn synthesize(&self, request: SynthesisRequest) -> Result<PcmBuffer, TtsError> {
        if request.ssml {
            return Err(TtsError::SsmlUnsupported {
                id: piper_engine_id(),
            });
        }
        let entry = self
            .catalog()
            .await?
            .into_iter()
            .find(|entry| entry.voice.id == request.voice_id)
            .ok_or_else(|| TtsError::InvalidVoice {
                id: request.voice_id.clone(),
            })?;
        let sample_rate = entry.voice.sample_rate_hint;
        let speaker_idx = entry.speaker;
        let model_path = entry.model_path;

        let mut args: Vec<std::ffi::OsString> =
            vec![MODEL_FLAG.into(), model_path.into(), "--output_raw".into()];

        let rate = bounded_rate(request.rate_multiplier);
        if (rate - NEUTRAL_RATE).abs() > f32::EPSILON {
            let length_scale = NEUTRAL_RATE / rate;
            args.push("--length_scale".into());
            args.push(format!("{length_scale:.3}").into());
        }

        if let Some(speaker) = speaker_idx {
            args.push("--speaker".into());
            args.push(speaker.to_string().into());
        }

        let mut input = request.text.into_bytes();
        if input.last() != Some(&b'\n') {
            input.push(b'\n');
        }
        let output = self.run_piper(args, input).await?;

        if !output.status.success() {
            let stderr: String = String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(200)
                .collect();
            let stderr = stderr.trim();
            let detail = if stderr.is_empty() {
                format!("piper exited with {}", output.status)
            } else {
                format!("piper exited with {}: {stderr}", output.status)
            };
            return Err(TtsError::SynthesisFailed {
                id: piper_engine_id(),
                detail,
            });
        }

        let (pairs, _) = output.stdout.as_chunks::<2>();
        let samples: Vec<i16> = pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect();

        Ok(PcmBuffer::new(samples, sample_rate, 1))
    }
}

pub struct PiperEngineFactory {
    pub piper_binary: PathBuf,
    pub voices_dir: PathBuf,
    pub timeout: Duration,
}

impl TtsEngineFactory for PiperEngineFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

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

    fn write_voice(dir: &std::path::Path, stem: &str, sidecar: &str) {
        std::fs::write(dir.join(format!("{stem}.onnx")), b"fake model").expect("write onnx");
        std::fs::write(dir.join(format!("{stem}.onnx.json")), sidecar).expect("write sidecar");
    }

    fn scan(dir: &std::path::Path) -> Vec<PiperEntry> {
        scan_voices_dir(&dir.to_path_buf(), &EngineId("piper".into())).expect("scan")
    }

    #[test]
    fn scan_parses_sidecar_single_speaker() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_voice(
            tmp.path(),
            "uk_UA-test-medium",
            r#"{"language":{"code":"uk_UA"},"audio":{"sample_rate":22050},"num_speakers":1}"#,
        );
        let voices = scan(tmp.path());
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].voice.id.0, "uk_UA-test-medium");
        assert_eq!(voices[0].voice.locale, "uk-UA");
        assert_eq!(voices[0].voice.sample_rate_hint, 22_050);
        assert_eq!(voices[0].speaker, None);
    }

    #[test]
    fn scan_expands_multi_speaker_model_with_speaker_indices() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_voice(
            tmp.path(),
            "en_US-multi",
            r#"{"language":{"code":"en_US"},"audio":{"sample_rate":16000},"num_speakers":2}"#,
        );
        let voices = scan(tmp.path());
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].voice.id.0, "en_US-multi#0");
        assert_eq!(voices[1].voice.id.0, "en_US-multi#1");
        assert_eq!(voices[0].speaker, Some(0));
        assert_eq!(voices[1].speaker, Some(1));
    }

    #[test]
    fn scan_keeps_hash_in_file_stem_of_single_speaker_model() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_voice(
            tmp.path(),
            "en_US-a#7",
            r#"{"language":{"code":"en_US"},"audio":{"sample_rate":22050}}"#,
        );
        let voices = scan(tmp.path());
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].voice.id.0, "en_US-a#7");
        assert_eq!(voices[0].speaker, None);
        assert_eq!(voices[0].model_path, tmp.path().join("en_US-a#7.onnx"));
    }

    #[test]
    fn scan_skips_unreadable_sidecar_and_keeps_other_voices() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        std::fs::write(tmp.path().join("broken.onnx"), b"m").expect("onnx");
        std::fs::create_dir(tmp.path().join("broken.onnx.json")).expect("dir as sidecar");
        write_voice(
            tmp.path(),
            "ok",
            r#"{"language":{"code":"en_US"},"audio":{"sample_rate":22050}}"#,
        );
        let voices = scan(tmp.path());
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].voice.id.0, "ok");
    }

    #[test]
    fn scan_skips_malformed_sidecar() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        write_voice(tmp.path(), "bad", "{not json");
        assert!(scan(tmp.path()).is_empty());
    }

    #[test]
    fn bounded_rate_clamps_to_range_and_neutralises_non_finite() {
        for (input, expected) in [
            (MIN_RATE, MIN_RATE),
            (MIN_RATE - 0.01, MIN_RATE),
            (0.0, MIN_RATE),
            (-3.0, MIN_RATE),
            (MAX_RATE, MAX_RATE),
            (MAX_RATE + 0.01, MAX_RATE),
            (100.0, MAX_RATE),
            (1.5, 1.5),
            (f32::NAN, NEUTRAL_RATE),
            (f32::INFINITY, NEUTRAL_RATE),
            (f32::NEG_INFINITY, NEUTRAL_RATE),
        ] {
            assert_eq!(bounded_rate(input), expected, "{input}");
        }
    }
}
