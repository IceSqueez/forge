use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::{DataProvider, MediaBlob};
use time::OffsetDateTime;

use crate::document_bundle::{encode_documents, write_document_bundle};
use crate::error::TransferError;
use crate::full_writer::FORGE_VERSION;
use crate::manifest::{BUNDLE_FORMAT_VERSION, BundleKind};
use crate::naming::partial_path;
use crate::pack::closure::PackSelection;
use crate::pack::documents::PackDocuments;
use crate::pack::inventory::PackInventory;
use crate::pack::writer::{DocumentSource, resolve_media};
use crate::setup::documents::{
    ClipHotkeyDocument, SETUP_DOCUMENTS_VERSION, SetupDocuments, TtsDocument, VoicesDocument,
};
use crate::setup::inventory::{SetupInventory, load_setup_inventory};
use crate::setup::layout::SETUP_LAYOUT;
use crate::setup::manifest::SetupManifest;
use crate::snapshot::run_blocking;

pub struct SetupExportRequest {
    pub provider: Arc<dyn DataProvider>,
    pub overlay_root: PathBuf,
    pub destination: PathBuf,
}

pub async fn write_setup(request: SetupExportRequest) -> Result<PathBuf, TransferError> {
    let SetupExportRequest {
        provider,
        overlay_root,
        destination,
    } = request;
    let inventory = load_setup_inventory(provider.as_ref()).await?;
    let mut selection = everything_in(&inventory.configuration);
    let media = resolve_media(provider.as_ref(), &inventory.configuration, &mut selection).await?;
    let schema_version = provider.schema_version().await?;
    let output = destination.clone();
    let assembly = SetupAssembly {
        inventory,
        selection,
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

fn everything_in(inventory: &PackInventory) -> PackSelection {
    PackSelection {
        actions: inventory.actions.keys().copied().collect(),
        triggers: inventory.triggers.keys().copied().collect(),
        queues: inventory.queues.keys().copied().collect(),
        scripts: inventory.scripts.keys().cloned().collect(),
        globals: inventory
            .globals
            .values()
            .filter(|global| global.persisted)
            .map(|global| global.name.clone())
            .collect(),
        clips: inventory.clips.keys().copied().collect(),
        overlays: inventory.overlays.keys().cloned().collect(),
        media: inventory.media.keys().cloned().collect(),
        ..PackSelection::default()
    }
}

struct SetupAssembly {
    inventory: SetupInventory,
    selection: PackSelection,
    media: Vec<(MediaBlob, PathBuf)>,
    overlay_root: PathBuf,
    schema_version: u32,
}

impl SetupAssembly {
    fn write(self, destination: &Path) -> Result<(), TransferError> {
        let mut files = Vec::new();
        let configuration = DocumentSource {
            inventory: &self.inventory.configuration,
            selection: &self.selection,
            media: &self.media,
            overlay_root: &self.overlay_root,
            layout: SETUP_LAYOUT,
        }
        .collect(&mut files)?;
        let schema_version = self.schema_version;
        let documents = self.documents(configuration);
        let (documents_bytes, documents_entry) = encode_documents(&documents, SETUP_LAYOUT)?;
        let mut contents = vec![documents_entry];
        contents.extend(files.iter().map(|file| file.entry.clone()));
        let manifest = SetupManifest {
            format_version: BUNDLE_FORMAT_VERSION,
            kind: BundleKind::Setup,
            forge_version: FORGE_VERSION.to_owned(),
            schema_version,
            created_at: OffsetDateTime::now_utc(),
            contents,
        };
        write_document_bundle(
            destination,
            &manifest,
            SETUP_LAYOUT,
            &documents_bytes,
            &files,
        )
    }

    fn documents(self, configuration: PackDocuments) -> SetupDocuments {
        let SetupInventory {
            configuration: inventory,
            tts_pipeline,
            filter_rules,
            voice_aliases,
            voice_strategy,
            ignore_profile,
            portable_settings,
        } = self.inventory;
        let global_values = self
            .selection
            .globals
            .iter()
            .filter_map(|name| inventory.globals.get(name))
            .map(|global| (global.name.clone(), global.value.clone()))
            .collect();
        let clip_hotkeys = inventory
            .clips
            .values()
            .filter_map(|clip| {
                clip.hotkey.as_ref().map(|hotkey| ClipHotkeyDocument {
                    clip_id: clip.id,
                    hotkey: hotkey.clone(),
                })
            })
            .collect();
        SetupDocuments {
            version: SETUP_DOCUMENTS_VERSION,
            configuration,
            global_values,
            clip_hotkeys,
            tts: TtsDocument {
                pipeline: tts_pipeline,
                filter_rules,
            },
            voices: VoicesDocument {
                aliases: voice_aliases,
                strategy: voice_strategy,
                ignore_profile,
            },
            settings: portable_settings,
        }
    }
}
