use rubato::audioadapter::Adapter;
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Async, FixedAsync, PolynomialDegree, Resampler};

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

    if channels == 0 {
        return Err(AudioError::Resample("buffer has zero channels".into()));
    }
    let ch = usize::from(channels);
    let frame_count = src.len() / ch;
    if frame_count == 0 {
        return Ok(Vec::new());
    }

    let waves_in: Vec<Vec<f32>> = (0..ch)
        .map(|c| {
            (0..frame_count)
                .map(|f| f32::from(src[f * ch + c]) / 32768.0)
                .collect()
        })
        .collect();

    let ratio = f64::from(dst_rate) / f64::from(src_rate);

    let mut resampler = Async::<f32>::new_poly(
        ratio,
        2.0,
        PolynomialDegree::Septic,
        frame_count,
        ch,
        FixedAsync::Input,
    )
    .map_err(|e| AudioError::Resample(e.to_string()))?;

    let input_adapter = SequentialSliceOfVecs::new(waves_in.as_slice(), ch, frame_count)
        .map_err(|e| AudioError::Resample(e.to_string()))?;

    let output = resampler
        .process(&input_adapter, None)
        .map_err(|e| AudioError::Resample(e.to_string()))?;

    let out_frames = output.frames();
    let mut out = Vec::with_capacity(out_frames * ch);
    for frame_idx in 0..out_frames {
        for c in 0..ch {
            let s = output
                .read_sample(c, frame_idx)
                .unwrap_or(0.0)
                .clamp(-1.0, 1.0);
            out.push((s * 32767.0) as i16);
        }
    }

    Ok(out)
}

const STEREO: u16 = 2;
const MONO: u16 = 1;
const SIDE: f32 = 1.0;
const MIX: f32 = std::f32::consts::FRAC_1_SQRT_2;
const MUTE: f32 = 0.0;

type Gains = (f32, f32);

const LEFT: Gains = (SIDE, MUTE);
const RIGHT: Gains = (MUTE, SIDE);
const CENTER: Gains = (MIX, MIX);
const SURROUND_LEFT: Gains = (MIX, MUTE);
const SURROUND_RIGHT: Gains = (MUTE, MIX);
const LFE: Gains = (MUTE, MUTE);

fn stereo_downmix(src_channels: usize) -> Option<&'static [Gains]> {
    match src_channels {
        3 => Some(&[LEFT, RIGHT, CENTER]),
        4 => Some(&[LEFT, RIGHT, SURROUND_LEFT, SURROUND_RIGHT]),
        5 => Some(&[LEFT, RIGHT, CENTER, SURROUND_LEFT, SURROUND_RIGHT]),
        6 => Some(&[LEFT, RIGHT, CENTER, LFE, SURROUND_LEFT, SURROUND_RIGHT]),
        7 => Some(&[
            LEFT,
            RIGHT,
            CENTER,
            LFE,
            CENTER,
            SURROUND_LEFT,
            SURROUND_RIGHT,
        ]),
        8 => Some(&[
            LEFT,
            RIGHT,
            CENTER,
            LFE,
            SURROUND_LEFT,
            SURROUND_RIGHT,
            SURROUND_LEFT,
            SURROUND_RIGHT,
        ]),
        _ => None,
    }
}

fn downmix_frame_to_stereo(frame: &[i16], mix: &[Gains], norm: f32) -> [i16; 2] {
    let (left, right) = frame
        .iter()
        .zip(mix)
        .fold((0.0_f32, 0.0_f32), |(l, r), (&sample, &(gl, gr))| {
            (l + f32::from(sample) * gl, r + f32::from(sample) * gr)
        });
    let clamp = |v: f32| (v * norm).clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16;
    [clamp(left), clamp(right)]
}

