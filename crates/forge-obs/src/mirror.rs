use forge_platform_core::DetailSection;
use forge_types::RecoverRwLock;
use std::collections::HashMap;
use std::sync::RwLock;

use crate::catalog::ObsCatalog;
use crate::item_ids::SceneItemIds;
use crate::source::SourceInfo;

#[derive(Default)]
struct MirrorState {
    catalog: ObsCatalog,
    ids: SceneItemIds,
}

#[derive(Default)]
pub(crate) struct ObsMirror {
    state: RwLock<MirrorState>,
}

pub(crate) struct AppliedEvent {
    pub(crate) previous_scene: Option<String>,
    pub(crate) visibility: Option<forge_events::Event>,
    pub(crate) lock: Option<forge_events::Event>,
}

pub(crate) struct FetchedSnapshot {
    pub(crate) scenes: Vec<String>,
    pub(crate) audio_inputs: Vec<String>,
    pub(crate) sources: HashMap<String, Vec<SourceInfo>>,
    pub(crate) current_scene: Option<String>,
    pub(crate) current_preview_scene: Option<String>,
    pub(crate) ids: SceneItemIds,
}

pub(crate) struct FetchedTopology {
    pub(crate) scenes: Vec<String>,
    pub(crate) audio_inputs: Vec<String>,
    pub(crate) sources: HashMap<String, Vec<(String, Option<String>)>>,
    pub(crate) current_scene: Option<String>,
    pub(crate) current_preview_scene: Option<String>,
}

impl ObsMirror {
    fn read_state<R>(&self, read: impl FnOnce(&MirrorState) -> R) -> R {
        read(&self.state.read_recover())
    }

    fn write_state<R>(&self, write: impl FnOnce(&mut MirrorState) -> R) -> R {
        write(&mut self.state.write_recover())
    }

    pub(crate) fn read<R>(&self, read: impl FnOnce(&ObsCatalog) -> R) -> R {
        self.read_state(|state| read(&state.catalog))
    }

    pub(crate) fn revision(&self) -> u64 {
        self.read_state(|state| state.catalog.revision)
    }

    pub(crate) fn option_fingerprint(&self) -> u64 {
        self.read_state(|state| state.catalog.option_fingerprint())
    }

    pub(crate) fn content_rows(&self) -> Vec<DetailSection> {
        self.read_state(|state| crate::catalog::catalog_sections(&state.catalog, true))
    }

    pub(crate) fn item_id(&self, scene: &str, source: &str) -> Option<i64> {
        self.read_state(|state| state.ids.item_id(scene, source))
    }

    pub(crate) fn remember_item(&self, scene: &str, item_id: i64, source: &str) {
        self.write_state(|state| state.ids.insert(scene, item_id, source));
    }

    pub(crate) fn forget_items(&self) {
        self.write_state(|state| state.ids.clear());
    }

    pub(crate) fn record_input_volume(&self, input: &str, db: f32) {
        self.write_state(|state| {
            let catalog = &mut state.catalog;
            for sources in catalog.sources.values_mut() {
                for info in sources.iter_mut() {
                    if info.name == input {
                        info.audio_db = Some(db);
                    }
                }
            }
            catalog.revision = catalog.revision.wrapping_add(1);
        });
    }

    pub(crate) fn apply_event(&self, ev: &obws::events::Event) -> AppliedEvent {
        self.write_state(|state| {
            let MirrorState { catalog, ids } = state;
            let previous_scene = match ev {
                obws::events::Event::CurrentProgramSceneChanged { .. } => {
                    catalog.current_scene.clone()
                }
                obws::events::Event::CurrentPreviewSceneChanged { .. } => {
                    catalog.current_preview_scene.clone()
                }
                _ => None,
            };
            let mut visibility = None;
            let mut lock = None;

            crate::events::apply_catalog_update(ev, catalog);
            crate::events::apply_item_id_update(ev, ids);

            match ev {
                obws::events::Event::SceneItemEnableStateChanged {
                    scene,
                    item_id,
                    enabled,
                    ..
                } => {
                    if let Some(name) = ids.source_name(&scene.name, *item_id) {
                        if let Some(info) = catalog
                            .sources
                            .get_mut(&scene.name)
                            .and_then(|sources| sources.iter_mut().find(|s| s.name == name))
                        {
                            info.visible = *enabled;
                        }
                        catalog.revision = catalog.revision.wrapping_add(1);
                        visibility = Some(crate::events::map_scene_item_visibility(
                            &scene.name,
                            &name,
                            *enabled,
                        ));
                    }
                }
                obws::events::Event::SceneItemLockStateChanged {
                    scene,
                    item_id,
                    locked,
                    ..
                } => {
                    if let Some(name) = ids.source_name(&scene.name, *item_id) {
                        if let Some(info) = catalog
                            .sources
                            .get_mut(&scene.name)
                            .and_then(|sources| sources.iter_mut().find(|s| s.name == name))
                        {
                            info.locked = *locked;
                        }
                        catalog.revision = catalog.revision.wrapping_add(1);
                        lock = Some(crate::events::map_scene_item_lock(
                            &scene.name,
                            &name,
                            *locked,
                        ));
                    }
                }
                obws::events::Event::SceneItemCreated { scene, source, .. } => {
                    let known_kind = catalog
                        .sources
                        .values()
                        .flatten()
                        .find(|s| s.name == source.name && s.kind.is_some())
                        .and_then(|s| s.kind.clone());
                    if let Some(kind) = known_kind
                        && let Some(info) = catalog
                            .sources
                            .get_mut(&scene.name)
                            .and_then(|items| items.iter_mut().find(|s| s.name == source.name))
                    {
                        info.kind = Some(kind);
                    }
                }
                _ => {}
            }
            AppliedEvent {
                previous_scene,
                visibility,
                lock,
            }
        })
    }

