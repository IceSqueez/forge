//! Smoke tests for the symphonia-backed PCM decoder.
//!
//! Uses synthetic WAV data constructed in-memory so no real audio files are needed.
//! All tests run without an audio device.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::io::Write as _;

use forge_audio::decode_file;

fn write_wav_bytes(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = (samples.len() * 2) as u32;
    let file_len = 36 + data_len;

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    buf.write_all(b"RIFF").unwrap();
    buf.write_all(&file_len.to_le_bytes()).unwrap();
    buf.write_all(b"WAVE").unwrap();
    buf.write_all(b"fmt ").unwrap();
    buf.write_all(&16u32.to_le_bytes()).unwrap();
    buf.write_all(&1u16.to_le_bytes()).unwrap();
    buf.write_all(&channels.to_le_bytes()).unwrap();
    buf.write_all(&sample_rate.to_le_bytes()).unwrap();
    buf.write_all(&byte_rate.to_le_bytes()).unwrap();
    buf.write_all(&block_align.to_le_bytes()).unwrap();
    buf.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    buf.write_all(b"data").unwrap();
    buf.write_all(&data_len.to_le_bytes()).unwrap();
    for &s in samples {
        buf.write_all(&s.to_le_bytes()).unwrap();
    }
    buf
}

fn sine_samples(sample_rate: u32, freq_hz: f64, duration_ms: u64) -> Vec<i16> {
    let n = (u64::from(sample_rate) * duration_ms / 1_000) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(sample_rate);
            let v = (2.0 * std::f64::consts::PI * freq_hz * t).sin();
            (v * 32_767.0) as i16
        })
        .collect()
}

#[test]
fn decode_22050_mono_100ms_has_correct_sample_rate_and_channels() {
    let sample_rate = 22_050_u32;
    let samples = sine_samples(sample_rate, 1_000.0, 100);
    let wav_bytes = write_wav_bytes(sample_rate, 1, &samples);

    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(tmp.path(), &wav_bytes).unwrap();

    let pcm = decode_file(tmp.path()).unwrap();

    assert_eq!(pcm.sample_rate, 22_050, "sample_rate must be preserved");
    assert_eq!(pcm.channels, 1, "channels must be 1 for mono");
}

#[test]
fn decode_22050_mono_100ms_frame_count_matches_duration() {
    let sample_rate = 22_050_u32;
    let samples = sine_samples(sample_rate, 1_000.0, 100);
    let expected_frames = samples.len();
    let wav_bytes = write_wav_bytes(sample_rate, 1, &samples);

    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(tmp.path(), &wav_bytes).unwrap();

    let pcm = decode_file(tmp.path()).unwrap();

    let tolerance = expected_frames / 20 + 2;
    assert!(
        pcm.frame_count().abs_diff(expected_frames) <= tolerance,
        "frame_count {} must be within {tolerance} of expected {expected_frames}",
        pcm.frame_count()
    );
}

#[test]
fn decode_44100_stereo_500ms_has_correct_metadata() {
    let sample_rate = 44_100_u32;
    let channels = 2_u16;
    let samples = sine_samples(sample_rate, 440.0, 500);
    let stereo: Vec<i16> = samples
        .iter()
        .flat_map(|&s| [s, s])
        .collect();
    let wav_bytes = write_wav_bytes(sample_rate, channels, &stereo);

    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(tmp.path(), &wav_bytes).unwrap();

    let pcm = decode_file(tmp.path()).unwrap();
    assert_eq!(pcm.sample_rate, 44_100);
    assert_eq!(pcm.channels, 2);
    assert!(pcm.frame_count() > 0);
}

#[test]
fn decode_missing_file_returns_io_error() {
    let result = decode_file(std::path::Path::new("/nonexistent_forge_test/audio.wav"));
    assert!(result.is_err(), "decode of missing file must return Err");
}

#[test]
fn decode_empty_wav_data_returns_decode_error() {
    let tmp = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
    std::fs::write(tmp.path(), b"not a wav file at all").unwrap();
    let result = decode_file(tmp.path());
    assert!(result.is_err(), "decode of corrupt data must return Err");
}
