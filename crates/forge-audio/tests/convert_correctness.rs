//! Integration tests for PCM format conversion: resample + remix pipeline.
//!
//! These tests verify the end-to-end 22050 Hz mono -> 48000 Hz stereo path that
//! the soundboard uses for every clip played on a typical USB/HDMI audio device.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use forge_audio::PcmBuffer;
use forge_audio::convert::{remix, resample};

#[test]
fn resample_22050_mono_to_48000_produces_correct_frame_ratio() {
    let frames = 2_205_usize; // 100 ms at 22050 Hz
    let src: Vec<i16> = (0..frames as i16).map(|i| i * 10).collect();

    let resampled = resample(&src, 22_050, 48_000, 1).unwrap();

    let expected_frames = (frames as f64 * 48_000.0 / 22_050.0) as usize;
    let actual_frames = resampled.len();
    let tolerance = (expected_frames as f64 * 0.02) as usize + 2;

    assert!(
        actual_frames.abs_diff(expected_frames) <= tolerance,
        "resampled frame count {actual_frames} must be within {tolerance} of expected {expected_frames}"
    );
}

#[test]
fn remix_mono_to_stereo_after_resample_has_correct_channel_count() {
    let frames = 2_205_usize;
    let src: Vec<i16> = (0..frames as i16).map(|i| i % 1000).collect();

    let resampled = resample(&src, 22_050, 48_000, 1).unwrap();
    let stereo = remix(&resampled, 1, 2);

    assert_eq!(
        stereo.len() % 2,
        0,
        "stereo output length must be even (2 samples per frame)"
    );

    let stereo_frames = stereo.len() / 2;
    let mono_frames = resampled.len();
    assert_eq!(
        stereo_frames, mono_frames,
        "remix mono->stereo must not change frame count"
    );
}

#[test]
fn pcmbuffer_from_22050_mono_resampled_to_48000_stereo_has_correct_metadata() {
    let frames = 4_410_usize; // 200 ms at 22050 Hz
    let src: Vec<i16> = (0..frames as i16).map(|i| (i % 500) * 60).collect();

    let resampled = resample(&src, 22_050, 48_000, 1).unwrap();
    let stereo = remix(&resampled, 1, 2);

    let expected_frames = (frames as f64 * 48_000.0 / 22_050.0) as usize;
    let tolerance = (expected_frames as f64 * 0.02) as usize + 2;

    let buf = PcmBuffer::new(stereo, 48_000, 2);

    assert_eq!(
        buf.sample_rate, 48_000,
        "sample_rate must be 48000 after resample"
    );
    assert_eq!(buf.channels, 2, "channels must be 2 after remix");
    assert!(
        buf.frame_count().abs_diff(expected_frames) <= tolerance,
        "frame_count {} must be within {tolerance} of expected {expected_frames}",
        buf.frame_count()
    );
}

#[test]
fn resample_noop_when_rates_are_equal() {
    let src: Vec<i16> = vec![100, 200, 300, 400];
    let out = resample(&src, 44_100, 44_100, 1).unwrap();
    assert_eq!(out, src, "identity resample must return unchanged samples");
}

#[test]
fn remix_mono_to_stereo_duplicates_each_sample() {
    let src: Vec<i16> = vec![1000, 2000, 3000];
    let out = remix(&src, 1, 2);
    assert_eq!(out, vec![1000, 1000, 2000, 2000, 3000, 3000]);
}

#[test]
fn remix_stereo_to_mono_averages_channels() {
    let src: Vec<i16> = vec![100, 200, 300, 400];
    let out = remix(&src, 2, 1);
    assert_eq!(out, vec![150, 350]);
}

#[test]
fn resample_empty_input_returns_empty() {
    let out = resample(&[], 22_050, 48_000, 1).unwrap();
    assert!(out.is_empty());
}
