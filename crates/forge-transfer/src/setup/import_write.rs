use std::collections::BTreeMap;

use forge_overlay::image_references;
use forge_registry::InternalNamespace;
use forge_storage::settings::core_settings;
use forge_storage::{
    GlobalsRepo, MediaBlobId, MediaReferrer, MediaReferrerKind, OverlayId, ScriptRecord,
    ScriptRepo, SettingRow, StoredClip, clip_source_referrer, remove_row, restore_row,
};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

use crate::documents::convert::{action_of, queue_of, trigger_of};
use crate::documents::model::ClipSourceDocument;
use crate::documents::overlay_files::{place_overlay_files, rewrite_overlay_media};
use crate::documents::undo::Undo;
use crate::error::TransferError;
use crate::setup::import_apply::{SetupImport, SetupLivePart, SkippedSetting};
use crate::setup::import_undo::SetupUndo;

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
            self.journal
                .push_configuration(Undo::Media(blob.id.clone()));
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

    pub(super) async fn write_queues(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.queue_repo();
        let staged = self.staged;
        for document in &staged.documents().configuration.queues {
            let queue = queue_of(document);
            let previous = self.inventory.configuration.queues.get(&queue.id).cloned();
            repo.save(&queue).await?;
            match &previous {
                None => self.queues.added.push(queue.clone()),
                Some(local) if *local != queue => self.queues.changed.push(queue.clone()),
                Some(_) => {}
            }
            self.journal.push_configuration(Undo::Queue {
                id: queue.id,
                previous,
            });
            self.refresh.insert(SetupLivePart::Queues);
        }
        Ok(())
    }

    pub(super) async fn write_scripts(&mut self) -> Result<(), TransferError> {
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
            ScriptRepo::save(self.provider, record).await?;
            self.journal.push_configuration(Undo::Script {
                id: document.id,
                previous,
                archived,
            });
            if archived {
                self.provider.restore_script(document.id).await?;
            }
            self.refresh.insert(SetupLivePart::Scripts);
        }
        Ok(())
    }

    pub(super) async fn write_globals(&mut self) -> Result<(), TransferError> {
        let staged = self.staged;
        let documents = staged.documents();
        for document in &documents.configuration.globals {
            let value = documents
                .global_values
                .get(&document.name)
                .cloned()
                .unwrap_or_else(|| document.default_value.clone());
            let previous = self.inventory.configuration.stored_global(&document.name);
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            GlobalsRepo::set(self.provider, &document.name, value, document.persisted).await?;
            self.journal.push_configuration(Undo::Global {
                name: document.name.clone(),
                previous: previous.map(|stored| stored.row),
                archived,
            });
            if archived {
                GlobalsRepo::restore(self.provider, &document.name).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn write_clips(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.soundboard_clips_repo();
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
            repo.save(&clip).await?;
            self.journal.push_configuration(Undo::Clip {
                id: document.id,
                previous,
            });
            if let Some(blob) = blob {
                self.retain(clip_source_referrer(document.id), &blob)
                    .await?;
            }
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        referrer: MediaReferrer,
        blob: &MediaBlobId,
    ) -> Result<(), TransferError> {
        let repo = self.provider.media_repo();
        let previous = repo.blob_of(&referrer).await?;
        repo.retain(&referrer, blob).await?;
        self.journal
            .push_configuration(Undo::Referrer { referrer, previous });
        Ok(())
    }

    pub(super) async fn write_overlays(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.overlay_repo();
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
            let mut definition = match &local {
                Some(local) => local.clone(),
                None => {
                    let created = repo
                        .create(
                            &document.display_name,
                            &document.kind_id,
                            document.config_schema_version,
                        )
                        .await?;
                    self.journal.push_configuration(Undo::Overlay {
                        id: created.id.clone(),
                        previous: None,
                    });
                    created
                }
            };
            definition.display_name = document.display_name.clone();
            definition.kind_id = document.kind_id.clone();
            definition.enabled = document.enabled;
            definition.position = document.position;
            definition.config = config;
            definition.config_schema_version = document.config_schema_version;
            definition.source_overrides = document.source_overrides.clone();
            repo.save(&definition).await?;
            if local.is_some() {
                self.journal.push_configuration(Undo::Overlay {
                    id: definition.id.clone(),
                    previous: local,
                });
            }
            if definition.id.as_str() != document.id {
                self.remap.insert(
                    InternalNamespace::Overlay,
                    document.id.clone(),
                    definition.id.as_str(),
                );
            }
            let journal = &mut self.journal;
            place_overlay_files(
                &self.overlay_root,
                &definition.id,
                &document.files,
                |file| staged.file(file),
                |undo| journal.push_configuration(undo),
            )
            .await?;
            for reference in image_references(&definition.config) {
                let Ok(blob) = MediaBlobId::from_stored(reference.image) else {
                    continue;
                };
                if self.known_media.contains(&blob) {
                    let referrer = MediaReferrer::new(
                        MediaReferrerKind::Overlay,
                        definition.id.as_str(),
                        reference.key,
                    );
                    self.retain(referrer, &blob).await?;
                }
            }
            self.refresh.insert(SetupLivePart::Overlays);
        }
        Ok(())
    }

    pub(super) async fn write_triggers(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.trigger_instance_repo();
        let staged = self.staged;
        for document in &staged.documents().configuration.triggers {
            let mut trigger = trigger_of(document);
            self.registries
                .catalog()
                .rewrite_trigger_instance(&mut trigger, &self.remap);
            let previous = self
                .inventory
                .configuration
                .stored_trigger(&trigger.id)
                .map(|stored| stored.row);
            repo.save(&trigger).await?;
            self.journal.push_configuration(Undo::Trigger {
                id: trigger.id,
                previous,
                archived: false,
            });
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    pub(super) async fn write_actions(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.action_repo();
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
            let previous = self
                .inventory
                .configuration
                .stored_action(&action.id)
                .map(|stored| stored.row);
            let created = previous.is_none();
            repo.save(&action).await?;
            self.journal.push_configuration(Undo::Action {
                id: action.id,
                previous,
                archived: false,
            });
            if created {
                repo.restore(action.id).await?;
            }
        }
        Ok(())
    }

    pub(super) async fn link_triggers(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.trigger_instance_repo();
        let carried = self.setup_trigger_ids();
        let staged = self.staged;
        for document in &staged.documents().configuration.actions {
            let linked = document
                .trigger_ids
                .iter()
                .filter(|trigger| carried.contains(*trigger));
            for trigger in linked {
                repo.link_action(document.id, *trigger).await?;
                self.journal.push_configuration(Undo::Link {
                    action: document.id,
                    trigger: *trigger,
                });
                self.refresh.insert(SetupLivePart::Hotkeys);
            }
        }
        Ok(())
    }

    pub(super) async fn write_tts(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.tts_filters_repo();
        let tts = &self.staged.documents().tts;
        repo.set_pipeline_settings(&tts.pipeline).await?;
        self.journal
            .push(SetupUndo::TtsPipeline(self.inventory.tts_pipeline.clone()));
        repo.replace_rules(&tts.filter_rules).await?;
        self.journal
            .push(SetupUndo::FilterRules(self.inventory.filter_rules.clone()));
        self.refresh.insert(SetupLivePart::Tts);
        Ok(())
    }

    pub(super) async fn write_voices(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.voice_alias_repo();
        let voices = &self.staged.documents().voices;
        for alias in &voices.aliases {
            let previous = self
                .inventory
                .voice_aliases
                .iter()
                .find(|local| local.id == alias.id)
                .cloned();
            let stored = repo.upsert(alias).await?;
            self.journal.push(SetupUndo::VoiceAlias {
                id: stored.id,
                previous,
            });
        }
        repo.set_strategy(&voices.strategy).await?;
        self.journal.push(SetupUndo::VoiceStrategy(
            self.inventory.voice_strategy.clone(),
        ));
        repo.set_ignore_profile(&voices.ignore_profile).await?;
        self.journal.push(SetupUndo::IgnoreProfile(
            self.inventory.ignore_profile.clone(),
        ));
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

    pub(super) async fn write_settings(&mut self) -> Result<(), TransferError> {
        let (incoming, skipped) = self.admit_incoming_settings();
        self.skipped_settings = skipped;
        let local = self.inventory.portable_settings.clone();
        for (key, row) in &incoming {
            let previous = local.get(key).cloned();
            if previous
                .as_ref()
                .is_some_and(|local_row| local_row.value() == row.value())
            {
                continue;
            }
            restore_row(self.provider, row).await?;
            self.journal.push(match previous {
                Some(previous) => SetupUndo::SettingRestore(previous),
                None => SetupUndo::SettingRemove(row.clone()),
            });
            self.note_setting_change(key);
        }
        let carried = &self.staged.documents().settings;
        for (key, row) in local {
            if carried.contains_key(&key) {
                continue;
            }
            remove_row(self.provider, &row).await?;
            self.note_setting_change(&key);
            self.journal.push(SetupUndo::SettingRestore(row));
        }
        Ok(())
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
