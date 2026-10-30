use std::collections::BTreeSet;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use forge_registry::ReferenceRemap;
use forge_storage::{
    DataProvider, MediaBlobId, OverlayId, SettingSkip, SettingSpec, SettingsCatalog, StorageError,
    StoreChange,
};
use forge_types::{Queue, QueueId};

use crate::bundle::naming::fresh_pre_setup_stem;
use crate::documents::closure::ClosureRegistries;
use crate::documents::overlay_configs::check_overlay_configs;
use crate::documents::undo::UndoJournal;
use crate::error::TransferError;
use crate::safety_copies::safety_snapshot::write_safety_snapshot;
use crate::setup::inventory::{SetupInventory, load_setup_inventory};
use crate::setup::reader::StagedSetup;

pub struct SetupImportRequest<'a> {
    pub provider: Arc<dyn DataProvider>,
    pub settings_catalog: &'a SettingsCatalog,
    pub hotkey_settings: &'a [SettingSpec],
    pub registries: ClosureRegistries<'a>,
    pub staged: &'a StagedSetup,
    pub data_dir: PathBuf,
    pub overlay_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetupLivePart {
    Queues,
    Scripts,
    Overlays,
    Hotkeys,
    Tts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupQueueChanges {
    pub added: Vec<Queue>,
    pub changed: Vec<Queue>,
    pub removed: Vec<QueueId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedSetting {
    pub key: String,
    pub reason: SettingSkip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupImportOutcome {
    pub safety_snapshot: PathBuf,
    pub refresh: BTreeSet<SetupLivePart>,
    pub queues: SetupQueueChanges,
    pub restart_recommended: bool,
    pub cleanup_failures: Vec<String>,
    pub skipped_settings: Vec<SkippedSetting>,
}

pub async fn apply_setup_import(
    request: SetupImportRequest<'_>,
) -> Result<SetupImportOutcome, TransferError> {
    let SetupImportRequest {
        provider,
        settings_catalog,
        hotkey_settings,
        registries,
        staged,
        data_dir,
        overlay_root,
    } = request;
    check_overlay_configs(
        &staged.documents().configuration.overlays,
        registries.overlays,
    )?;
    let inventory = load_setup_inventory(provider.as_ref()).await?;
    let safety_snapshot =
        write_safety_snapshot(&provider, &data_dir, fresh_pre_setup_stem()).await?;

    let mut import = SetupImport {
        provider: provider.as_ref(),
        settings_catalog,
        hotkey_settings,
        registries,
        staged,
        known_media: inventory.configuration.media.keys().cloned().collect(),
        inventory,
        overlay_root,
        remap: ReferenceRemap::default(),
        journal: UndoJournal::default(),
        changes: Vec::new(),
        minted_overlays: BTreeSet::new(),
        refresh: BTreeSet::new(),
        queues: SetupQueueChanges::default(),
        settings_changed: false,
        skipped_settings: Vec::new(),
        removed_overlays: Vec::new(),
        carried_media: BTreeSet::new(),
    };
    match import.run().await {
        Ok(()) => {
            import.journal.settle_overlay_dirs().await;
            let cleanup_failures = import.clean_up().await;
            Ok(SetupImportOutcome {
                safety_snapshot,
                refresh: import.refresh,
                queues: import.queues,
                restart_recommended: import.settings_changed,
                cleanup_failures,
                skipped_settings: import.skipped_settings,
            })
        }
        Err(cause) => {
            let journal = std::mem::take(&mut import.journal);
            match journal.roll_back(provider.as_ref()).await {
                Ok(()) => Err(cause),
                Err(failure) => Err(TransferError::SetupRollbackIncomplete {
                    cause: cause.to_string(),
                    failure,
                    snapshot: safety_snapshot,
                }),
            }
        }
    }
}

pub(super) struct SetupImport<'p, 'a> {
    pub(super) provider: &'p dyn DataProvider,
    pub(super) settings_catalog: &'a SettingsCatalog,
    pub(super) hotkey_settings: &'a [SettingSpec],
    pub(super) registries: ClosureRegistries<'a>,
    pub(super) staged: &'a StagedSetup,
    pub(super) inventory: SetupInventory,
    pub(super) known_media: BTreeSet<MediaBlobId>,
    pub(super) overlay_root: PathBuf,
    pub(super) remap: ReferenceRemap,
    pub(super) journal: UndoJournal,
    pub(super) changes: Vec<StoreChange>,
    pub(super) minted_overlays: BTreeSet<OverlayId>,
    pub(super) refresh: BTreeSet<SetupLivePart>,
    pub(super) queues: SetupQueueChanges,
    pub(super) settings_changed: bool,
    pub(super) skipped_settings: Vec<SkippedSetting>,
    pub(super) removed_overlays: Vec<OverlayId>,
    pub(super) carried_media: BTreeSet<MediaBlobId>,
}

impl SetupImport<'_, '_> {
    async fn run(&mut self) -> Result<(), TransferError> {
        self.unarchive_local();
        self.unlink_local().await?;
        self.remove_absent_triggers();
        self.remove_absent_scripts();
        self.remove_absent_clips();
        self.remove_absent_globals();
        self.remove_absent_voice_aliases();
        self.write_media().await?;
        self.write_queues();
        self.write_scripts();
        self.write_globals();
        self.write_clips().await?;
        self.write_overlays().await?;
        self.write_triggers();
        self.write_actions()?;
        self.link_triggers();
        self.write_tts();
        self.write_voices()?;
        self.write_settings();
        self.remove_absent_actions();
        self.remove_absent_queues();
        self.remove_absent_overlays();
        self.provider.commit_changes(&self.changes).await?;
        Ok(())
    }

    async fn clean_up(&self) -> Vec<String> {
        let mut failures = Vec::new();
        for id in &self.removed_overlays {
            let directory = self.overlay_root.join(id.as_str());
            match tokio::fs::remove_dir_all(&directory).await {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    failures.push(format!("{}: {e}", directory.display()));
                }
                _ => {}
            }
        }
        let media = self.provider.media_repo();
        for id in self.known_media.difference(&self.carried_media) {
            match media.delete(id).await {
                Ok(_) | Err(StorageError::MediaReferenced { .. }) => {}
                Err(e) => failures.push(format!("{}: {e}", id.as_str())),
            }
        }
        failures
    }
}
