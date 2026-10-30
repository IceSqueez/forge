use crate::error::EspeakError;

const RIFF_TAG: &[u8; 4] = b"RIFF";
const WAVE_TAG: &[u8; 4] = b"WAVE";
const FMT_TAG: &[u8; 4] = b"fmt ";
const DATA_TAG: &[u8; 4] = b"data";
const RIFF_HEADER_LEN: usize = 12;
const CHUNK_HEADER_LEN: usize = 8;
const FMT_MIN_LEN: usize = 16;
const PCM_FORMAT: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;

pub(crate) struct WavAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

struct Format {
    sample_rate: u32,
    channels: u16,
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn parse_format(body: &[u8]) -> Option<Format> {
    if body.len() < FMT_MIN_LEN
        || read_u16(body, 0)? != PCM_FORMAT
        || read_u16(body, 14)? != BITS_PER_SAMPLE
    {
        return None;
    }
    let channels = read_u16(body, 2)?;
    if channels == 0 {
        return None;
    }
    Some(Format {
        sample_rate: read_u32(body, 4)?,
        channels,
    })
}

pub(crate) fn parse(bytes: &[u8]) -> Result<WavAudio, EspeakError> {
    if bytes.len() < RIFF_HEADER_LEN
        || &bytes[..4] != RIFF_TAG
        || &bytes[8..RIFF_HEADER_LEN] != WAVE_TAG
    {
        return Err(EspeakError::UnsupportedAudio);
    }
    let mut format = None;
    let mut cursor = RIFF_HEADER_LEN;
    while cursor + CHUNK_HEADER_LEN <= bytes.len() {
        let tag = &bytes[cursor..cursor + 4];
        let declared = read_u32(bytes, cursor + 4).ok_or(EspeakError::UnsupportedAudio)? as usize;
        let body_start = cursor + CHUNK_HEADER_LEN;
        let body_end = body_start.saturating_add(declared).min(bytes.len());
        if tag == FMT_TAG {
            format = parse_format(&bytes[body_start..body_end]);
        } else if tag == DATA_TAG {
            let format = format.ok_or(EspeakError::UnsupportedAudio)?;
            let (pairs, _) = bytes[body_start..body_end].as_chunks::<2>();
            return Ok(WavAudio {
                samples: pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect(),
                sample_rate: format.sample_rate,
                channels: format.channels,
            });
        }
        cursor = body_end + (declared & 1);
    }
    Err(EspeakError::UnsupportedAudio)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn fmt_body(format: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend(format.to_le_bytes());
        body.extend(channels.to_le_bytes());
        body.extend(rate.to_le_bytes());
        body.extend((rate * u32::from(channels) * u32::from(bits) / 8).to_le_bytes());
        body.extend((channels * bits / 8).to_le_bytes());
        body.extend(bits.to_le_bytes());
        body
    }

    fn wav(chunks: &[Vec<u8>]) -> Vec<u8> {
        let payload: Vec<u8> = chunks.concat();
        let mut out = b"RIFF".to_vec();
        out.extend((payload.len() as u32 + 4).to_le_bytes());
        out.extend(b"WAVE");
        out.extend(payload);
        out
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn mono_fmt(rate: u32) -> Vec<u8> {
        chunk(FMT_TAG, &fmt_body(1, 1, rate, 16))
    }

    #[test]
    fn parse_reads_samples_rate_and_channels_of_a_valid_wav() {
        let bytes = wav(&[
            chunk(FMT_TAG, &fmt_body(1, 2, 22_050, 16)),
            chunk(DATA_TAG, &pcm(&[1, -2, i16::MAX, i16::MIN])),
        ]);
        let audio = parse(&bytes).expect("valid wav");
        assert_eq!(audio.samples, vec![1, -2, i16::MAX, i16::MIN]);
        assert_eq!(audio.sample_rate, 22_050);
        assert_eq!(audio.channels, 2);
    }

    #[test]
    fn parse_honours_a_16_khz_sample_rate_instead_of_assuming_22_khz() {
        let bytes = wav(&[mono_fmt(16_000), chunk(DATA_TAG, &pcm(&[5, 6]))]);
        assert_eq!(parse(&bytes).expect("wav").sample_rate, 16_000);
    }

    #[test]
    fn parse_skips_unknown_chunks_around_fmt() {
        let bytes = wav(&[
            chunk(b"LIST", b"abc"),
            mono_fmt(22_050),
            chunk(b"fact", &[0, 0, 0, 0]),
            chunk(DATA_TAG, &pcm(&[7, 8])),
        ]);
        assert_eq!(parse(&bytes).expect("wav").samples, vec![7, 8]);
    }

    #[test]
    fn parse_drops_a_trailing_odd_byte_of_the_data_chunk() {
        let mut data = pcm(&[3, 4]);
        data.push(0x7f);
        let bytes = wav(&[mono_fmt(22_050), chunk(DATA_TAG, &data)]);
        assert_eq!(parse(&bytes).expect("wav").samples, vec![3, 4]);
    }

    #[test]
    fn parse_tolerates_a_data_length_larger_than_the_buffer() {
        let mut bytes = wav(&[mono_fmt(22_050)]);
        bytes.extend(b"data");
        bytes.extend(u32::MAX.to_le_bytes());
        bytes.extend(pcm(&[9, 10, 11]));
        assert_eq!(parse(&bytes).expect("wav").samples, vec![9, 10, 11]);
    }

    #[test]
    fn parse_accepts_an_empty_data_chunk() {
        let bytes = wav(&[mono_fmt(22_050), chunk(DATA_TAG, &[])]);
        assert!(parse(&bytes).expect("wav").samples.is_empty());
    }

    #[test]
    fn parse_rejects_input_that_is_not_a_riff_wave() {
        let mut wrong_form = wav(&[mono_fmt(22_050), chunk(DATA_TAG, &pcm(&[1]))]);
        wrong_form[8..12].copy_from_slice(b"AVI ");
        for bad in [
            Vec::new(),
            b"RIFF".to_vec(),
            b"OggS0000000000".to_vec(),
            b"not audio at all, just text".to_vec(),
            wrong_form,
        ] {
            assert!(matches!(parse(&bad), Err(EspeakError::UnsupportedAudio)));
        }
    }

    #[test]
    fn parse_rejects_unsupported_formats_and_missing_chunks() {
        let data = chunk(DATA_TAG, &pcm(&[1, 2]));
        let fmt =
            |format, channels, bits| chunk(FMT_TAG, &fmt_body(format, channels, 22_050, bits));
        let cases = [
            ("float", wav(&[fmt(3, 1, 16), data.clone()])),
            ("8-bit", wav(&[fmt(1, 1, 8), data.clone()])),
            ("24-bit", wav(&[fmt(1, 1, 24), data.clone()])),
            ("zero channels", wav(&[fmt(1, 0, 16), data.clone()])),
            (
                "short fmt",
                wav(&[chunk(FMT_TAG, &[1, 0, 1, 0]), data.clone()]),
            ),
            ("data before fmt", wav(&[data.clone(), fmt(1, 1, 16)])),
            ("no data", wav(&[fmt(1, 1, 16)])),
        ];
        for (name, bytes) in cases {
            assert!(
                matches!(parse(&bytes), Err(EspeakError::UnsupportedAudio)),
                "{name}"
            );
        }
    }
}
