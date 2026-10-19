use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_registry::InternalNamespace;
use forge_storage::{DataProvider, MediaBlob, MediaBlobId, StorageError};
use time::OffsetDateTime;

use crate::document_bundle::{encode_documents, write_document_bundle};
use crate::error::TransferError;
use crate::full_writer::FORGE_VERSION;
use crate::manifest::{BUNDLE_FORMAT_VERSION, BundleKind, ContentEntry};
use crate::naming::partial_path;
use crate::pack::closure::PackSelection;
use crate::pack::documents::{
    ActionDocument, ClipDocument, ClipSourceDocument, GlobalDocument, MediaDocument,
    OverlayDocument, OverlayFileDocument, PACK_DOCUMENTS_VERSION, PackDocuments, QueueDocument,
    ScriptDocument, TriggerDocument,
};
use crate::pack::inventory::PackInventory;
use crate::pack::layout::{DocumentLayout, PACK_LAYOUT, safe_segments};
use crate::pack::manifest::{PackManifest, PackReference};
use crate::payload::{PayloadFile, digested};
use crate::snapshot::run_blocking;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMetadata {
    pub name: String,
    pub author: String,
    pub description: String,
}

pub struct PackExportRequest {
    pub provider: Arc<dyn DataProvider>,
    pub inventory: PackInventory,
    pub selection: PackSelection,
    pub metadata: PackMetadata,
    pub overlay_root: PathBuf,
    pub destination: PathBuf,
}

pub async fn write_pack(request: PackExportRequest) -> Result<PathBuf, TransferError> {
    let PackExportRequest {
        provider,
        inventory,
        mut selection,
        metadata,
        overlay_root,
        destination,
    } = request;
    if metadata.name.trim().is_empty() {
        return Err(TransferError::PackNameRequired);
    }
    if selection.actions.is_empty() {
        return Err(TransferError::EmptyPack);
    }
    let media = resolve_media(provider.as_ref(), &inventory, &mut selection).await?;
    let schema_version = provider.schema_version().await?;
    let output = destination.clone();
    let assembly = PackAssembly {
        inventory,
        selection,
        metadata,
        media,
        overlay_root,
        schema_version,
    };
    let written = run_blocking(assembly, move |assembly| assembly.write(&output)).await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(partial_path(&destination)).await;
    }
    written.map(|()| destination)
}

pub(crate) async fn resolve_media(
    provider: &dyn DataProvider,
    inventory: &PackInventory,
    selection: &mut PackSelection,
) -> Result<Vec<(MediaBlob, PathBuf)>, TransferError> {
    let repo = provider.media_repo();
    let mut resolved = Vec::new();
    let mut unreadable = Vec::new();
    for id in &selection.media {
        let Some(blob) = inventory.media.get(id) else {
            continue;
        };
        match repo.resolve(id).await {
            Ok(path) => resolved.push((blob.clone(), path)),
            Err(StorageError::NotFound { .. }) => unreadable.push(id.clone()),
            Err(e) => return Err(e.into()),
        }
    }
    for id in unreadable {
        selection.media.remove(&id);
        selection.unresolved_references.insert(media_reference(&id));
    }
    Ok(resolved)
}

fn media_reference(id: &MediaBlobId) -> PackReference {
    PackReference {
        namespace: InternalNamespace::ManagedMedia.as_str().to_owned(),
        value: id.as_str().to_owned(),
    }
}

struct PackAssembly {
    inventory: PackInventory,
    selection: PackSelection,
    metadata: PackMetadata,
    media: Vec<(MediaBlob, PathBuf)>,
    overlay_root: PathBuf,
    schema_version: u32,
}

impl PackAssembly {
    fn write(self, destination: &Path) -> Result<(), TransferError> {
        let mut files = Vec::new();
        let documents = DocumentSource {
            inventory: &self.inventory,
            selection: &self.selection,
            media: &self.media,
            overlay_root: &self.overlay_root,
            layout: PACK_LAYOUT,
        }
        .collect(&mut files)?;
        let (documents_bytes, documents_entry) = encode_documents(&documents, PACK_LAYOUT)?;
        let mut contents = vec![documents_entry];
        contents.extend(files.iter().map(|file| file.entry.clone()));
        let manifest = self.manifest(contents);
        write_document_bundle(
            destination,
            &manifest,
            PACK_LAYOUT,
            &documents_bytes,
            &files,
        )
    }

