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
