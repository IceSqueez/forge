use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};

use crate::bundle::digest::digest_reader;
use crate::bundle::document_layout::DocumentLayout;
use crate::bundle::extract::Extractor;
use crate::bundle::layout::{ARCHIVE_SEPARATOR, ArchiveScope};
use crate::bundle::limits::{MAX_PACK_DOCUMENTS_BYTES, MAX_PACK_MANIFEST_BYTES};
use crate::bundle::manifest::{ContentEntry, MANIFEST_ENTRY_NAME};
use crate::bundle::naming::partial_path;
use crate::bundle::outer::{open_outer, read_bounded};
use crate::bundle::payload::{PayloadFile, append_bytes, append_verified};
use crate::documents::model::{ClipSourceDocument, PackDocuments};
use crate::error::TransferError;
use forge_durable::create_private;

pub(crate) trait DocumentManifest: Sized {
    fn contents(&self) -> &[ContentEntry];
}

pub(crate) struct StagedDocuments<M> {
    pub(crate) manifest: M,
    pub(crate) documents: Vec<u8>,
}

pub(crate) fn encode_documents(
    documents: &impl Serialize,
    layout: DocumentLayout,
) -> Result<(Vec<u8>, ContentEntry), TransferError> {
    let bytes = serde_json::to_vec(documents).map_err(std::io::Error::from)?;
    let (size, sha256_hex) = digest_reader(bytes.as_slice())?;
    let entry = ContentEntry {
        path: layout.documents_entry(),
        size,
        sha256_hex,
    };
    Ok((bytes, entry))
}

pub(crate) fn write_document_bundle(
    destination: &Path,
    manifest: &impl Serialize,
    layout: DocumentLayout,
    documents: &[u8],
    files: &[PayloadFile],
) -> Result<(), TransferError> {
    let manifest_bytes = serde_json::to_vec(manifest).map_err(std::io::Error::from)?;
    let partial = partial_path(destination);
    let sink = BufWriter::new(create_private(&partial)?);
    let mut builder = tar::Builder::new(GzEncoder::new(sink, Compression::default()));
    append_bytes(&mut builder, MANIFEST_ENTRY_NAME, &manifest_bytes)?;
    append_bytes(&mut builder, &layout.documents_entry(), documents)?;
    for file in files {
        append_verified(&mut builder, file)?;
    }
    let mut sink = builder.into_inner()?.finish()?;
    sink.flush()?;
    sink.get_ref().sync_all()?;
    drop(sink);
    std::fs::rename(&partial, destination)?;
    Ok(())
}

pub(crate) fn stage_document_bundle<M: DocumentManifest>(
    bundle: &Path,
    staging_dir: &Path,
    layout: DocumentLayout,
    scope: ArchiveScope,
    parse_manifest: impl FnOnce(&[u8]) -> Result<M, TransferError>,
) -> Result<StagedDocuments<M>, TransferError> {
    std::fs::create_dir_all(staging_dir)?;
    let mut archive = open_outer(bundle)?;
    let mut entries = archive.entries().map_err(|_| TransferError::NotABundle)?;
    let mut first = entries
        .next()
        .ok_or(TransferError::NotABundle)?
        .map_err(|_| TransferError::NotABundle)?;
    let is_manifest = first.header().entry_type().is_file()
        && first
            .path()
            .map_err(|_| TransferError::NotABundle)?
            .as_ref()
            == Path::new(MANIFEST_ENTRY_NAME);
    if !is_manifest {
        return Err(TransferError::NotABundle);
    }
    let size = first.size();
    let manifest = parse_manifest(&read_bounded(&mut first, size, MAX_PACK_MANIFEST_BYTES)?)?;
    drop(first);

    let documents_path = layout.documents_entry();
    let documents_size = manifest
        .contents()
        .iter()
        .find(|entry| entry.path == documents_path)
        .map(|entry| entry.size)
        .ok_or_else(|| TransferError::ContentsMismatch {
            path: documents_path.clone(),
        })?;
    if documents_size > MAX_PACK_DOCUMENTS_BYTES {
        return Err(TransferError::EntryTooLarge {
            path: documents_path,
            size: documents_size,
            limit: MAX_PACK_DOCUMENTS_BYTES,
        });
    }

    let mut extractor = Extractor::new(
        staging_dir.to_path_buf(),
        scope,
        Some(manifest.contents().to_vec()),
    );
    for entry in entries {
        extractor.extract(&mut entry.map_err(TransferError::damaged)?)?;
    }
    extractor.finish()?;

    let documents = std::fs::read(staged_path(staging_dir, &documents_path))?;
    Ok(StagedDocuments {
        manifest,
        documents,
    })
}

#[derive(Deserialize)]
struct DocumentsHead {
    version: u32,
}

pub(crate) fn documents_version(bytes: &[u8]) -> Result<u32, TransferError> {
    let head: DocumentsHead = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    Ok(head.version)
}

pub(crate) fn check_bundled_files(
    contents: &[ContentEntry],
    layout: DocumentLayout,
    documents: &PackDocuments,
) -> Result<(), TransferError> {
    let documents_path = layout.documents_entry();
    let carried =
        |path: &str| path != documents_path && contents.iter().any(|entry| entry.path == path);
    if let Some(missing) = documents.bundled_files().find(|path| !carried(path)) {
        return Err(TransferError::ContentsMismatch {
            path: missing.to_owned(),
        });
    }
    let dangling_clip = documents.clips.iter().any(|clip| match &clip.source {
        ClipSourceDocument::Media { media_id } => {
            !documents.media.iter().any(|media| &media.id == media_id)
        }
        ClipSourceDocument::File { .. } | ClipSourceDocument::Missing => false,
    });
    if dangling_clip {
        return Err(TransferError::damaged(
            "a sound clip refers to media the file does not carry",
        ));
    }
    Ok(())
}

pub(crate) fn carried_file(
    staging_dir: &Path,
    contents: &[ContentEntry],
    layout: DocumentLayout,
    bundle_path: &str,
) -> Option<PathBuf> {
    let carried = bundle_path != layout.documents_entry()
        && contents.iter().any(|entry| entry.path == bundle_path);
    carried.then(|| staged_path(staging_dir, bundle_path))
}

fn staged_path(staging_dir: &Path, bundle_path: &str) -> PathBuf {
    bundle_path
        .split(ARCHIVE_SEPARATOR)
        .fold(staging_dir.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}