    pub(crate) fn replace_snapshot(&self, started_at: u64, fetched: FetchedSnapshot) -> bool {
        self.write_state(|state| {
            if state.catalog.revision != started_at {
                return false;
            }
            let catalog = &mut state.catalog;
            catalog.scenes = fetched.scenes;
            catalog.audio_inputs = fetched.audio_inputs;
            catalog.sources = fetched.sources;
            catalog.current_scene = fetched.current_scene;
            catalog.current_preview_scene = fetched.current_preview_scene;
            state.ids = fetched.ids;
            true
        })
    }

    pub(crate) fn reconcile(
        &self,
        started_at: u64,
        fetched: FetchedTopology,
        ids: SceneItemIds,
    ) -> bool {
        self.write_state(|state| {
            if state.catalog.revision != started_at {
                return false;
            }
            merge_reconciled_topology(&mut state.catalog, fetched);
            state.ids = ids;
            true
        })
    }

    #[cfg(test)]
    pub(crate) fn edit(&self, edit: impl FnOnce(&mut ObsCatalog)) {
        self.write_state(|state| edit(&mut state.catalog));
    }

    #[cfg(test)]
    pub(crate) fn write_for_test(&self) -> CatalogGuard<'_> {
        CatalogGuard(self.state.write_recover())
    }
}

#[cfg(test)]
pub(crate) struct CatalogGuard<'a>(std::sync::RwLockWriteGuard<'a, MirrorState>);

#[cfg(test)]
impl std::ops::Deref for CatalogGuard<'_> {
    type Target = ObsCatalog;

    fn deref(&self) -> &ObsCatalog {
        &self.0.catalog
    }
}

#[cfg(test)]
impl std::ops::DerefMut for CatalogGuard<'_> {
    fn deref_mut(&mut self) -> &mut ObsCatalog {
        &mut self.0.catalog
    }
}

pub(crate) fn merge_reconciled_topology(catalog: &mut ObsCatalog, fetched: FetchedTopology) {
    let mut sources_by_scene: HashMap<String, Vec<SourceInfo>> =
        HashMap::with_capacity(fetched.sources.len());
    for (scene, entries) in fetched.sources {
        let live = catalog.sources.get(&scene);
        let mut infos = Vec::with_capacity(entries.len());
        for (name, kind) in entries {
            let live_info = live.and_then(|known| known.iter().find(|s| s.name == name));
            infos.push(SourceInfo {
                visible: live_info.map(|s| s.visible).unwrap_or(true),
                locked: live_info.map(|s| s.locked).unwrap_or(false),
                audio_db: live_info.and_then(|s| s.audio_db),
                kind: kind.or_else(|| live_info.and_then(|s| s.kind.clone())),
                name,
            });
        }
        sources_by_scene.insert(scene, infos);
    }

    catalog.scenes = fetched.scenes;
    catalog.audio_inputs = fetched.audio_inputs;
    catalog.sources = sources_by_scene;
    catalog.current_scene = fetched.current_scene;
    catalog.current_preview_scene = fetched.current_preview_scene;
}
#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(scene: &str, source: &str, item_id: i64) -> FetchedSnapshot {
        let mut ids = SceneItemIds::default();
        ids.insert(scene, item_id, source);
        FetchedSnapshot {
            scenes: vec![scene.to_owned()],
            audio_inputs: Vec::new(),
            sources: HashMap::new(),
            current_scene: Some(scene.to_owned()),
            current_preview_scene: None,
            ids,
        }
    }

    #[test]
    fn a_snapshot_replaces_the_catalog_and_the_item_ids_together() {
        let mirror = ObsMirror::default();
        mirror.remember_item("Old", 1, "Cam");

        let replaced = mirror.replace_snapshot(mirror.revision(), snapshot("Main", "Logo", 7));

        assert!(replaced);
        assert_eq!(
            (
                mirror.read(|catalog| catalog.current_scene.clone()),
                mirror.item_id("Main", "Logo"),
                mirror.item_id("Old", "Cam"),
            ),
            (Some("Main".to_owned()), Some(7), None)
        );
    }

    #[test]
    fn a_snapshot_fetched_before_a_live_change_is_discarded() {
        let mirror = ObsMirror::default();
        let started_at = mirror.revision();
        mirror.record_input_volume("Mic", 0.0);

        let replaced = mirror.replace_snapshot(started_at, snapshot("Main", "Logo", 7));

        assert!(!replaced);
        assert_eq!(mirror.read(|catalog| catalog.current_scene.clone()), None);
        assert_eq!(mirror.item_id("Main", "Logo"), None);
    }

    #[test]
    fn a_reconcile_fetched_before_a_live_change_is_discarded() {
        let mirror = ObsMirror::default();
        let started_at = mirror.revision();
        mirror.record_input_volume("Mic", 0.0);
        let fetched = FetchedTopology {
            scenes: vec!["Main".to_owned()],
            audio_inputs: Vec::new(),
            sources: HashMap::new(),
            current_scene: None,
            current_preview_scene: None,
        };

        assert!(!mirror.reconcile(started_at, fetched, SceneItemIds::default()));
        assert!(mirror.read(|catalog| catalog.scenes.is_empty()));
    }
}
