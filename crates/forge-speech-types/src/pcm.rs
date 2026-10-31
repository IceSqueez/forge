use serde::{Deserialize, Serialize};

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

    pub fn from_s16le(bytes: &[u8], sample_rate: u32, channels: u16) -> Self {
        let (pairs, _) = bytes.as_chunks::<{ size_of::<i16>() }>();
        let samples = pairs.iter().copied().map(i16::from_le_bytes).collect();
        Self::new(samples, sample_rate, channels)
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
mod tests {
    use super::*;

    #[test]
    fn from_s16le_decodes_little_endian_pairs_in_order_and_drops_a_trailing_odd_byte() {
        let cases: [(&[u8], &[i16]); 4] = [
            (&[], &[]),
            (&[0x7f], &[]),
            (&[0x34, 0x12, 0xff, 0xff], &[0x1234, -1]),
            (&[0x00, 0x80, 0xff, 0x7f, 0x7f], &[i16::MIN, i16::MAX]),
        ];
        for (bytes, samples) in cases {
            let buffer = PcmBuffer::from_s16le(bytes, 24_000, 2);
            assert_eq!(buffer.samples, samples, "{bytes:?}");
            assert_eq!(buffer.sample_rate, 24_000);
            assert_eq!(buffer.channels, 2);
        }
    }

    #[test]
    fn frame_count_divides_interleaved_samples_by_channel_count() {
        for (channels, expected) in [(2u16, 50usize), (1, 100), (0, 0)] {
            let buf = PcmBuffer::new(vec![0; 100], 44_100, channels);
            assert_eq!(buf.frame_count(), expected, "channels={channels}");
        }
    }

    #[test]
    fn duration_ms_counts_frames_at_the_sample_rate_and_is_zero_without_a_rate() {
        for (samples, sample_rate, channels, expected) in [
            (88_200, 44_100_u32, 2_u16, 1_000_u64),
            (44_100, 44_100, 1, 1_000),
            (100, 0, 2, 0),
            (100, 44_100, 0, 0),
        ] {
            let buf = PcmBuffer::new(vec![0; samples], sample_rate, channels);
            assert_eq!(buf.duration_ms(), expected, "{samples} at {sample_rate}");
        }
    }

    #[test]
    fn apply_gain_saturates_at_the_i16_range_instead_of_wrapping() {
        let mut buf = PcmBuffer::new(vec![20_000, -20_000, 100], 8_000, 1);
        buf.apply_gain(2.0);
        assert_eq!(buf.samples, vec![i16::MAX, i16::MIN, 200]);
    }

    fn ramp(len: usize, sample_rate: u32, channels: u16) -> PcmBuffer {
        PcmBuffer::new((0..len).map(|i| i as i16).collect(), sample_rate, channels)
    }

    #[test]
    fn truncate_to_secs_cuts_at_the_cap_counting_every_interleaved_channel() {
        for (channels, initial, cap, expected) in [
            (1u16, 16_000usize, 1u32, 8_000usize),
            (1, 16_000, 2, 16_000),
            (1, 16_000, 3, 16_000),
            (1, 16_000, u32::MAX, 16_000),
            (2, 32_000, 1, 16_000),
            (2, 32_000, 2, 32_000),
            (2, 32_000, 3, 32_000),
            (2, 32_000, u32::MAX, 32_000),
        ] {
            let mut buf = ramp(initial, 8_000, channels);
            buf.truncate_to_secs(cap);
            assert_eq!(
                buf.samples.len(),
                expected,
                "cap {cap}s on a 2 s {channels}-channel buffer"
            );
            assert_eq!(
                buf.samples.last(),
                Some(&((expected - 1) as i16)),
                "cap {cap}s on {channels} channels must keep the head of the clip, not its tail"
            );
        }
    }

    #[test]
    fn truncate_to_secs_zero_leaves_the_buffer_intact() {
        let mut buf = ramp(16_000, 8_000, 1);
        buf.truncate_to_secs(0);
        assert_eq!(buf.samples.len(), 16_000);
    }

    #[test]
    fn truncate_to_secs_without_a_usable_rate_or_channel_count_is_a_noop() {
        for (sample_rate, channels) in [(0u32, 2u16), (8_000, 0)] {
            let mut buf = ramp(500, sample_rate, channels);
            buf.truncate_to_secs(1);
            assert_eq!(
                buf.samples.len(),
                500,
                "rate={sample_rate} channels={channels} carries no frame size to cut on"
            );
        }
    }
}
