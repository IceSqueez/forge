#![allow(clippy::expect_used)]
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_speech_types::VoiceId;
use forge_tts_core::{SynthesisRequest, TtsEngine, TtsError};
use forge_tts_piper::PiperEngine;

const VOICE_STEM: &str = "en_US-amy-medium";

fn install_voice(dir: &Path) {
    std::fs::write(dir.join(format!("{VOICE_STEM}.onnx")), b"fake onnx").expect("write onnx");
    std::fs::write(
        dir.join(format!("{VOICE_STEM}.onnx.json")),
        r#"{"language":{"code":"en_US","family":"en","name_native":"English"},"audio":{"sample_rate":22050},"num_speakers":1}"#,
    )
    .expect("write sidecar");
}

fn install_piper(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("piper");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake binary");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fake binary");
    path
}

fn request() -> SynthesisRequest {
    SynthesisRequest {
        text: "hello".into(),
        voice_id: VoiceId(VOICE_STEM.into()),
        pitch_semitones: 0.0,
        rate_multiplier: 1.0,
        ssml: false,
    }
}

#[tokio::test]
async fn a_piper_process_that_exits_with_failure_is_a_synthesis_failure_not_an_unavailable_engine()
{
    for (body, detail_part) in [
        (
            "cat > /dev/null; echo 'model is corrupt' >&2; exit 3",
            "model is corrupt",
        ),
        ("cat > /dev/null; exit 3", "piper exited with"),
    ] {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let voices_dir = tmp.path().join("voices");
        std::fs::create_dir(&voices_dir).expect("mkdir voices");
        install_voice(&voices_dir);
        let piper = install_piper(tmp.path(), body);
        let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");

        let result = engine.synthesize(request()).await;

        assert!(
            matches!(&result, Err(TtsError::SynthesisFailed { id, detail }) if id.0 == "piper" && detail.contains(detail_part)),
            "{body}: {result:?}"
        );
    }
}

struct Rig {
    tmp: tempfile::TempDir,
    engine: PiperEngine,
}

fn rig_with_budget(body: &str, budget: Duration) -> Rig {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");
    install_voice(&voices_dir);
    std::fs::write(voices_dir.join("en_US-multi.onnx"), b"fake onnx").expect("write onnx");
    std::fs::write(
        voices_dir.join("en_US-multi.onnx.json"),
        r#"{"language":{"code":"en_US"},"audio":{"sample_rate":16000},"num_speakers":3}"#,
    )
    .expect("write sidecar");
    let script = body.replace("$OUT", &tmp.path().display().to_string());
    let piper = install_piper(tmp.path(), &script);
    let engine = PiperEngine::new(piper, voices_dir, budget).expect("engine");
    Rig { tmp, engine }
}

fn rig(body: &str) -> Rig {
    rig_with_budget(body, Duration::from_secs(30))
}

const RECORD_ARGV: &str = r#"printf '%s\n' "$@" > $OUT/argv; cat > /dev/null"#;

impl Rig {
    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.tmp.path().join("argv"))
            .expect("recorded argv")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn flag_value(&self, flag: &str) -> Option<String> {
        let argv = self.argv();
        let at = argv.iter().position(|arg| arg == flag)?;
        argv.get(at + 1).cloned()
    }
}

#[tokio::test]
async fn little_endian_stdout_becomes_samples_at_the_voice_rate_and_a_stray_byte_is_dropped() {
    let rig = rig(r"cat > /dev/null; printf '\001\000\377\177\000\200\007'");

    let pcm = rig.engine.synthesize(request()).await.expect("pcm");

    assert_eq!(pcm.samples, vec![1, i16::MAX, i16::MIN]);
    assert_eq!(pcm.sample_rate, 22_050);
}

#[tokio::test]
async fn a_speed_change_passes_the_inverse_length_scale_and_neutral_speed_passes_none() {
    let rig = rig(RECORD_ARGV);

    rig.engine
        .synthesize(SynthesisRequest {
            rate_multiplier: 2.0,
            ..request()
        })
        .await
        .expect("pcm");
    assert_eq!(rig.flag_value("--length_scale").as_deref(), Some("0.500"));

    rig.engine.synthesize(request()).await.expect("pcm");
    assert_eq!(rig.flag_value("--length_scale"), None);
}

#[tokio::test]
async fn a_multi_speaker_voice_passes_its_speaker_and_a_single_speaker_voice_passes_none() {
    let rig = rig(RECORD_ARGV);

    rig.engine
        .synthesize(SynthesisRequest {
            voice_id: VoiceId("en_US-multi#2".into()),
            ..request()
        })
        .await
        .expect("pcm");
    assert_eq!(rig.flag_value("--speaker").as_deref(), Some("2"));

    rig.engine.synthesize(request()).await.expect("pcm");
    assert_eq!(rig.flag_value("--speaker"), None);
}

#[tokio::test]
async fn the_text_reaches_stdin_ending_in_exactly_one_newline() {
    for text in ["hello", "hello\n"] {
        let rig = rig("cat > $OUT/stdin");

        rig.engine
            .synthesize(SynthesisRequest {
                text: text.into(),
                ..request()
            })
            .await
            .expect("pcm");

        assert_eq!(
            std::fs::read_to_string(rig.tmp.path().join("stdin")).expect("recorded stdin"),
            "hello\n"
        );
    }
}

#[tokio::test]
async fn ssml_is_refused_before_the_process_starts() {
    let rig = rig("touch $OUT/started");

    let result = rig
        .engine
        .synthesize(SynthesisRequest {
            ssml: true,
            ..request()
        })
        .await;

    assert!(matches!(result, Err(TtsError::SsmlUnsupported { .. })));
    assert!(!rig.tmp.path().join("started").exists());
}

#[tokio::test]
async fn a_voice_outside_the_catalog_is_refused_before_the_process_starts() {
    let rig = rig("touch $OUT/started");

    let result = rig
        .engine
        .synthesize(SynthesisRequest {
            voice_id: VoiceId("missing".into()),
            ..request()
        })
        .await;

    assert!(matches!(result, Err(TtsError::InvalidVoice { id }) if id.0 == "missing"));
    assert!(!rig.tmp.path().join("started").exists());
}

#[tokio::test]
async fn a_process_that_never_finishes_is_a_timeout_carrying_the_budget() {
    const BUDGET_MS: u64 = 50;
    let rig = rig_with_budget("exec sleep 600", Duration::from_millis(BUDGET_MS));

    let result = rig.engine.synthesize(request()).await;

    assert!(matches!(result, Err(TtsError::Timeout { ms }) if ms == BUDGET_MS));
}
