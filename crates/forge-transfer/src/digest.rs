use std::fmt::Write as _;
use std::io::{self, Read};

use sha2::{Digest, Sha256};

pub(crate) struct DigestingReader<R> {
    inner: io::Take<R>,
    expected_len: u64,
    seen: u64,
    hasher: Sha256,
}

impl<R: Read> DigestingReader<R> {
    pub(crate) fn new(inner: R, expected_len: u64) -> Self {
        Self {
            inner: inner.take(expected_len),
            expected_len,
            seen: 0,
            hasher: Sha256::new(),
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.seen == self.expected_len
    }

    pub(crate) fn finish_hex(self) -> String {
        to_hex(&self.hasher.finalize())
    }
}

impl<R: Read> Read for DigestingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read == 0 && !buf.is_empty() && !self.is_complete() {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        self.hasher.update(&buf[..read]);
        self.seen += read as u64;
        Ok(read)
    }
}

const READ_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) fn digest_reader(mut reader: impl Read) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    let mut size = 0_u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, to_hex(&hasher.finalize())))
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}
