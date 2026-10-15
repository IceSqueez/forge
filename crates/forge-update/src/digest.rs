use std::fmt;

use sha2::{Digest, Sha256};

const SHA256_LEN: usize = 32;
const HEX_RADIX: u32 = 16;
const HEX_DIGITS_PER_BYTE: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; SHA256_LEN]);

impl Sha256Digest {
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        if hex.len() != SHA256_LEN * HEX_DIGITS_PER_BYTE || !hex.is_ascii() {
            return None;
        }
        let mut bytes = [0u8; SHA256_LEN];
        for (slot, pair) in bytes
            .iter_mut()
            .zip(hex.as_bytes().chunks(HEX_DIGITS_PER_BYTE))
        {
            let pair = std::str::from_utf8(pair).ok()?;
            *slot = u8::from_str_radix(pair, HEX_RADIX).ok()?;
        }
        Some(Self(bytes))
    }

    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = StreamingSha256::default();
        hasher.update(bytes);
        hasher.finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Digest({self})")
    }
}

#[derive(Default)]
pub(crate) struct StreamingSha256(Sha256);

impl StreamingSha256 {
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub(crate) fn finish(self) -> Sha256Digest {
        Sha256Digest(self.0.finalize().into())
    }
}
