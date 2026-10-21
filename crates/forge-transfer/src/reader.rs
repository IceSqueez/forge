use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::TransferError;
use crate::layout::database_entry;
use crate::limits::{MAX_PACK_MANIFEST_BYTES, MAX_PUBLIC_MANIFEST_BYTES};
use crate::manifest::{BundleKind, MANIFEST_ENTRY_NAME, PublicManifest, check_head, parse_public};
use crate::outer::{open_outer, read_bounded};
use crate::snapshot::run_blocking;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleSummary {
    Encrypted(Box<PublicManifest>),
    LegacyUnencrypted,
}

impl BundleSummary {
    pub fn kind(&self) -> BundleKind {
        match self {
            Self::Encrypted(manifest) => manifest.kind,
            Self::LegacyUnencrypted => BundleKind::Full,
        }
    }

    pub fn needs_passphrase(&self) -> bool {
        matches!(self, Self::Encrypted(_))
    }
}

pub async fn read_bundle_kind(bundle: PathBuf) -> Result<BundleKind, TransferError> {
    run_blocking(bundle, |bundle| detect_kind(&bundle)).await
}

fn detect_kind(bundle: &Path) -> Result<BundleKind, TransferError> {
    let mut archive = open_outer(bundle)?;
    let mut entries = archive.entries().map_err(|_| TransferError::NotABundle)?;
    let mut first = entries
        .next()
        .ok_or(TransferError::NotABundle)?
        .map_err(|_| TransferError::NotABundle)?;
    let is_file = first.header().entry_type().is_file();
    let path = first
        .path()
        .map_err(|_| TransferError::NotABundle)?
        .into_owned();
    if is_file && path == Path::new(MANIFEST_ENTRY_NAME) {
        let size = first.size();
        let bytes = read_bounded(&mut first, size, MAX_PACK_MANIFEST_BYTES)?;
        return check_head(&bytes);
    }
    if is_file && path == Path::new(&database_entry()) {
        return Ok(BundleKind::Full);
    }
    Err(TransferError::NotABundle)
}

pub async fn read_bundle_summary(bundle: PathBuf) -> Result<BundleSummary, TransferError> {
    run_blocking(bundle, |bundle| summarize(&bundle)).await
}

fn summarize(bundle: &Path) -> Result<BundleSummary, TransferError> {
    let mut archive = open_outer(bundle)?;
    let mut entries = archive.entries().map_err(|_| TransferError::NotABundle)?;
    let mut first = entries
        .next()
        .ok_or(TransferError::NotABundle)?
        .map_err(|_| TransferError::NotABundle)?;
    summarize_first_entry(&mut first)
}

pub(crate) fn summarize_first_entry<R: Read>(
    entry: &mut tar::Entry<'_, R>,
) -> Result<BundleSummary, TransferError> {
    let is_file = entry.header().entry_type().is_file();
    let path = entry
        .path()
        .map_err(|_| TransferError::NotABundle)?
        .into_owned();
    if is_file && path == Path::new(MANIFEST_ENTRY_NAME) {
        let size = entry.size();
        let bytes = read_bounded(entry, size, MAX_PUBLIC_MANIFEST_BYTES)?;
        return parse_public(&bytes).map(|manifest| BundleSummary::Encrypted(Box::new(manifest)));
    }
    if is_file && path == Path::new(&database_entry()) {
        return Ok(BundleSummary::LegacyUnencrypted);
    }
    Err(TransferError::NotABundle)
}
