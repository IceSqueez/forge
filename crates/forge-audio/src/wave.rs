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
