use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::error::TransferError;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

pub(crate) type OuterArchive = tar::Archive<GzDecoder<BufReader<File>>>;

pub(crate) fn open_outer(path: &Path) -> Result<OuterArchive, TransferError> {
    let mut file = File::open(path)?;
    let mut magic = [0_u8; GZIP_MAGIC.len()];
    if file.read_exact(&mut magic).is_err() || magic != GZIP_MAGIC {
        return Err(TransferError::NotABundle);
    }
    let file = File::open(path)?;
    Ok(tar::Archive::new(GzDecoder::new(BufReader::new(file))))
}

pub(crate) fn read_bounded(
    entry: impl Read,
    size: u64,
    limit: u64,
) -> Result<Vec<u8>, TransferError> {
    if size > limit {
        return Err(TransferError::damaged("the manifest is too large"));
    }
    let mut bytes = Vec::new();
    entry
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(TransferError::damaged)?;
    if bytes.len() as u64 != size {
        return Err(TransferError::damaged("the manifest is truncated"));
    }
    Ok(bytes)
}
