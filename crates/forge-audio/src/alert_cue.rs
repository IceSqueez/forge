use std::f64::consts::TAU;

use crate::pcm::PcmBuffer;

const SAMPLE_RATE_HZ: u32 = 44_100;
const CHANNELS: u16 = 2;
const FIRST_TONE_HZ: f64 = 880.0;
const SECOND_TONE_HZ: f64 = 1_174.66;
const TONE_DURATION_MS: u32 = 110;
const FADE_DURATION_MS: u32 = 12;
const AMPLITUDE: f64 = 0.22;
const MILLIS_PER_SECOND: u32 = 1_000;

pub fn alert_cue() -> PcmBuffer {
    let mut samples = Vec::new();
    for frequency_hz in [FIRST_TONE_HZ, SECOND_TONE_HZ] {
        append_tone(&mut samples, frequency_hz);
    }
    PcmBuffer::new(samples, SAMPLE_RATE_HZ, CHANNELS)
}

fn frames_for(duration_ms: u32) -> usize {
    (u64::from(SAMPLE_RATE_HZ) * u64::from(duration_ms) / u64::from(MILLIS_PER_SECOND)) as usize
}

fn append_tone(samples: &mut Vec<i16>, frequency_hz: f64) {
    let total_frames = frames_for(TONE_DURATION_MS);
    let fade_frames = frames_for(FADE_DURATION_MS).max(1);
    for frame in 0..total_frames {
        let phase = TAU * frequency_hz * frame as f64 / f64::from(SAMPLE_RATE_HZ);
        let fade_in = (frame as f64 / fade_frames as f64).min(1.0);
        let fade_out = ((total_frames - 1 - frame) as f64 / fade_frames as f64).min(1.0);
        let value = phase.sin() * AMPLITUDE * fade_in * fade_out;
        let sample = (value * f64::from(i16::MAX)) as i16;
        for _ in 0..CHANNELS {
            samples.push(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TONE_COUNT: usize = 2;
    const ZERO_CROSSING_TOLERANCE: usize = 4;

    fn tone_frames() -> usize {
        (SAMPLE_RATE_HZ * TONE_DURATION_MS / MILLIS_PER_SECOND) as usize
    }

    fn fade_frames() -> usize {
        (SAMPLE_RATE_HZ * FADE_DURATION_MS / MILLIS_PER_SECOND) as usize
    }

    fn left_channel(buffer: &PcmBuffer) -> Vec<i16> {
        buffer
            .samples
            .chunks(usize::from(CHANNELS))
            .map(|frame| frame[0])
            .collect()
    }

    fn amplitude_cap() -> i16 {
        (AMPLITUDE * f64::from(i16::MAX)) as i16
    }

    fn sign_changes(segment: &[i16]) -> usize {
        let mut changes = 0;
        let mut previous_positive: Option<bool> = None;
        for &sample in segment.iter().filter(|&&sample| sample != 0) {
            let positive = sample > 0;
            if previous_positive.is_some_and(|previous| previous != positive) {
                changes += 1;
            }
            previous_positive = Some(positive);
        }
        changes
    }

    fn expected_sign_changes(frequency_hz: f64) -> usize {
        let duration_secs = f64::from(TONE_DURATION_MS) / f64::from(MILLIS_PER_SECOND);
        (2.0 * frequency_hz * duration_secs).round() as usize
    }

    #[test]
    fn cue_is_stereo_at_configured_rate_and_two_tones_long() {
        let cue = alert_cue();
        assert_eq!(
            (cue.sample_rate, cue.channels, cue.samples.len()),
            (
                SAMPLE_RATE_HZ,
                CHANNELS,
                TONE_COUNT * tone_frames() * usize::from(CHANNELS)
            )
        );
    }

    #[test]
    fn every_frame_carries_the_same_sample_on_both_channels() {
        let cue = alert_cue();
        assert!(
            cue.samples
                .chunks(usize::from(CHANNELS))
                .all(|frame| frame.iter().all(|&sample| sample == frame[0]))
        );
    }

    #[test]
    fn peak_never_exceeds_amplitude_cap() {
        let cap = amplitude_cap();
        let peak = alert_cue()
            .samples
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(peak <= cap.unsigned_abs(), "peak {peak} over cap {cap}");
    }

    #[test]
    fn tone_body_reaches_close_to_amplitude_cap() {
        let floor = u32::from(amplitude_cap().unsigned_abs()) * 95 / 100;
        let left = left_channel(&alert_cue());
        for tone in left.chunks(tone_frames()) {
            let body_peak = tone[fade_frames()..tone.len() - fade_frames()]
                .iter()
                .map(|sample| u32::from(sample.unsigned_abs()))
                .max()
                .unwrap_or(0);
            assert!(body_peak >= floor, "body peak {body_peak} below {floor}");
        }
    }

    #[test]
    fn cue_starts_and_ends_on_silence() {
        let left = left_channel(&alert_cue());
        assert_eq!((left.first(), left.last()), (Some(&0), Some(&0)));
    }

    #[test]
    fn fade_in_and_fade_out_ramp_linearly_within_cap() {
        let cap = f64::from(amplitude_cap());
        let fade = fade_frames() as f64;
        let left = left_channel(&alert_cue());
        for tone in left.chunks(tone_frames()) {
            let last = tone.len() - 1;
            for (frame, &sample) in tone.iter().enumerate() {
                let envelope = (frame as f64 / fade)
                    .min((last - frame) as f64 / fade)
                    .min(1.0);
                assert!(
                    f64::from(sample.unsigned_abs()) <= cap * envelope + 1.0,
                    "frame {frame} sample {sample} above envelope {envelope}"
                );
            }
        }
    }

    #[test]
    fn tone_boundary_is_silent_on_both_sides() {
        let left = left_channel(&alert_cue());
        let boundary = tone_frames();
        assert_eq!((left[boundary - 1], left[boundary]), (0, 0));
    }

    #[test]
    fn each_tone_oscillates_at_its_configured_frequency() {
        let left = left_channel(&alert_cue());
        let tones: Vec<&[i16]> = left.chunks(tone_frames()).collect();
        for (tone, frequency_hz) in tones.into_iter().zip([FIRST_TONE_HZ, SECOND_TONE_HZ]) {
            let measured = sign_changes(tone);
            let expected = expected_sign_changes(frequency_hz);
            assert!(
                measured.abs_diff(expected) <= ZERO_CROSSING_TOLERANCE,
                "{frequency_hz} Hz: {measured} sign changes, expected about {expected}"
            );
        }
    }
}
