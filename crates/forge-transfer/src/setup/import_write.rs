use std::collections::BTreeMap;

use forge_overlay::image_references;
use forge_registry::InternalNamespace;
use forge_storage::settings::core_settings;
use forge_storage::{
    MediaBlobId, MediaReferrer, MediaReferrerKind, NewOverlay, OverlayConfig, OverlayDefinition,
    OverlayId, ScriptRecord, SettingRow, StorageError, StoreChange, StoredClip,
    clip_source_referrer, setting_row, unique_overlay_id,
};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

use crate::documents::convert::{action_of, queue_of, trigger_of};
use crate::documents::model::{ClipSourceDocument, OverlayDocument};
use crate::documents::overlay_files::{place_overlay_files, rewrite_overlay_media};
use crate::documents::undo::Undo;
use crate::error::TransferError;
use crate::setup::import_apply::{SetupImport, SetupLivePart, SkippedSetting};

const TTS_SETTINGS: [&str; 3] = [
    core_settings::TTS_DISABLED_ENGINES.name(),
    core_settings::TTS_SYNTHESIS_DEFAULTS.name(),
    core_settings::TTS_MASTER_VOLUME.name(),
];

impl SetupImport<'_, '_> {
    async fn store_staged(
        &mut self,
        setup_path: &str,
        label: &str,
    ) -> Result<MediaBlobId, TransferError> {
        let source =
            self.staged
                .file(setup_path)
                .ok_or_else(|| TransferError::ContentsMismatch {
                    path: setup_path.to_owned(),
                })?;
        let bytes = tokio::fs::read(&source).await?;
        let blob = self.provider.media_repo().store(label, bytes).await?;
        if self.known_media.insert(blob.id.clone()) {
            self.journal.push(Undo::Media(blob.id.clone()));
        }
        self.carried_media.insert(blob.id.clone());
        Ok(blob.id)
    }

