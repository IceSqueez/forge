use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::AudioError;
use crate::pcm::PcmBuffer;

pub fn decode_file(path: &Path) -> Result<PcmBuffer, AudioError> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    decode_stream(mss, hint)
}

pub fn decode_bytes(bytes: &[u8], hint_ext: Option<&str>) -> Result<PcmBuffer, AudioError> {
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = hint_ext {
        hint.with_extension(ext);
    }

    decode_stream(mss, hint)
}

fn decode_stream(mss: MediaSourceStream, hint: Hint) -> Result<PcmBuffer, AudioError> {
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Decode(e.to_string()))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| AudioError::Decode("no decodable audio track found".to_string()))?;

    let track_id = track.id;
    let codec_params = track
        .codec_params
        .as_ref()
        .ok_or_else(|| AudioError::Decode("track has no codec parameters".to_string()))?
        .audio()
        .ok_or_else(|| AudioError::Decode("track is not audio".to_string()))?;

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| AudioError::Decode("track has no sample rate".to_string()))?;
    let channels = codec_params
        .channels
        .as_ref()
        .map(|c| c.count() as u16)
        .ok_or_else(|| AudioError::Decode("track has no channel info".to_string()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|e| AudioError::Decode(e.to_string()))?;

    let mut all_samples: Vec<i16> = Vec::new();
    let mut interleaved_buf: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        };

        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                interleaved_buf.clear();
                decoded.copy_to_vec_interleaved(&mut interleaved_buf);
                for &s in interleaved_buf.iter() {
                    all_samples.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        }
    }

    Ok(PcmBuffer::new(all_samples, sample_rate, channels))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::cast_possible_truncation)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn write_wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
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
        let n = (sample_rate as u64 * duration_ms / 1000) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                let v = (2.0 * std::f64::consts::PI * freq_hz * t).sin();
                (v * 32767.0) as i16
            })
            .collect()
    }

    #[test]
    fn decode_22050_mono_wav_100ms() {
        let sample_rate = 22_050_u32;
        let freq = 1_000.0_f64;
        let samples = sine_samples(sample_rate, freq, 100);

        let wav_bytes = write_wav(sample_rate, 1, &samples);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &wav_bytes).unwrap();

        let pcm = decode_file(tmp.path()).unwrap();

        assert_eq!(pcm.sample_rate, sample_rate);
        assert_eq!(pcm.channels, 1);

        let expected_frames = samples.len();
        let tolerance = expected_frames / 20 + 2;
        assert!(
            pcm.frame_count().abs_diff(expected_frames) <= tolerance,
            "expected ~{} frames, got {}",
            expected_frames,
            pcm.frame_count(),
        );
    }

    #[test]
    fn decode_bytes_24000_mono_wav_50ms() {
        let sample_rate = 24_000_u32;
        let samples = sine_samples(sample_rate, 440.0, 50);
        let wav_bytes = write_wav(sample_rate, 1, &samples);

        let pcm = decode_bytes(&wav_bytes, Some("wav")).unwrap();

        assert_eq!(pcm.sample_rate, sample_rate);
        assert_eq!(pcm.channels, 1);

        let expected_frames = samples.len();
        let tolerance = expected_frames / 20 + 2;
        assert!(
            pcm.frame_count().abs_diff(expected_frames) <= tolerance,
            "expected ~{} frames, got {}",
            expected_frames,
            pcm.frame_count(),
        );
    }
}
