use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::{DataProvider, MediaBlob};
use time::OffsetDateTime;

use crate::bundle::blocking::run_blocking;
use crate::bundle::document_layout::PACK_LAYOUT;
use crate::bundle::manifest::{BUNDLE_FORMAT_VERSION, BundleKind, ContentEntry, FORGE_VERSION};
use crate::bundle::naming::partial_path;
use crate::documents::archive::{encode_documents, write_document_bundle};
use crate::documents::closure::PackSelection;
use crate::documents::inventory::PackInventory;
use crate::documents::source::{DocumentSource, resolve_media};
use crate::error::TransferError;
use crate::pack::manifest::PackManifest;

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
    if written.is_err()
        && let Err(error) = tokio::fs::remove_file(partial_path(&destination)).await
    {
        tracing::debug!(%error, "partial pack could not be removed");
    }
    written.map(|()| destination)
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
