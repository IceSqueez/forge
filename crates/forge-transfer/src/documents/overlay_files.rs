use std::io;
use std::path::{Path, PathBuf};

use forge_overlay::{CLIP_REFERENCE_PREFIX, IMAGE_REFERENCE_PREFIX};
use forge_registry::{InternalNamespace, ReferenceRemap};
use forge_storage::OverlayId;
use forge_types::Variant;

use crate::bundle::document_layout::safe_segments;
use crate::bundle::naming::{sibling_with_extension, unique_suffix};
use crate::documents::model::{DocumentConfig, OverlayFileDocument};
use crate::documents::undo::{Undo, UndoJournal};
use crate::error::TransferError;

pub(crate) const PENDING_IMPORT_MARKER: &str = ".forge-import-pending";
pub(crate) const SET_ASIDE_EXTENSION: &str = "forge-import-previous";
pub(crate) const UNCOMMITTED_IMPORT_PREFIX: &str = ".forge-import-uncommitted-";
pub(crate) const SET_ASIDE_SESSION_SEPARATOR: char = '-';

pub(crate) async fn place_overlay_files(
    overlay_root: &Path,
    id: &OverlayId,
    files: &[OverlayFileDocument],
    locate: impl Fn(&str) -> Option<PathBuf>,
    undo: &mut UndoJournal,
) -> Result<(), TransferError> {
    if files.is_empty() {
        return Ok(());
    }
    let unsafe_name = |name: &str| TransferError::UnsafeEntry {
        path: name.to_owned(),
        reason: "overlay file name is not a plain relative path",
    };
    if !safe_segments(id.as_str()).is_some_and(|segments| segments.len() == 1) {
        return Err(unsafe_name(id.as_str()));
    }
    let directory = overlay_root.join(id.as_str());
    if !tokio::fs::try_exists(&directory).await? {
        tokio::fs::create_dir_all(&directory).await?;
        undo.push(Undo::OverlayDir(directory.clone()));
        tokio::fs::write(directory.join(PENDING_IMPORT_MARKER), []).await?;
    }
    for file in files {
        let segments = safe_segments(&file.name).ok_or_else(|| unsafe_name(&file.name))?;
        let source = locate(&file.file).ok_or_else(|| TransferError::ContentsMismatch {
            path: file.file.clone(),
        })?;
        let destination = segments
            .iter()
            .fold(directory.clone(), |path, segment| path.join(segment));
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        undo.mark_uncommitted(overlay_root).await?;
        let aside = sibling_with_extension(
            &destination,
            &format!(
                "{SET_ASIDE_EXTENSION}-{}{SET_ASIDE_SESSION_SEPARATOR}{}",
                undo.session(),
                unique_suffix()
            ),
        );
        let set_aside = match tokio::fs::rename(&destination, &aside).await {
            Ok(()) => Some(aside),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        undo.push(Undo::OverlayFile {
            path: destination.clone(),
            set_aside,
        });
        tokio::fs::copy(&source, &destination).await?;
    }
    Ok(())
}

pub(crate) fn rewrite_overlay_media(config: &mut DocumentConfig, remap: &ReferenceRemap) {
    for value in config.values_mut() {
        let Variant::String(stored) = value else {
            continue;
        };
        let rewritten = [
            (CLIP_REFERENCE_PREFIX, InternalNamespace::SoundboardClip),
            (IMAGE_REFERENCE_PREFIX, InternalNamespace::ManagedMedia),
        ]
        .into_iter()
        .find_map(|(prefix, namespace)| {
            let referenced = stored.strip_prefix(prefix)?;
            let to = remap.get(namespace, referenced)?;
            Some(format!("{prefix}{to}"))
        });
        if let Some(rewritten) = rewritten {
            *stored = rewritten;
        }
    }
}
