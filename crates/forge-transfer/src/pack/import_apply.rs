use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use forge_overlay::image_references;
use forge_registry::{InternalNamespace, ReferenceRemap};
use forge_storage::{
    DataProvider, GlobalsRepo, MediaBlobId, MediaReferrer, MediaReferrerKind, OverlayId,
    ScriptRecord, ScriptRepo, StoredClip, clip_source_referrer,
};
use forge_types::{ActionId, ClipId, OutputDevice, QueueId, ScriptId, TriggerInstanceId};
use time::OffsetDateTime;
use ulid::Ulid;

use crate::bundle::naming::fresh_pre_pack_stem;
use crate::documents::closure::ClosureRegistries;
use crate::documents::convert::{action_of, queue_of, trigger_of};
use crate::documents::inventory::{PackInventory, load_pack_inventory};
use crate::documents::model::{ActionDocument, ClipSourceDocument, OverlayFileDocument};
use crate::documents::overlay_files::{place_overlay_files, rewrite_overlay_media};
use crate::documents::undo::{Undo, UndoJournal};
use crate::error::TransferError;
use crate::pack::import_conflicts::detect_conflicts;
use crate::pack::import_names::NameBook;
use crate::pack::import_plan::{
    PackConflict, PackImportPlan, PackItemKey, PackItemKind, PackPlanItem, PackResolution,
};
use crate::pack::reader::StagedPack;
use crate::safety_copies::safety_snapshot::write_safety_snapshot;

