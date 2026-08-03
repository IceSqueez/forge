use serde::{Deserialize, Serialize};

/// Interleaved: `samples.len() == frames * channels`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcmBuffer {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl PcmBuffer {
    pub fn new(samples: Vec<i16>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        let frames = self.frame_count() as u64;
        frames * 1000 / u64::from(self.sample_rate)
    }

    pub fn apply_gain(&mut self, gain: f32) {
        if (gain - 1.0_f32).abs() < f32::EPSILON {
            return;
        }
        for s in &mut self.samples {
            *s = (*s as f32 * gain).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }

    /// `secs == 0` is a no-op (caller's cap-disabled sentinel), not a truncation to silence.
    pub fn truncate_to_secs(&mut self, secs: u32) {
        if secs == 0 || self.sample_rate == 0 || self.channels == 0 {
            return;
        }
        let max_frames = u64::from(secs) * u64::from(self.sample_rate);
        let max_samples = max_frames.saturating_mul(u64::from(self.channels));
        let max_samples = max_samples.min(self.samples.len() as u64) as usize;
        self.samples.truncate(max_samples);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_stereo() {
        let buf = PcmBuffer::new(vec![0; 100], 44_100, 2);
        assert_eq!(buf.frame_count(), 50);
    }

    #[test]
    fn frame_count_mono() {
        let buf = PcmBuffer::new(vec![0; 100], 44_100, 1);
        assert_eq!(buf.frame_count(), 100);
    }

    #[test]
    fn frame_count_zero_channels_is_zero() {
        let buf = PcmBuffer::new(vec![0; 100], 44_100, 0);
        assert_eq!(buf.frame_count(), 0);
    }

    #[test]
    fn duration_ms_one_second() {
        let buf = PcmBuffer::new(vec![0; 88_200], 44_100, 2);
        assert_eq!(buf.duration_ms(), 1_000);
    }

    #[test]
    fn duration_ms_zero_sample_rate_is_zero() {
        let buf = PcmBuffer::new(vec![0; 100], 0, 2);
        assert_eq!(buf.duration_ms(), 0);
    }
}
