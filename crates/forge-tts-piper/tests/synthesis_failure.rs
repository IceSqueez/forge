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
