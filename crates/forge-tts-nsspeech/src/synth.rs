/// Maps a rate multiplier ([0.25, 4.0]) to AVFoundation's [0.0, 1.0] speech rate scale.
///
/// `AVSpeechUtteranceDefaultSpeechRate` (0.5) maps to multiplier 1.0.
/// Multipliers above 1.0 push toward `AVSpeechUtteranceMaximumSpeechRate` (1.0);
/// multipliers below 1.0 push toward `AVSpeechUtteranceMinimumSpeechRate` (0.0).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn rate_apple_from_multiplier(multiplier: f32) -> f32 {
    const RATE_DEFAULT: f32 = 0.5;
    const RATE_MAX: f32 = 1.0;
    const RATE_MIN: f32 = 0.0;
    (RATE_DEFAULT + (multiplier - 1.0) * (RATE_MAX - RATE_DEFAULT)).clamp(RATE_MIN, RATE_MAX)
}

/// Maps pitch semitones ([-12.0, 12.0]) to AVFoundation pitchMultiplier ([0.5, 2.0]).
///
/// Uses the standard semitone-to-frequency-ratio formula: `2^(semitones / 12)`.
/// The range [-12, +12] maps exactly to [0.5, 2.0] before clamping.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn pitch_mult_from_semitones(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0).clamp(0.5, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_neutral_multiplier_gives_default() {
        let r = rate_apple_from_multiplier(1.0);
        assert!((r - 0.5).abs() < 1e-6, "expected 0.5, got {r}");
    }

    #[test]
    fn rate_quarter_speed_clamped_to_min() {
        let r = rate_apple_from_multiplier(0.25);
        assert!((r - 0.125).abs() < 1e-6, "expected 0.125, got {r}");
    }

    #[test]
    fn rate_half_speed() {
        let r = rate_apple_from_multiplier(0.5);
        assert!((r - 0.25).abs() < 1e-6, "expected 0.25, got {r}");
    }

    #[test]
    fn rate_double_speed_gives_max() {
        let r = rate_apple_from_multiplier(2.0);
        assert!((r - 1.0).abs() < 1e-6, "expected 1.0, got {r}");
    }

    #[test]
    fn rate_quad_speed_clamped_to_max() {
        let r = rate_apple_from_multiplier(4.0);
        assert!((r - 1.0).abs() < 1e-6, "expected 1.0 (clamped), got {r}");
    }

    #[test]
    fn pitch_zero_semitones_gives_one() {
        let p = pitch_mult_from_semitones(0.0);
        assert!((p - 1.0).abs() < 1e-6, "expected 1.0, got {p}");
    }

    #[test]
    fn pitch_minus_twelve_semitones_gives_half() {
        let p = pitch_mult_from_semitones(-12.0);
        assert!((p - 0.5).abs() < 1e-6, "expected 0.5, got {p}");
    }

    #[test]
    fn pitch_plus_twelve_semitones_gives_two() {
        let p = pitch_mult_from_semitones(12.0);
        assert!((p - 2.0).abs() < 1e-6, "expected 2.0, got {p}");
    }

    #[test]
    fn pitch_minus_six_semitones_approx_sqrt_half() {
        let p = pitch_mult_from_semitones(-6.0);
        let expected = 2.0_f32.powf(-0.5);
        assert!((p - expected).abs() < 1e-6, "expected ~{expected}, got {p}");
    }

    #[test]
    fn pitch_plus_six_semitones_approx_sqrt_two() {
        let p = pitch_mult_from_semitones(6.0);
        let expected = 2.0_f32.powf(0.5);
        assert!((p - expected).abs() < 1e-6, "expected ~{expected}, got {p}");
    }
}
