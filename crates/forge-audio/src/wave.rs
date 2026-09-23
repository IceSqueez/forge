use crate::pcm::PcmBuffer;

const RIFF_TAG: &[u8; 4] = b"RIFF";
const WAVE_TAG: &[u8; 4] = b"WAVE";
const FMT_TAG: &[u8; 4] = b"fmt ";
const DATA_TAG: &[u8; 4] = b"data";

const CHUNK_HEADER_BYTES: u32 = 8;
const FORM_TYPE_BYTES: u32 = 4;
const FMT_BODY_BYTES: u32 = 16;
const FORMAT_TAG_PCM: u16 = 1;
const BYTES_PER_SAMPLE: u16 = 2;
const BITS_PER_BYTE: u16 = 8;
const BITS_PER_SAMPLE: u16 = BYTES_PER_SAMPLE * BITS_PER_BYTE;

const HEADER_BYTES: usize = (CHUNK_HEADER_BYTES
    + FORM_TYPE_BYTES
    + CHUNK_HEADER_BYTES
    + FMT_BODY_BYTES
    + CHUNK_HEADER_BYTES) as usize;

pub fn encode_wave(buffer: &PcmBuffer) -> Vec<u8> {
    let data_len = buffer
        .samples
        .len()
        .saturating_mul(usize::from(BYTES_PER_SAMPLE));
    let data_bytes = declared_len(data_len);
    let riff_bytes = FORM_TYPE_BYTES
        .saturating_add(CHUNK_HEADER_BYTES)
        .saturating_add(FMT_BODY_BYTES)
        .saturating_add(CHUNK_HEADER_BYTES)
        .saturating_add(data_bytes);

    let block_align = buffer.channels.saturating_mul(BYTES_PER_SAMPLE);
    let byte_rate = buffer.sample_rate.saturating_mul(u32::from(block_align));

    let mut out = Vec::with_capacity(HEADER_BYTES.saturating_add(data_len));
    out.extend_from_slice(RIFF_TAG);
    out.extend_from_slice(&riff_bytes.to_le_bytes());
    out.extend_from_slice(WAVE_TAG);
    out.extend_from_slice(FMT_TAG);
    out.extend_from_slice(&FMT_BODY_BYTES.to_le_bytes());
    out.extend_from_slice(&FORMAT_TAG_PCM.to_le_bytes());
    out.extend_from_slice(&buffer.channels.to_le_bytes());
    out.extend_from_slice(&buffer.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    out.extend_from_slice(DATA_TAG);
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in &buffer.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn declared_len(bytes: usize) -> u32 {
    u32::try_from(bytes).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
pub(crate) mod tests {
    use super::*;

    const RIFF: &[u8; 4] = b"RIFF";
    const WAVE: &[u8; 4] = b"WAVE";
    const FMT: &[u8; 4] = b"fmt ";
    const DATA: &[u8; 4] = b"data";
    const WAVE_FORMAT_PCM: u16 = 1;
    const CANONICAL_HEADER_BYTES: usize = 44;
    const CHUNK_HEADER: usize = 8;
    const BITS_IN_A_BYTE: usize = 8;

    const TAG_BYTES: usize = size_of::<[u8; 4]>();
    const U16_BYTES: usize = size_of::<u16>();
    const U32_BYTES: usize = size_of::<u32>();

    pub(crate) struct WaveHeader {
        pub(crate) riff_tag: [u8; 4],
        pub(crate) riff_size: u32,
        pub(crate) wave_tag: [u8; 4],
        pub(crate) fmt_tag: [u8; 4],
        pub(crate) fmt_size: u32,
        pub(crate) fmt_body_at: usize,
        pub(crate) format_tag: u16,
        pub(crate) channels: u16,
        pub(crate) sample_rate: u32,
        pub(crate) byte_rate: u32,
        pub(crate) block_align: u16,
        pub(crate) bits_per_sample: u16,
        pub(crate) data_tag: [u8; 4],
        pub(crate) data_size: u32,
    }

    struct Reader<'a> {
        bytes: &'a [u8],
        at: usize,
    }

    impl Reader<'_> {
        fn tag(&mut self) -> [u8; 4] {
            let out = self.bytes[self.at..self.at + TAG_BYTES].try_into().unwrap();
            self.at += TAG_BYTES;
            out
        }

        fn u16(&mut self) -> u16 {
            let out =
                u16::from_le_bytes(self.bytes[self.at..self.at + U16_BYTES].try_into().unwrap());
            self.at += U16_BYTES;
            out
        }

        fn u32(&mut self) -> u32 {
            let out =
                u32::from_le_bytes(self.bytes[self.at..self.at + U32_BYTES].try_into().unwrap());
            self.at += U32_BYTES;
            out
        }
    }

    pub(crate) fn parse_header(wave: &[u8]) -> WaveHeader {
        let mut r = Reader { bytes: wave, at: 0 };
        let riff_tag = r.tag();
        let riff_size = r.u32();
        let wave_tag = r.tag();
        let fmt_tag = r.tag();
        let fmt_size = r.u32();
        let fmt_body_at = r.at;
        WaveHeader {
            riff_tag,
            riff_size,
            wave_tag,
            fmt_tag,
            fmt_size,
            fmt_body_at,
            format_tag: r.u16(),
            channels: r.u16(),
            sample_rate: r.u32(),
            byte_rate: r.u32(),
            block_align: r.u16(),
            bits_per_sample: r.u16(),
            data_tag: r.tag(),
            data_size: r.u32(),
        }
    }

    fn shapes() -> Vec<PcmBuffer> {
        vec![
            PcmBuffer::new(Vec::new(), 22_050, 1),
            PcmBuffer::new(vec![7, -7, 3], 44_100, 1),
            PcmBuffer::new(vec![1, 2, 3, 4], 48_000, 2),
        ]
    }

    #[test]
    fn the_chunk_layout_is_the_one_a_wave_decoder_walks() {
        for buffer in shapes() {
            let wave = encode_wave(&buffer);
            let h = parse_header(&wave);
            let shape = format!("{} samples", buffer.samples.len());

            assert_eq!(&h.riff_tag, RIFF, "{shape}");
            assert_eq!(&h.wave_tag, WAVE, "{shape}");
            assert_eq!(&h.fmt_tag, FMT, "{shape}");
            assert_eq!(&h.data_tag, DATA, "{shape}");
            assert_eq!(
                h.format_tag, WAVE_FORMAT_PCM,
                "{shape}: only uncompressed PCM is announced"
            );
            assert_eq!(
                &wave[h.fmt_body_at + h.fmt_size as usize..][..TAG_BYTES],
                DATA,
                "{shape}: the declared fmt size must land the decoder on the data chunk"
            );
        }
    }

    #[test]
    fn the_format_chunk_describes_the_buffer_the_engine_produced() {
        for buffer in shapes() {
            let wave = encode_wave(&buffer);
            let h = parse_header(&wave);
            let shape = format!("{} Hz, {} channels", buffer.sample_rate, buffer.channels);

            assert_eq!(
                h.sample_rate, buffer.sample_rate,
                "{shape} must reach the page at its own rate"
            );
            assert_eq!(
                h.channels, buffer.channels,
                "{shape} must reach the page unmixed"
            );
            assert_eq!(
                usize::from(h.bits_per_sample) / BITS_IN_A_BYTE * buffer.samples.len(),
                wave.len() - CANONICAL_HEADER_BYTES,
                "{shape}: the declared bit depth must match the bytes actually written"
            );
            assert_eq!(
                usize::from(h.block_align),
                usize::from(buffer.channels) * usize::from(h.bits_per_sample) / BITS_IN_A_BYTE,
                "{shape}: a frame is one sample per channel"
            );
            assert_eq!(
                h.byte_rate,
                buffer.sample_rate * u32::from(h.block_align),
                "{shape}: the byte rate is what the page plays the clip back at"
            );
        }
    }

    #[test]
    fn the_declared_sizes_account_for_exactly_the_bytes_that_follow_them() {
        for buffer in shapes() {
            let wave = encode_wave(&buffer);
            let h = parse_header(&wave);
            let payload = wave.len() - CANONICAL_HEADER_BYTES;
            let shape = format!("{} samples", buffer.samples.len());

            assert_eq!(
                h.riff_size as usize,
                wave.len() - CHUNK_HEADER,
                "{shape}: the RIFF size counts everything after its own field"
            );
            assert_eq!(
                h.data_size as usize, payload,
                "{shape}: the data size counts the payload alone"
            );
        }
    }

    #[test]
    fn the_payload_carries_every_sample_in_order_as_little_endian_pairs() {
        let samples = vec![0, 1, -1, i16::MIN, i16::MAX, -12_345];
        let buffer = PcmBuffer::new(samples.clone(), 16_000, 1);

        let wave = encode_wave(&buffer);
        let decoded: Vec<i16> = wave[CANONICAL_HEADER_BYTES..]
            .chunks_exact(size_of::<i16>())
            .map(|pair| i16::from_le_bytes(pair.try_into().unwrap()))
            .collect();

        assert_eq!(
            decoded, samples,
            "the page must hear the samples the engine produced, in order"
        );
    }
}
