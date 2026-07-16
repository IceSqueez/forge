//! Regression: PiperEngine voice scanner reads onnx + sidecar JSON correctly.
//!
//! Uses a synthetic voices directory with hand-crafted .onnx stubs and
//! .onnx.json sidecars. No real Piper binary is required - the scanner only
//! reads the filesystem.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::time::Duration;

use forge_tts_core::TtsEngine;
use forge_tts_piper::PiperEngine;

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
async fn end_to_end_voice_listing_through_engine() {
    // E2E happy path: 2 single-speaker + 1 multi-speaker (3 speakers) → expect 5 voices
    // through the public list_voices() path. The internal scanner is covered by lib.rs unit
    // tests; this confirms the engine wires it through correctly.
    let tmp = tempfile::tempdir().expect("tmpdir");
    let voices_dir = tmp.path().join("voices");
    std::fs::create_dir(&voices_dir).expect("mkdir voices");
    synthetic_voice(&voices_dir, "uk_UA-ukrainian-medium", "uk_UA", 22_050, 1);
    synthetic_voice(&voices_dir, "en_US-amy-medium", "en_US", 22_050, 1);
    synthetic_voice(&voices_dir, "en_US-multi", "en_US", 16_000, 3);

    let bin_dir = tmp.path().join("bin");
    std::fs::create_dir(&bin_dir).expect("mkdir bin");
    let piper = fake_piper_binary(&bin_dir);

    let engine = PiperEngine::new(piper, voices_dir, Duration::from_secs(30)).expect("engine");
    let voices = engine.list_voices().await.expect("list_voices");
    assert_eq!(voices.len(), 5);
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
