//! PCM format conversion: sample-rate resampling and channel count remixing.
//!
//! `FastFixedIn` is chosen over `SincFixedIn` because it processes the entire input
//! as a single fixed-size chunk (no streaming latency) and is fast enough for
//! pre-rendered soundboard clips. Quality difference is inaudible at >=44.1 kHz.
//! Reference: <https://docs.rs/rubato/0.16/>

use rubato::{FastFixedIn, PolynomialDegree, Resampler};

use crate::error::AudioError;

pub fn resample(
    src: &[i16],
    src_rate: u32,
    dst_rate: u32,
    channels: u16,
) -> Result<Vec<i16>, AudioError> {
    if src_rate == dst_rate {
        return Ok(src.to_vec());
    }
    if src.is_empty() {
        return Ok(Vec::new());
    }

    let ch = channels as usize;
    let frame_count = src.len() / ch;
    if frame_count == 0 {
        return Ok(Vec::new());
    }

    let waves_in: Vec<Vec<f32>> = (0..ch)
        .map(|c| {
            (0..frame_count)
                .map(|f| src[f * ch + c] as f32 / 32768.0)
                .collect()
        })
        .collect();

    let ratio = dst_rate as f64 / src_rate as f64;

    let mut resampler =
        FastFixedIn::<f32>::new(ratio, 2.0, PolynomialDegree::Septic, frame_count, ch)
            .map_err(|e| AudioError::Resample(e.to_string()))?;

    let waves_out = resampler
        .process(&waves_in, None)
        .map_err(|e| AudioError::Resample(e.to_string()))?;

    let out_frames = waves_out[0].len();
    let mut out = Vec::with_capacity(out_frames * ch);
    for frame_idx in 0..out_frames {
        for channel in &waves_out {
            let s = channel[frame_idx].clamp(-1.0, 1.0);
            out.push((s * 32767.0) as i16);
        }
    }

    Ok(out)
}

/// Remix interleaved PCM from `src_channels` to `dst_channels`.
///
/// - mono → any: duplicate the single channel into all outputs.
/// - any → mono: average all source channels per frame.
/// - src_channels < dst_channels: pad missing channels with silence.
/// - src_channels > dst_channels: truncate to the first `dst_channels`.
pub fn remix(src: &[i16], src_channels: u16, dst_channels: u16) -> Vec<i16> {
    if src_channels == dst_channels {
        return src.to_vec();
    }
    if src.is_empty() {
        return Vec::new();
    }

    let src_ch = src_channels as usize;
    let dst_ch = dst_channels as usize;
    let frames = src.len() / src_ch;
    let mut out = Vec::with_capacity(frames * dst_ch);

    for frame in src.chunks_exact(src_ch) {
        for d in 0..dst_ch {
            let sample = if dst_ch == 1 {
                let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                (sum / src_ch as i32) as i16
            } else if src_ch == 1 {
                frame[0]
            } else if d < src_ch {
                frame[d]
            } else {
                0
            };
            out.push(sample);
        }
    }

    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn resample_identity_returns_unchanged() {
        let input: Vec<i16> = (0..100).map(|i| i as i16).collect();
        let output = resample(&input, 44_100, 44_100, 1).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn resample_22050_mono_to_48000_stereo_length_sanity() {
        let frames = 2205_usize;
        let input: Vec<i16> = (0..frames).map(|i| i as i16).collect();

        let resampled = resample(&input, 22_050, 48_000, 1).unwrap();
        let remixed = remix(&resampled, 1, 2);

        let expected_frames = (frames as f64 * 48_000.0 / 22_050.0) as usize;
        let actual_frames = remixed.len() / 2;

        let tolerance = (expected_frames as f64 * 0.02) as usize + 2;
        assert!(
            actual_frames.abs_diff(expected_frames) <= tolerance,
            "expected ~{} frames, got {}",
            expected_frames,
            actual_frames,
        );
    }

    #[test]
    fn remix_mono_to_stereo_duplicates() {
        let src: Vec<i16> = vec![100, 200, 300];
        let out = remix(&src, 1, 2);
        assert_eq!(out, vec![100, 100, 200, 200, 300, 300]);
    }

    #[test]
    fn remix_stereo_to_mono_averages() {
        let src: Vec<i16> = vec![100, 200, 300, 400];
        let out = remix(&src, 2, 1);
        assert_eq!(out, vec![150, 350]);
    }

    #[test]
    fn remix_pads_missing_channels_with_silence() {
        let src: Vec<i16> = vec![100, 200];
        let out = remix(&src, 2, 4);
        assert_eq!(out, vec![100, 200, 0, 0]);
    }

    #[test]
    fn remix_truncates_excess_channels() {
        // 2 frames × 6 channels → 2 frames × 2 channels (keep first two channels)
        let src: Vec<i16> = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let out = remix(&src, 6, 2);
        assert_eq!(out, vec![10, 20, 70, 80]);
    }

    #[test]
    fn remix_identity_returns_unchanged() {
        let src: Vec<i16> = vec![1, 2, 3, 4];
        let out = remix(&src, 2, 2);
        assert_eq!(out, src);
    }
}