    pub(super) async fn write_media(&mut self) -> Result<(), TransferError> {
        let staged = self.staged;
        for media in &staged.documents().configuration.media {
            let blob = self.store_staged(&media.file, &media.label).await?;
            if blob.as_str() != media.id {
                self.remap.insert(
                    InternalNamespace::ManagedMedia,
                    media.id.clone(),
                    blob.as_str(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn write_queues(&mut self) {
        let staged = self.staged;
        for document in &staged.documents().configuration.queues {
            let queue = queue_of(document);
            match self.inventory.configuration.queues.get(&queue.id) {
                None => self.queues.added.push(queue.clone()),
                Some(local) if *local != queue => self.queues.changed.push(queue.clone()),
                Some(_) => {}
            }
            self.changes.push(StoreChange::SaveQueue(queue));
            self.refresh.insert(SetupLivePart::Queues);
        }
    }

    pub(super) fn write_scripts(&mut self) {
        let staged = self.staged;
        for document in &staged.documents().configuration.scripts {
            let previous = self.inventory.configuration.stored_script(document.id);
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            let previous = previous.map(|stored| stored.row);
            let now = OffsetDateTime::now_utc();
            let record = ScriptRecord {
                id: document.id,
                name: document.name.clone(),
                body: document.body.clone(),
                contract: document.contract.clone(),
                enabled: document.enabled,
                created_at: previous.as_ref().map_or(now, |script| script.created_at),
                last_modified: now,
            };
            self.changes.push(StoreChange::SaveScript(Box::new(record)));
            if archived {
                self.changes.push(StoreChange::RestoreScript(document.id));
            }
            self.refresh.insert(SetupLivePart::Scripts);
        }
    }

    pub(super) fn write_globals(&mut self) {
        let staged = self.staged;
        let documents = staged.documents();
        for document in &documents.configuration.globals {
            let value = documents
                .global_values
                .get(&document.name)
                .cloned()
                .unwrap_or_else(|| document.default_value.clone());
            self.changes.push(StoreChange::SetGlobal {
                name: document.name.clone(),
                value,
                persisted: document.persisted,
            });
        }
    }

    pub(super) async fn write_clips(&mut self) -> Result<(), TransferError> {
        let media = self.provider.media_repo();
        let staged = self.staged;
        let documents = staged.documents();
        let hotkeys: BTreeMap<ClipId, &str> = documents
            .clip_hotkeys
            .iter()
            .map(|binding| (binding.clip_id, binding.hotkey.as_str()))
            .collect();
        for document in &documents.configuration.clips {
            let blob = match &document.source {
                ClipSourceDocument::Media { media_id } => {
                    let stored = self
                        .remap
                        .get(InternalNamespace::ManagedMedia, media_id)
                        .unwrap_or(media_id);
                    Some(MediaBlobId::from_stored(stored)?)
                }
                ClipSourceDocument::File { file, label } => {
                    Some(self.store_staged(file, label).await?)
                }
                ClipSourceDocument::Missing => None,
            };
            let previous = self
                .inventory
                .configuration
                .clips
                .get(&document.id)
                .cloned();
            let file_path = match &blob {
                Some(blob) => media.resolve(blob).await?,
                None => previous
                    .as_ref()
                    .map(|clip| clip.file_path.clone())
                    .unwrap_or_default(),
            };
            let clip = StoredClip {
                id: document.id,
                name: document.name.clone(),
                file_path,
                volume: document.volume,
                output_device: previous
                    .as_ref()
                    .map_or(OutputDevice::Default, |clip| clip.output_device.clone()),
                hotkey: hotkeys.get(&document.id).map(|hotkey| (*hotkey).to_owned()),
                created_at: previous
                    .as_ref()
                    .map_or_else(OffsetDateTime::now_utc, |clip| clip.created_at),
                category: document.category.clone(),
                loop_playback: document.loop_playback,
                duration_secs: document.duration_secs,
                builtin_id: document.builtin_id.clone(),
            };
            self.changes.push(StoreChange::SaveClip(Box::new(clip)));
            if let Some(blob) = blob {
                self.changes.push(StoreChange::RetainMedia {
                    referrer: clip_source_referrer(document.id),
                    blob,
                });
            }
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    pub(super) async fn write_overlays(&mut self) -> Result<(), TransferError> {
        let staged = self.staged;
        for document in &staged.documents().configuration.overlays {
            let mut config = document.config.clone();
            rewrite_overlay_media(&mut config, &self.remap);
            let local = self
                .inventory
                .configuration
                .overlays
                .get(&OverlayId::new(document.id.as_str()))
                .cloned();
            let id = match local {
                Some(local) => {
                    let id = local.id.clone();
                    self.changes
                        .push(StoreChange::SaveOverlay(Box::new(replaced_overlay(
                            local, document, &config,
                        ))));
                    id
                }
                None => {
                    let id = self.mint_overlay_id(&document.display_name);
                    self.changes
                        .push(StoreChange::CreateOverlay(Box::new(NewOverlay {
                            id: id.clone(),
                            display_name: document.display_name.clone(),
                            kind_id: document.kind_id.clone(),
                            enabled: document.enabled,
                            position: Some(document.position),
                            config: config.clone(),
                            config_schema_version: document.config_schema_version,
                            source_overrides: document.source_overrides.clone(),
                        })));
                    id
                }
            };
            if id.as_str() != document.id {
                self.remap
                    .insert(InternalNamespace::Overlay, document.id.clone(), id.as_str());
            }
            let journal = &mut self.journal;
            place_overlay_files(
                &self.overlay_root,
                &id,
                &document.files,
                |file| staged.file(file),
                |undo| journal.push(undo),
            )
            .await?;
            for reference in image_references(&config) {
                let Ok(blob) = MediaBlobId::from_stored(reference.image) else {
                    continue;
                };
                if self.known_media.contains(&blob) {
                    let referrer =
                        MediaReferrer::new(MediaReferrerKind::Overlay, id.as_str(), reference.key);
                    self.changes
                        .push(StoreChange::RetainMedia { referrer, blob });
                }
            }
            self.refresh.insert(SetupLivePart::Overlays);
        }
        Ok(())
    }

    fn mint_overlay_id(&mut self, display_name: &str) -> OverlayId {
        let id = unique_overlay_id(display_name, |candidate| {
            self.inventory
                .configuration
                .overlays
                .contains_key(candidate)
                || self.minted_overlays.contains(candidate)
        });
        self.minted_overlays.insert(id.clone());
        id
    }

    pub(super) fn write_triggers(&mut self) {
        let staged = self.staged;
        for document in &staged.documents().configuration.triggers {
            let mut trigger = trigger_of(document);
            self.registries
                .catalog()
                .rewrite_trigger_instance(&mut trigger, &self.remap);
            self.changes
                .push(StoreChange::SaveTrigger(Box::new(trigger)));
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
    }

    pub(super) fn write_actions(&mut self) -> Result<(), TransferError> {
        let queues = self.setup_queue_ids();
        let staged = self.staged;
        for document in &staged.documents().configuration.actions {
            let mut action = action_of(document);
            self.registries
                .catalog()
                .rewrite_action(&mut action, &self.remap)
                .map_err(TransferError::damaged)?;
            if !queues.contains(&action.queue_id) {
                return Err(TransferError::damaged(format!(
                    "action '{}' refers to a queue the setup does not carry",
                    action.name
                )));
            }
            let created = self
                .inventory
                .configuration
                .stored_action(&action.id)
                .is_none();
            let id = action.id;
            self.changes.push(StoreChange::SaveAction(Box::new(action)));
            if created {
                self.changes.push(StoreChange::RestoreAction(id));
            }
        }
        Ok(())
    }

    pub(super) fn link_triggers(&mut self) {
        let carried = self.setup_trigger_ids();
        let staged = self.staged;
        for document in &staged.documents().configuration.actions {
            let linked = document
                .trigger_ids
                .iter()
                .filter(|trigger| carried.contains(*trigger));
            for trigger in linked {
                self.changes.push(StoreChange::LinkTrigger {
                    action: document.id,
                    trigger: *trigger,
                });
                self.refresh.insert(SetupLivePart::Hotkeys);
            }
        }
    }

    pub(super) fn write_tts(&mut self) {
        let tts = &self.staged.documents().tts;
        self.changes.push(StoreChange::SetPipelineSettings(Box::new(
            tts.pipeline.clone(),
        )));
        self.changes
            .push(StoreChange::ReplaceFilterRules(tts.filter_rules.clone()));
        self.refresh.insert(SetupLivePart::Tts);
    }

    pub(super) fn write_voices(&mut self) -> Result<(), StorageError> {
        let voices = &self.staged.documents().voices;
        for alias in &voices.aliases {
            self.changes
                .push(StoreChange::UpsertVoiceAlias(Box::new(alias.clone())));
        }
        self.changes.push(StoreChange::PutSetting(setting_row(
            &core_settings::VOICE_ASSIGNMENT_STRATEGY,
            &voices.strategy,
        )?));
        self.changes.push(StoreChange::SetIgnoreProfile(Box::new(
            voices.ignore_profile.clone(),
        )));
        self.refresh.insert(SetupLivePart::Tts);
        Ok(())
    }

    fn admit_incoming_settings(&self) -> (BTreeMap<String, SettingRow>, Vec<SkippedSetting>) {
        let mut admitted = BTreeMap::new();
        let mut skipped = Vec::new();
        for (key, value) in &self.staged.documents().settings {
            let value = if key == core_settings::AUDIO_OVERLAY_ID.name() {
                self.remap
                    .get(InternalNamespace::Overlay, value)
                    .unwrap_or(value)
            } else {
                value.as_str()
            };
            match self.settings_catalog.admit(key, value) {
                Ok(row) => {
                    admitted.insert(key.clone(), row);
                }
                Err(reason) => skipped.push(SkippedSetting {
                    key: key.clone(),
                    reason,
                }),
            }
        }
        (admitted, skipped)
    }

    pub(super) fn write_settings(&mut self) {
        let (incoming, skipped) = self.admit_incoming_settings();
        self.skipped_settings = skipped;
        let local = self.inventory.portable_settings.clone();
        for (key, row) in &incoming {
            if local
                .get(key)
                .is_some_and(|local_row| local_row.value() == row.value())
            {
                continue;
            }
            self.changes.push(StoreChange::PutSetting(row.clone()));
            self.note_setting_change(key);
        }
        let carried = &self.staged.documents().settings;
        for (key, row) in local {
            if carried.contains_key(&key) {
                continue;
            }
            self.changes.push(StoreChange::DeleteSetting(row));
            self.note_setting_change(&key);
        }
    }

    fn note_setting_change(&mut self, key: &str) {
        self.settings_changed = true;
        if self
            .hotkey_settings
            .iter()
            .any(|spec| spec.key.name() == key)
        {
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        if TTS_SETTINGS.contains(&key) || key.starts_with(core_settings::TTS_ENGINE_PARAMS.prefix())
        {
            self.refresh.insert(SetupLivePart::Tts);
        }
    }
}

fn replaced_overlay(
    mut definition: OverlayDefinition,
    document: &OverlayDocument,
    config: &OverlayConfig,
) -> OverlayDefinition {
    definition.display_name = document.display_name.clone();
    definition.kind_id = document.kind_id.clone();
    definition.enabled = document.enabled;
    definition.position = document.position;
    definition.config = config.clone();
    definition.config_schema_version = document.config_schema_version;
    definition.source_overrides = document.source_overrides.clone();
    definition
}