pub struct PackImportRequest<'a> {
    pub provider: Arc<dyn DataProvider>,
    pub registries: ClosureRegistries<'a>,
    pub staged: &'a StagedPack,
    pub plan: PackImportPlan,
    pub data_dir: PathBuf,
    pub overlay_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackWrittenItem {
    pub key: PackItemKey,
    pub local_id: String,
    pub replaced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackImportOutcome {
    pub safety_snapshot: Option<PathBuf>,
    pub written: Vec<PackWrittenItem>,
    pub skipped: Vec<PackItemKey>,
}

pub async fn apply_pack_import(
    request: PackImportRequest<'_>,
) -> Result<PackImportOutcome, TransferError> {
    let PackImportRequest {
        provider,
        registries,
        staged,
        plan,
        data_dir,
        overlay_root,
    } = request;
    let inventory = load_pack_inventory(provider.as_ref()).await?;
    let detected = detect_conflicts(staged.documents(), &inventory);
    let unchanged = detected.len() == plan.items().len()
        && detected.iter().zip(plan.items()).all(|(now, planned)| {
            now.key == planned.key
                && now.conflict == planned.conflict
                && now.archived == planned.archived
        });
    if !unchanged {
        return Err(TransferError::PackPlanStale);
    }
    let safety_snapshot = if plan.replaces_anything() {
        Some(write_safety_snapshot(&provider, &data_dir, fresh_pre_pack_stem()).await?)
    } else {
        None
    };

    let mut import = PackImport {
        provider: provider.as_ref(),
        registries,
        staged,
        known_media: inventory.media.keys().cloned().collect(),
        inventory,
        overlay_root,
        targets: BTreeMap::new(),
        remap: ReferenceRemap::default(),
        undo: UndoJournal::default(),
        written_queues: BTreeSet::new(),
        written_triggers: BTreeSet::new(),
        written: Vec::new(),
        skipped: Vec::new(),
    };
    let ran = match import.assign_targets(&plan) {
        Ok(()) => import.write_all().await,
        Err(e) => Err(e),
    };
    match ran {
        Ok(()) => Ok(PackImportOutcome {
            safety_snapshot,
            written: import.written,
            skipped: import.skipped,
        }),
        Err(cause) => {
            let journal = std::mem::take(&mut import.undo);
            match journal.roll_back(provider.as_ref()).await {
                Ok(()) => Err(cause),
                Err(failure) => Err(TransferError::PackRollbackIncomplete {
                    cause: cause.to_string(),
                    failure,
                    snapshot: safety_snapshot,
                }),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetMode {
    Create,
    Copy,
    Replace,
    Skip,
}

#[derive(Debug, Clone)]
struct Target {
    local_id: String,
    name: String,
    mode: TargetMode,
}

impl Target {
    fn replaces(&self) -> bool {
        self.mode == TargetMode::Replace
    }
}

struct PackImport<'p, 'a> {
    provider: &'p dyn DataProvider,
    registries: ClosureRegistries<'a>,
    staged: &'a StagedPack,
    inventory: PackInventory,
    known_media: BTreeSet<MediaBlobId>,
    overlay_root: PathBuf,
    targets: BTreeMap<PackItemKey, Target>,
    remap: ReferenceRemap,
    undo: UndoJournal,
    written_queues: BTreeSet<QueueId>,
    written_triggers: BTreeSet<TriggerInstanceId>,
    written: Vec<PackWrittenItem>,
    skipped: Vec<PackItemKey>,
}

fn key(kind: PackItemKind, id: impl Into<String>) -> PackItemKey {
    PackItemKey {
        kind,
        id: id.into(),
    }
}

fn ulid_id<T: From<Ulid>>(raw: &str) -> Result<T, TransferError> {
    raw.parse::<Ulid>()
        .map(T::from)
        .map_err(|_| TransferError::damaged(format!("'{raw}' is not a valid id")))
}

fn namespace_of(kind: PackItemKind) -> InternalNamespace {
    match kind {
        PackItemKind::Media => InternalNamespace::ManagedMedia,
        PackItemKind::Queue => InternalNamespace::Queue,
        PackItemKind::Script => InternalNamespace::Script,
        PackItemKind::Global => InternalNamespace::Global,
        PackItemKind::Clip => InternalNamespace::SoundboardClip,
        PackItemKind::Overlay => InternalNamespace::Overlay,
        PackItemKind::Trigger => InternalNamespace::TriggerInstance,
        PackItemKind::Action => InternalNamespace::Action,
    }
}

fn is_name_keyed(kind: PackItemKind) -> bool {
    matches!(kind, PackItemKind::Script | PackItemKind::Global)
}

fn fresh_id(kind: PackItemKind, pack_id: &str) -> String {
    match kind {
        PackItemKind::Action => ActionId::new().to_string(),
        PackItemKind::Trigger => TriggerInstanceId::new().to_string(),
        PackItemKind::Queue => QueueId::new().to_string(),
        PackItemKind::Script => ScriptId::new().to_string(),
        PackItemKind::Clip => ClipId::new().to_string(),
        PackItemKind::Media | PackItemKind::Overlay | PackItemKind::Global => pack_id.to_owned(),
    }
}

fn mode_of(item: &PackPlanItem) -> (TargetMode, Option<&str>) {
    let local = match &item.conflict {
        PackConflict::None => return (TargetMode::Create, None),
        PackConflict::IdMatch => item.key.id.as_str(),
        PackConflict::NameMatch { local_id } => local_id.as_str(),
    };
    match item.resolution {
        Some(PackResolution::KeepBoth) => (TargetMode::Copy, None),
        Some(PackResolution::Replace) => (TargetMode::Replace, Some(local)),
        Some(PackResolution::Skip) | None => (TargetMode::Skip, Some(local)),
    }
}

impl<'a> PackImport<'_, 'a> {
    fn name_book(&self, kind: PackItemKind) -> NameBook {
        let inventory = &self.inventory;
        match kind {
            PackItemKind::Media => NameBook::default(),
            PackItemKind::Queue => {
                NameBook::new(inventory.queues.values().map(|q| q.name.as_str()))
            }
            PackItemKind::Script => NameBook::new(inventory.script_names()),
            PackItemKind::Global => NameBook::new(inventory.globals.keys().map(String::as_str)),
            PackItemKind::Clip => NameBook::new(inventory.clips.values().map(|c| c.name.as_str())),
            PackItemKind::Overlay => {
                NameBook::new(inventory.overlays.values().map(|o| o.display_name.as_str()))
            }
            PackItemKind::Trigger => {
                NameBook::new(inventory.triggers.values().map(|t| t.name.as_str()))
            }
            PackItemKind::Action => {
                NameBook::new(inventory.actions.values().map(|a| a.name.as_str()))
            }
        }
    }

    fn local_name(&self, kind: PackItemKind, local_id: &str) -> Option<String> {
        let inventory = &self.inventory;
        match kind {
            PackItemKind::Media => None,
            PackItemKind::Queue => ulid_id::<QueueId>(local_id)
                .ok()
                .and_then(|id| inventory.queues.get(&id))
                .map(|queue| queue.name.clone()),
            PackItemKind::Script => ulid_id::<ScriptId>(local_id)
                .ok()
                .and_then(|id| inventory.stored_script(id))
                .map(|stored| stored.row.name),
            PackItemKind::Global => Some(local_id.to_owned()),
            PackItemKind::Clip => ulid_id::<ClipId>(local_id)
                .ok()
                .and_then(|id| inventory.clips.get(&id))
                .map(|clip| clip.name.clone()),
            PackItemKind::Overlay => inventory
                .overlays
                .get(&OverlayId::new(local_id))
                .map(|overlay| overlay.display_name.clone()),
            PackItemKind::Trigger => ulid_id::<TriggerInstanceId>(local_id)
                .ok()
                .and_then(|id| inventory.stored_trigger(&id))
                .map(|stored| stored.row.name),
            PackItemKind::Action => ulid_id::<ActionId>(local_id)
                .ok()
                .and_then(|id| inventory.stored_action(&id))
                .map(|stored| stored.row.name),
        }
    }

    fn assign_targets(&mut self, plan: &PackImportPlan) -> Result<(), TransferError> {
        let mut books: BTreeMap<PackItemKind, NameBook> = BTreeMap::new();
        for item in plan.items() {
            let kind = item.key.kind;
            let book = books.entry(kind).or_insert_with(|| self.name_book(kind));
            let (mode, local) = mode_of(item);
            let mut target = match (mode, local) {
                (TargetMode::Copy, _) => Target {
                    local_id: fresh_id(kind, &item.key.id),
                    name: book.claim_copy(&item.name),
                    mode,
                },
                (TargetMode::Replace | TargetMode::Skip, Some(local)) => {
                    let local_name = self
                        .local_name(kind, local)
                        .ok_or(TransferError::PackPlanStale)?;
                    let name = match (mode, kind) {
                        (TargetMode::Skip, _) => local_name,
                        (_, PackItemKind::Script)
                            if item.name != local_name && book.is_taken(&item.name) =>
                        {
                            book.claim_copy(&item.name)
                        }
                        _ => item.name.clone(),
                    };
                    Target {
                        local_id: local.to_owned(),
                        name,
                        mode,
                    }
                }
                _ => {
                    book.claim(&item.name);
                    Target {
                        local_id: item.key.id.clone(),
                        name: item.name.clone(),
                        mode: TargetMode::Create,
                    }
                }
            };
            if kind == PackItemKind::Global {
                target.local_id = target.name.clone();
            }
            let namespace = namespace_of(kind);
            let (from, to) = if is_name_keyed(kind) {
                (&item.name, &target.name)
            } else {
                (&item.key.id, &target.local_id)
            };
            if from != to {
                self.remap.insert(namespace, from.clone(), to.clone());
            }
            self.targets.insert(item.key.clone(), target);
        }
        Ok(())
    }

    fn writable(&mut self, key: &PackItemKey) -> Option<Target> {
        let target = self.targets.get(key)?.clone();
        if target.mode == TargetMode::Skip {
            self.skipped.push(key.clone());
            return None;
        }
        Some(target)
    }

    fn record_written(&mut self, key: PackItemKey, local_id: String, target: &Target) {
        self.written.push(PackWrittenItem {
            key,
            local_id,
            replaced: target.replaces(),
        });
    }

    fn is_written(&self, key: &PackItemKey) -> bool {
        self.targets
            .get(key)
            .is_some_and(|target| target.mode != TargetMode::Skip)
    }

    async fn write_all(&mut self) -> Result<(), TransferError> {
        self.write_media().await?;
        self.write_queues().await?;
        self.write_scripts().await?;
        self.write_globals().await?;
        self.write_clips().await?;
        self.write_overlays().await?;
        self.write_triggers().await?;
        let linkable = self.write_actions().await?;
        self.link_triggers(linkable).await
    }

    fn needed_media(&self) -> BTreeSet<String> {
        let documents = self.staged.documents();
        let clips = documents
            .clips
            .iter()
            .filter(|clip| self.is_written(&key(PackItemKind::Clip, clip.id.to_string())))
            .filter_map(|clip| match &clip.source {
                ClipSourceDocument::Media { media_id } => Some(media_id.clone()),
                ClipSourceDocument::File { .. } | ClipSourceDocument::Missing => None,
            });
        let overlays = documents
            .overlays
            .iter()
            .filter(|overlay| self.is_written(&key(PackItemKind::Overlay, overlay.id.clone())))
            .flat_map(|overlay| image_references(&overlay.config))
            .map(|reference| reference.image);
        clips.chain(overlays).collect()
    }

    async fn store_staged(
        &mut self,
        pack_path: &str,
        label: &str,
    ) -> Result<MediaBlobId, TransferError> {
        let source =
            self.staged
                .file(pack_path)
                .ok_or_else(|| TransferError::ContentsMismatch {
                    path: pack_path.to_owned(),
                })?;
        let bytes = tokio::fs::read(&source).await?;
        let blob = self.provider.media_repo().store(label, bytes).await?;
        if self.known_media.insert(blob.id.clone()) {
            self.undo.push(Undo::Media(blob.id.clone()));
        }
        Ok(blob.id)
    }

    async fn write_media(&mut self) -> Result<(), TransferError> {
        let needed = self.needed_media();
        let staged = self.staged;
        for media in &staged.documents().media {
            let item = key(PackItemKind::Media, media.id.clone());
            if !needed.contains(&media.id) {
                self.skipped.push(item);
                continue;
            }
            let blob = self.store_staged(&media.file, &media.label).await?;
            if blob.as_str() != media.id {
                self.remap.insert(
                    InternalNamespace::ManagedMedia,
                    media.id.clone(),
                    blob.as_str(),
                );
            }
            self.written.push(PackWrittenItem {
                key: item,
                local_id: blob.as_str().to_owned(),
                replaced: false,
            });
        }
        Ok(())
    }

    async fn write_queues(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.queue_repo();
        let staged = self.staged;
        for document in &staged.documents().queues {
            let item = key(PackItemKind::Queue, document.id.to_string());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let mut queue = queue_of(document);
            queue.id = ulid_id(&target.local_id)?;
            queue.name = target.name.clone();
            let previous = target
                .replaces()
                .then(|| self.inventory.queues.get(&queue.id).cloned())
                .flatten();
            repo.save(&queue).await?;
            self.undo.push(Undo::Queue {
                id: queue.id,
                previous,
            });
            self.written_queues.insert(queue.id);
            self.record_written(item, queue.id.to_string(), &target);
        }
        Ok(())
    }

    async fn write_scripts(&mut self) -> Result<(), TransferError> {
        let staged = self.staged;
        for document in &staged.documents().scripts {
            let item = key(PackItemKind::Script, document.id.to_string());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let id: ScriptId = ulid_id(&target.local_id)?;
            let previous = target
                .replaces()
                .then(|| self.inventory.stored_script(id))
                .flatten();
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            let previous = previous.map(|stored| stored.row);
            let now = OffsetDateTime::now_utc();
            let record = ScriptRecord {
                id,
                name: target.name.clone(),
                body: document.body.clone(),
                contract: document.contract.clone(),
                enabled: document.enabled,
                created_at: previous.as_ref().map_or(now, |script| script.created_at),
                last_modified: now,
            };
            ScriptRepo::save(self.provider, record).await?;
            self.undo.push(Undo::Script {
                id,
                previous,
                archived,
            });
            if archived {
                self.provider.restore_script(id).await?;
            }
            self.record_written(item, id.to_string(), &target);
        }
        Ok(())
    }

    async fn write_globals(&mut self) -> Result<(), TransferError> {
        let staged = self.staged;
        for document in &staged.documents().globals {
            let item = key(PackItemKind::Global, document.name.clone());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let previous = target
                .replaces()
                .then(|| self.inventory.stored_global(&target.name))
                .flatten();
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            GlobalsRepo::set(
                self.provider,
                &target.name,
                document.default_value.clone(),
                document.persisted,
            )
            .await?;
            self.undo.push(Undo::Global {
                name: target.name.clone(),
                previous: previous.map(|stored| stored.row),
                archived,
            });
            self.record_written(item, target.name.clone(), &target);
        }
        Ok(())
    }

    async fn write_clips(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.soundboard_clips_repo();
        let media = self.provider.media_repo();
        let staged = self.staged;
        for document in &staged.documents().clips {
            let item = key(PackItemKind::Clip, document.id.to_string());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let id: ClipId = ulid_id(&target.local_id)?;
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
            let previous = target
                .replaces()
                .then(|| self.inventory.clips.get(&id).cloned())
                .flatten();
            let file_path = match &blob {
                Some(blob) => media.resolve(blob).await?,
                None => previous
                    .as_ref()
                    .map(|clip| clip.file_path.clone())
                    .unwrap_or_default(),
            };
            let clip = StoredClip {
                id,
                name: target.name.clone(),
                file_path,
                volume: document.volume,
                output_device: previous
                    .as_ref()
                    .map_or(OutputDevice::Default, |clip| clip.output_device.clone()),
                hotkey: previous.as_ref().and_then(|clip| clip.hotkey.clone()),
                created_at: previous
                    .as_ref()
                    .map_or_else(OffsetDateTime::now_utc, |clip| clip.created_at),
                category: document.category.clone(),
                loop_playback: document.loop_playback,
                duration_secs: document.duration_secs,
                builtin_id: document.builtin_id.clone(),
            };
            repo.save(&clip).await?;
            self.undo.push(Undo::Clip { id, previous });
            if let Some(blob) = blob {
                self.retain(clip_source_referrer(id), &blob).await?;
            }
            self.record_written(item, id.to_string(), &target);
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
        self.undo.push(Undo::Referrer { referrer, previous });
        Ok(())
    }

    async fn write_overlays(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.overlay_repo();
        let staged = self.staged;
        for document in &staged.documents().overlays {
            let item = key(PackItemKind::Overlay, document.id.clone());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let mut config = document.config.clone();
            rewrite_overlay_media(&mut config, &self.remap);
            let (mut definition, previous) = if target.replaces() {
                let previous = self
                    .inventory
                    .overlays
                    .get(&OverlayId::new(target.local_id.as_str()))
                    .cloned()
                    .ok_or(TransferError::PackPlanStale)?;
                (previous.clone(), Some(previous))
            } else {
                let created = repo
                    .create(
                        &target.name,
                        &document.kind_id,
                        document.config_schema_version,
                    )
                    .await?;
                self.undo.push(Undo::Overlay {
                    id: created.id.clone(),
                    previous: None,
                });
                (created, None)
            };
            definition.display_name = target.name.clone();
            definition.kind_id = document.kind_id.clone();
            definition.enabled = document.enabled;
            definition.config = config;
            definition.config_schema_version = document.config_schema_version;
            definition.source_overrides = document.source_overrides.clone();
            repo.save(&definition).await?;
            if previous.is_some() {
                self.undo.push(Undo::Overlay {
                    id: definition.id.clone(),
                    previous,
                });
            }
            if definition.id.as_str() != document.id {
                self.remap.insert(
                    InternalNamespace::Overlay,
                    document.id.clone(),
                    definition.id.as_str(),
                );
            }
            self.write_overlay_files(&definition.id, &document.files)
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
            self.record_written(item, definition.id.as_str().to_owned(), &target);
        }
        Ok(())
    }

    async fn write_overlay_files(
        &mut self,
        id: &OverlayId,
        files: &[OverlayFileDocument],
    ) -> Result<(), TransferError> {
        let staged = self.staged;
        let undo = &mut self.undo;
        place_overlay_files(
            &self.overlay_root,
            id,
            files,
            |file| staged.file(file),
            |entry| undo.push(entry),
        )
        .await
    }

    async fn write_triggers(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.trigger_instance_repo();
        let staged = self.staged;
        for document in &staged.documents().triggers {
            let item = key(PackItemKind::Trigger, document.id.to_string());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let mut trigger = trigger_of(document);
            trigger.id = ulid_id(&target.local_id)?;
            trigger.name = target.name.clone();
            if target.mode == TargetMode::Copy {
                trigger.user_defined = true;
            }
            self.registries
                .catalog()
                .rewrite_trigger_instance(&mut trigger, &self.remap);
            let previous = target
                .replaces()
                .then(|| self.inventory.stored_trigger(&trigger.id))
                .flatten();
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            repo.save(&trigger).await?;
            self.undo.push(Undo::Trigger {
                id: trigger.id,
                previous: previous.map(|stored| stored.row),
                archived,
            });
            if archived {
                repo.restore(trigger.id).await?;
            }
            self.written_triggers.insert(trigger.id);
            self.record_written(item, trigger.id.to_string(), &target);
        }
        Ok(())
    }

    async fn write_actions(
        &mut self,
    ) -> Result<Vec<(ActionId, &'a ActionDocument)>, TransferError> {
        let repo = self.provider.action_repo();
        let staged = self.staged;
        let mut linkable = Vec::new();
        for document in &staged.documents().actions {
            let item = key(PackItemKind::Action, document.id.to_string());
            let Some(target) = self.writable(&item) else {
                continue;
            };
            let mut action = action_of(document);
            action.id = ulid_id(&target.local_id)?;
            action.name = target.name.clone();
            self.registries
                .catalog()
                .rewrite_action(&mut action, &self.remap)
                .map_err(TransferError::damaged)?;
            let queue_available = self.written_queues.contains(&action.queue_id)
                || self.inventory.queues.contains_key(&action.queue_id);
            if !queue_available {
                return Err(TransferError::PackActionWithoutQueue {
                    action: action.name,
                });
            }
            let previous = target
                .replaces()
                .then(|| self.inventory.stored_action(&action.id))
                .flatten();
            let archived = previous.as_ref().is_some_and(|stored| stored.archived);
            let created = previous.is_none();
            repo.save(&action).await?;
            self.undo.push(Undo::Action {
                id: action.id,
                previous: previous.map(|stored| stored.row),
                archived,
            });
            if created || archived {
                repo.restore(action.id).await?;
            }
            linkable.push((action.id, document));
            self.record_written(item, action.id.to_string(), &target);
        }
        Ok(linkable)
    }

    async fn link_triggers(
        &mut self,
        linkable: Vec<(ActionId, &'a ActionDocument)>,
    ) -> Result<(), TransferError> {
        let repo = self.provider.trigger_instance_repo();
        for (action, document) in linkable {
            let linked: BTreeSet<TriggerInstanceId> = repo
                .list_for_action(action)
                .await?
                .into_iter()
                .map(|trigger| trigger.id)
                .collect();
            let mut position = i64::try_from(linked.len()).unwrap_or(i64::MAX);
            for pack_trigger in &document.trigger_ids {
                let trigger = match self.remap.get(
                    InternalNamespace::TriggerInstance,
                    &pack_trigger.to_string(),
                ) {
                    Some(local) => ulid_id(local)?,
                    None => *pack_trigger,
                };
                let exists = self.written_triggers.contains(&trigger)
                    || self.inventory.triggers.contains_key(&trigger);
                if linked.contains(&trigger) || !exists {
                    continue;
                }
                repo.link_action(action, trigger, position).await?;
                self.undo.push(Undo::Link { action, trigger });
                position = position.saturating_add(1);
            }
        }
        Ok(())
    }
}
