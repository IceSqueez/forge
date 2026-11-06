use bytes::{Buf, BufMut, Bytes, BytesMut};

const PREFIX_LEN: usize = 5;
const FLAG_UNCOMPRESSED: u8 = 0;
const FLAG_COMPRESSED: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FramingError {
    TooLarge { cap: u64 },
    Compressed,
    UnknownFlag,
}

pub(super) fn encode(message: &[u8]) -> Option<Bytes> {
    let length = u32::try_from(message.len()).ok()?;
    let mut framed = BytesMut::with_capacity(PREFIX_LEN.checked_add(message.len())?);
    framed.put_u8(FLAG_UNCOMPRESSED);
    framed.put_u32(length);
    framed.put_slice(message);
    Some(framed.freeze())
}

pub(super) struct MessageDecoder {
    buffer: BytesMut,
    cap: u64,
}

impl MessageDecoder {
    pub(super) fn new(cap: u64) -> Self {
        Self {
            buffer: BytesMut::new(),
            cap,
        }
    }

    pub(super) fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub(super) fn holds_partial_message(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub(super) fn next_message(&mut self) -> Result<Option<Vec<u8>>, FramingError> {
        let Some(&[flag, b0, b1, b2, b3]) = self.buffer.get(..PREFIX_LEN) else {
            return Ok(None);
        };
        match flag {
            FLAG_UNCOMPRESSED => {}
            FLAG_COMPRESSED => return Err(FramingError::Compressed),
            _ => return Err(FramingError::UnknownFlag),
        }
        let declared = u32::from_be_bytes([b0, b1, b2, b3]);
        let too_large = FramingError::TooLarge { cap: self.cap };
        if u64::from(declared) > self.cap {
            return Err(too_large);
        }
        let length = usize::try_from(declared).map_err(|_| too_large)?;
        let framed_length = PREFIX_LEN.checked_add(length).ok_or(too_large)?;
        if self.buffer.len() < framed_length {
            return Ok(None);
        }
        self.buffer.advance(PREFIX_LEN);
        Ok(Some(Vec::from(self.buffer.split_to(length).freeze())))
    }
}