pub fn remix(src: &[i16], src_channels: u16, dst_channels: u16) -> Vec<i16> {
    if src_channels == dst_channels {
        return src.to_vec();
    }
    if src.is_empty() || src_channels == 0 || dst_channels == 0 {
        return Vec::new();
    }

    let src_ch = usize::from(src_channels);
    let dst_ch = usize::from(dst_channels);
    let frames = src.len() / src_ch;
    let mut out = Vec::with_capacity(frames * dst_ch);

    if dst_channels == STEREO
        && let Some(mix) = stereo_downmix(src_ch)
    {
        let norm = 1.0 / mix.iter().map(|&(l, r)| l.max(r)).sum::<f32>().max(SIDE);
        for frame in src.chunks_exact(src_ch) {
            out.extend(downmix_frame_to_stereo(frame, mix, norm));
        }
        return out;
    }

    for frame in src.chunks_exact(src_ch) {
        for d in 0..dst_ch {
            let sample = if dst_channels == MONO {
                let sum: i32 = frame.iter().map(|&s| i32::from(s)).sum();
                (sum / i32::from(src_channels)) as i16
            } else if src_channels == MONO {
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
    fn remix_drops_trailing_channels_when_no_stereo_downmix_applies() {
        let src: Vec<i16> = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let out = remix(&src, 4, 3);
        assert_eq!(out, vec![10, 20, 30, 50, 60, 70]);
    }

    #[test]
    fn remix_to_stereo_from_a_layout_without_a_matrix_keeps_the_first_two_channels() {
        let src: Vec<i16> = (1..=9).collect();
        let out = remix(&src, 9, 2);
        assert_eq!(out, vec![1, 2]);
    }

    const FULL: i16 = 10_000;

    fn impulse_frame(channels: usize, hot: usize) -> Vec<i16> {
        let mut frame = vec![0; channels];
        frame[hot] = FULL;
        frame
    }

    fn assert_near(actual: i16, expected: f32, context: &str) {
        assert!(
            (f32::from(actual) - expected).abs() <= 1.0,
            "{context}: got {actual}, expected about {expected}"
        );
    }

    #[test]
    fn remix_5_1_to_stereo_routes_each_channel_to_its_side_with_center_split() {
        let norm = 1.0 / (1.0 + 1.0 + 3.0 * std::f32::consts::FRAC_1_SQRT_2);
        let mix = std::f32::consts::FRAC_1_SQRT_2;
        let full = f32::from(FULL);
        let expected: [(&str, usize, f32, f32); 6] = [
            ("front left", 0, full * norm, 0.0),
            ("front right", 1, 0.0, full * norm),
            ("center", 2, full * mix * norm, full * mix * norm),
            ("lfe", 3, 0.0, 0.0),
            ("surround left", 4, full * mix * norm, 0.0),
            ("surround right", 5, 0.0, full * mix * norm),
        ];
        for (name, hot, want_left, want_right) in expected {
            let out = remix(&impulse_frame(6, hot), 6, 2);
            assert_eq!(out.len(), 2, "{name}");
            assert_near(out[0], want_left, &format!("{name} left"));
            assert_near(out[1], want_right, &format!("{name} right"));
        }
    }

    #[test]
    fn remix_5_1_to_stereo_never_clips_when_every_channel_is_at_full_scale() {
        let out = remix(&[i16::MAX; 6], 6, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], out[1]);
        assert!(out[0] > 0 && out[0] < i16::MAX);
    }

    #[test]
    fn remix_7_1_rear_channels_fold_to_their_own_side() {
        let left_rear = remix(&impulse_frame(8, 6), 8, 2);
        let right_rear = remix(&impulse_frame(8, 7), 8, 2);
        assert!(left_rear[0] > 0 && left_rear[1] == 0);
        assert!(right_rear[0] == 0 && right_rear[1] > 0);
    }

    #[test]
    fn remix_3_channels_to_stereo_folds_the_third_into_both_sides() {
        let out = remix(&impulse_frame(3, 2), 3, 2);
        assert!(out[0] > 0);
        assert_eq!(out[0], out[1]);
    }

    #[test]
    fn remix_with_zero_channels_on_either_side_yields_nothing() {
        assert!(remix(&[1, 2, 3, 4], 0, 2).is_empty());
        assert!(remix(&[1, 2, 3, 4], 2, 0).is_empty());
    }

    #[test]
    fn resample_with_zero_channels_is_rejected_when_the_rates_differ() {
        let result = resample(&[1, 2, 3, 4], 22_050, 48_000, 0);
        assert!(matches!(result, Err(AudioError::Resample(_))));
    }

    #[test]
    fn remix_identity_returns_unchanged() {
        let src: Vec<i16> = vec![1, 2, 3, 4];
        let out = remix(&src, 2, 2);
        assert_eq!(out, src);
    }
}
