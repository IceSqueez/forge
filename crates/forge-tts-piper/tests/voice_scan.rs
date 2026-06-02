//! Regression: PiperEngine voice scanner reads onnx + sidecar JSON correctly.
//!
//! Uses a synthetic voices directory with hand-crafted .onnx stubs and
//! .onnx.json sidecars. No real Piper binary is required — the scanner only
//! reads the filesystem.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Duration;

use forge_tts_core::TtsEngineFactory;
use forge_tts_core::{EngineId, TtsEngine, VoiceId};
use forge_tts_piper::{PiperEngine, PiperEngineFactory};

fn sidecar_json(locale_code: &str, sample_rate: u32, num_speakers: u32) -> String {
    format!(
        r#"{{"language":{{"code":"{locale_code}","family":"test","name_native":"Test"}},"audio":{{"sample_rate":{sample_rate}}},"num_speakers":{num_speakers}}}"#
    )
}

fn synthetic_voice(
    dir: &std::path::Path,
    stem: &str,
    locale_code: &str,
    sample_rate: u32,
    num_speakers: u32,
) {
    let onnx = dir.join(format!("{stem}.onnx"));
    let sidecar = dir.join(format!("{stem}.onnx.json"));
    std::fs::write(&onnx, b"fake onnx content").expect("write onnx");
    std::fs::write(
        &sidecar,
        sidecar_json(locale_code, sample_rate, num_speakers),
    )
    .expect("write sidecar");
}

fn fake_piper_binary(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("piper");
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").expect("write fake binary");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("chmod fake binary");
    path
}

#[tokio::test]
async fn two_single_speaker_voices_discovered() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");

    synthetic_voice(&voices_dir, "uk_UA-ukrainian-medium", "uk_UA", 22_050, 1);
    synthetic_voice(&voices_dir, "en_US-amy-medium", "en_US", 22_050, 1);

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");

    let voices = engine.list_voices().await.expect("list_voices");
    assert_eq!(
        voices.len(),
        2,
        "expected exactly 2 voices, got {}",
        voices.len()
    );

    let ids: Vec<&str> = voices.iter().map(|v| v.id.0.as_str()).collect();
    assert!(ids.contains(&"uk_UA-ukrainian-medium"), "missing UK voice");
    assert!(ids.contains(&"en_US-amy-medium"), "missing EN voice");
}

#[tokio::test]
async fn single_speaker_voice_has_expected_metadata() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");

    synthetic_voice(&voices_dir, "uk_UA-ukrainian-medium", "uk_UA", 22_050, 1);

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");
    let voices = engine.list_voices().await.expect("list_voices");

    let voice = &voices[0];
    assert_eq!(voice.id, VoiceId("uk_UA-ukrainian-medium".into()));
    assert_eq!(
        voice.locale, "uk-UA",
        "locale must use BCP-47 format (- not _)"
    );
    assert_eq!(voice.sample_rate_hint, 22_050);
    assert_eq!(voice.engine_id, EngineId("piper".into()));
    assert!(!voice.is_neural);
}

#[tokio::test]
async fn multi_speaker_model_produces_one_voice_per_speaker() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");

    synthetic_voice(&voices_dir, "en_US-multi", "en_US", 16_000, 3);

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");
    let voices = engine.list_voices().await.expect("list_voices");

    assert_eq!(voices.len(), 3, "3 speakers should produce 3 voices");
    let ids: Vec<&str> = voices.iter().map(|v| v.id.0.as_str()).collect();
    assert!(ids.contains(&"en_US-multi#0"));
    assert!(ids.contains(&"en_US-multi#1"));
    assert!(ids.contains(&"en_US-multi#2"));
}

#[tokio::test]
async fn onnx_without_sidecar_is_silently_skipped() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");

    let onnx = voices_dir.join("orphan.onnx");
    std::fs::write(&onnx, b"no sidecar here").expect("write onnx");

    synthetic_voice(&voices_dir, "en_US-valid", "en_US", 22_050, 1);

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");
    let voices = engine.list_voices().await.expect("list_voices");

    assert_eq!(
        voices.len(),
        1,
        "orphan (no sidecar) must be skipped; only 1 valid voice expected"
    );
    assert_eq!(voices[0].id.0, "en_US-valid");
}

#[tokio::test]
async fn missing_voices_dir_returns_empty_list() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("no-such-dir");

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");
    let voices = engine
        .list_voices()
        .await
        .expect("list_voices with empty dir");

    assert!(
        voices.is_empty(),
        "missing voices dir must return empty list, not error"
    );
}

#[test]
fn factory_create_succeeds_when_binary_exists() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let bin = fake_piper_binary(tmp.path());
    let voices_dir = tmp.path().join("voices");

    let factory = PiperEngineFactory {
        piper_binary: bin,
        voices_dir,
        timeout: Duration::from_secs(10),
    };
    let result = factory.create();
    assert!(
        result.is_ok(),
        "factory.create must succeed when binary exists"
    );
}

#[test]
fn factory_create_fails_when_binary_missing() {
    use forge_tts_core::TtsError;
    let factory = PiperEngineFactory {
        piper_binary: PathBuf::from("/nonexistent/piper"),
        voices_dir: PathBuf::from("/tmp"),
        timeout: Duration::from_secs(10),
    };
    let result = factory.create();
    assert!(
        matches!(result, Err(TtsError::EngineUnavailable { .. })),
        "factory.create must return EngineUnavailable for missing binary"
    );
}
