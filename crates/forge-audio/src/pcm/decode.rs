use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::AudioError;
use forge_speech_types::PcmBuffer;

const MILLIS_PER_SECOND: f32 = 1000.0;

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

pub fn probe_duration_secs(path: &Path) -> Result<f32, AudioError> {
    if let Some(secs) = light_probe_duration_secs(path)? {
        return Ok(secs);
    }
    let pcm = decode_file(path)?;
    Ok(pcm.duration_ms() as f32 / MILLIS_PER_SECOND)
}

fn light_probe_duration_secs(path: &Path) -> Result<Option<f32>, AudioError> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format = symphonia::default::get_probe()
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

    Ok(match (track.time_base, track.duration) {
        (Some(time_base), Some(duration)) => time_base
            .calc_time(symphonia::core::units::Timestamp::new(
                i64::try_from(duration.get()).unwrap_or(i64::MAX),
            ))
            .map(|t| t.as_secs_f64() as f32),
        _ => None,
    })
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
        .map(|c| u16::try_from(c.count()).unwrap_or(u16::MAX))
        .ok_or_else(|| AudioError::Decode("track has no channel info".to_string()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|e| AudioError::Decode(e.to_string()))?;

    let mut all_samples: Vec<i16> = Vec::new();
    let mut interleaved_buf: Vec<i16> = Vec::new();

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
                all_samples.extend_from_slice(&interleaved_buf);
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(AudioError::Decode(e.to_string())),
        }
    }

    Ok(PcmBuffer::new(all_samples, sample_rate, channels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcm::wave::encode_wave;

    const ROUNDING_TOLERANCE: u16 = 1;

    fn ramp(sample_rate: u32, channels: u16, duration_ms: u64) -> PcmBuffer {
        let frames = u64::from(sample_rate) * duration_ms / 1_000;
        let samples = (0..frames * u64::from(channels))
            .map(|index| (index % 2_048) as i16 - 1_024)
            .collect();
        PcmBuffer::new(samples, sample_rate, channels)
    }

    #[test]
    fn wav_decodes_to_the_encoded_samples_rate_and_channels_from_bytes_and_from_a_file() {
        for (sample_rate, channels, duration_ms) in [
            (22_050_u32, 1_u16, 100_u64),
            (24_000, 1, 50),
            (44_100, 2, 500),
        ] {
            let source = ramp(sample_rate, channels, duration_ms);
            let wav = encode_wave(&source);
            let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
            std::fs::write(file.path(), &wav).unwrap();

            for decoded in [
                decode_bytes(&wav, Some("wav")).unwrap(),
                decode_file(file.path()).unwrap(),
            ] {
                assert_eq!(decoded.sample_rate, sample_rate);
                assert_eq!(decoded.channels, channels);
                assert_eq!(decoded.samples.len(), source.samples.len());
                assert!(
                    decoded
                        .samples
                        .iter()
                        .zip(&source.samples)
                        .all(|(got, want)| got.abs_diff(*want) <= ROUNDING_TOLERANCE),
                    "{sample_rate} Hz samples drifted from the source"
                );
            }
        }
    }

    #[test]
    fn decoded_pcm16_keeps_every_sample_exactly() {
        let samples = [
            i16::MIN,
            -1024,
            -2,
            -1,
            0,
            1,
            2,
            1024,
            16_384,
            i16::MAX - 1,
            i16::MAX,
        ];
        let wav_bytes = encode_wave(&PcmBuffer::new(samples.to_vec(), 24_000, 1));

        let pcm = decode_bytes(&wav_bytes, Some("wav")).unwrap();

        assert_eq!(pcm.samples, samples);
    }

    #[test]
    fn decode_missing_file_returns_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = decode_file(&dir.path().join("audio.wav"));
        assert!(matches!(result, Err(AudioError::Io(_))), "got {result:?}");
    }

    #[test]
    fn decode_of_a_file_that_is_not_audio_returns_decode_error() {
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(file.path(), b"not a wav file at all").unwrap();
        let result = decode_file(file.path());
        assert!(
            matches!(result, Err(AudioError::Decode(_))),
            "got {result:?}"
        );
    }
    #[test]
    fn probing_a_wave_file_reports_its_duration_without_decoding_the_samples() {
        const EXPECTED_SECS: f32 = 0.5;
        const TOLERANCE_SECS: f32 = 0.01;
        let source = ramp(22_050, 1, 500);
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(file.path(), encode_wave(&source)).unwrap();

        let secs = probe_duration_secs(file.path()).unwrap();

        assert!(
            (secs - EXPECTED_SECS).abs() < TOLERANCE_SECS,
            "probed {secs}s for a {EXPECTED_SECS}s clip"
        );
    }

    #[test]
    fn probing_a_file_that_is_not_audio_reports_a_decode_error() {
        let file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(file.path(), b"not a wav file at all").unwrap();

        assert!(matches!(
            probe_duration_secs(file.path()),
            Err(AudioError::Decode(_))
        ));
    }
}