    fn manifest(&self, contents: Vec<ContentEntry>) -> PackManifest {
        PackManifest {
            format_version: BUNDLE_FORMAT_VERSION,
            kind: BundleKind::Pack,
            forge_version: FORGE_VERSION.to_owned(),
            schema_version: self.schema_version,
            created_at: OffsetDateTime::now_utc(),
            name: self.metadata.name.trim().to_owned(),
            author: self.metadata.author.trim().to_owned(),
            description: self.metadata.description.trim().to_owned(),
            required_integrations: self
                .selection
                .required_integrations
                .iter()
                .cloned()
                .collect(),
            external_references: self.selection.external_references.iter().cloned().collect(),
            unresolved_references: self
                .selection
                .unresolved_references
                .iter()
                .cloned()
                .collect(),
            unknown_kinds: self.selection.unknown_kinds.iter().cloned().collect(),
            contents,
        }
    }
}

pub(crate) struct DocumentSource<'a> {
    pub(crate) inventory: &'a PackInventory,
    pub(crate) selection: &'a PackSelection,
    pub(crate) media: &'a [(MediaBlob, PathBuf)],
    pub(crate) overlay_root: &'a Path,
    pub(crate) layout: DocumentLayout,
}

impl DocumentSource<'_> {
    pub(crate) fn collect(
        &self,
        files: &mut Vec<PayloadFile>,
    ) -> Result<PackDocuments, TransferError> {
        let inventory = self.inventory;
        let selection = self.selection;
        let layout = self.layout;
        let mut media = Vec::new();
        let mut carried_media = BTreeSet::new();
        for (index, (blob, source)) in self.media.iter().enumerate() {
            let file = layout.media_entry(index);
            files.push(digested(source, file.clone())?);
            carried_media.insert(blob.id.clone());
            media.push(MediaDocument::of(blob, file));
        }
        let mut clips = Vec::new();
        for id in &selection.clips {
            let Some(clip) = inventory.clips.get(id) else {
                continue;
            };
            let managed = inventory
                .clip_media
                .get(id)
                .filter(|blob| carried_media.contains(*blob));
            let source = match managed {
                Some(blob) => ClipSourceDocument::Media {
                    media_id: blob.as_str().to_owned(),
                },
                None if clip.file_path.is_file() => {
                    let file = layout.clip_entry(clips.len());
                    files.push(digested(&clip.file_path, file.clone())?);
                    ClipSourceDocument::File {
                        file,
                        label: clip
                            .file_path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    }
                }
                None => ClipSourceDocument::Missing,
            };
            clips.push(ClipDocument::of(clip, source));
        }
        let mut overlays = Vec::new();
        for id in &selection.overlays {
            let Some(overlay) = inventory.overlays.get(id) else {
                continue;
            };
            let index = overlays.len();
            let mut carried = Vec::new();
            if safe_segments(id.as_str()).is_some_and(|segments| segments.len() == 1) {
                let directory = self.overlay_root.join(id.as_str());
                for name in &overlay.source_overrides {
                    let Some(segments) = safe_segments(name) else {
                        continue;
                    };
                    let source = segments
                        .iter()
                        .fold(directory.clone(), |path, segment| path.join(segment));
                    if !is_regular_file(&source) {
                        continue;
                    }
                    let file = layout.overlay_entry(index, &segments);
                    files.push(digested(&source, file.clone())?);
                    carried.push(OverlayFileDocument {
                        name: name.clone(),
                        file,
                    });
                }
            }
            overlays.push(OverlayDocument::of(overlay, carried));
        }
        Ok(PackDocuments {
            version: PACK_DOCUMENTS_VERSION,
            actions: selection
                .actions
                .iter()
                .filter_map(|id| inventory.actions.get(id))
                .map(|action| {
                    let linked = inventory
                        .action_triggers
                        .get(&action.id)
                        .into_iter()
                        .flatten();
                    let carried = linked
                        .filter(|trigger| selection.triggers.contains(*trigger))
                        .copied()
                        .collect();
                    ActionDocument::of(action, carried)
                })
                .collect(),
            triggers: selection
                .triggers
                .iter()
                .filter_map(|id| inventory.triggers.get(id))
                .map(TriggerDocument::of)
                .collect(),
            queues: selection
                .queues
                .iter()
                .filter_map(|id| inventory.queues.get(id))
                .map(QueueDocument::of)
                .collect(),
            scripts: selection
                .scripts
                .iter()
                .filter_map(|name| inventory.scripts.get(name))
                .map(ScriptDocument::of)
                .collect(),
            globals: selection
                .globals
                .iter()
                .filter_map(|name| inventory.globals.get(name))
                .map(GlobalDocument::of)
                .collect(),
            clips,
            overlays,
            media,
        })
    }
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}
