use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use forge_storage::{DataProvider, TriggerChanges};
use forge_types::{
    ActionId, PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId, Variant,
};

use crate::backend::HotkeyId;
use crate::binding::{BindingHalf, BindingRow, COMBO_FIELD, HotkeyEdge};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;
use crate::reconciler::HotkeyReconciler;

fn combo_of(instance: &TriggerInstance) -> Option<&String> {
    match instance.overrides.get(COMBO_FIELD) {
        Some(Variant::String(combo)) => Some(combo),
        _ => None,
    }
}

fn canonical_combo_of(instance: &TriggerInstance) -> Option<String> {
    combo_of(instance).map(|combo| {
        HotkeyCombo::parse(combo)
            .map(|parsed| parsed.as_str().to_owned())
            .unwrap_or_else(|_| combo.clone())
    })
}

pub async fn load_bindings(
    backend: Arc<dyn DataProvider>,
    registered: Vec<(HotkeyId, String)>,
) -> Result<Vec<BindingRow>, HotkeyError> {
    let triggers = backend.trigger_instance_repo();
    let actions = backend.action_repo();
    let instances = triggers.list_all().await?;

    let mut grouped: BTreeMap<String, (Option<BindingHalf>, Option<BindingHalf>)> = BTreeMap::new();
    for instance in instances {
        let Some(edge) = HotkeyEdge::from_kind(&instance.kind_id) else {
            continue;
        };
        let Some(combo) = canonical_combo_of(&instance) else {
            continue;
        };
        let linked = triggers.actions_using(instance.id).await?;
        let mut action = None;
        if let Some(first) = linked.first()
            && let Some(found) = actions.get(*first).await?
        {
            action = Some((found.id, found.name));
        }
        let half = BindingHalf {
            instance_id: instance.id,
            enabled: instance.enabled,
            action,
        };
        let slot = grouped.entry(combo).or_default();
        let target = match edge {
            HotkeyEdge::Press => &mut slot.0,
            HotkeyEdge::Release => &mut slot.1,
        };
        if target.is_none() {
            *target = Some(half);
        }
    }

    let rows = grouped
        .into_iter()
        .filter_map(|(combo, (press, release))| {
            let key = press.as_ref().or(release.as_ref())?.instance_id;
            Some(BindingRow {
                key,
                registered: registered.iter().any(|(_, known)| known == &combo),
                combo,
                press,
                release,
            })
        })
        .collect();
    Ok(rows)
}

async fn hotkey_instances_for_combo(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<Vec<TriggerInstance>, HotkeyError> {
    let instances = backend.trigger_instance_repo().list_all().await?;
    Ok(instances
        .into_iter()
        .filter(|instance| HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter(|instance| {
            canonical_combo_of(instance).is_some_and(|existing| existing == combo_str)
        })
        .collect())
}

async fn drop_instances(
    backend: &Arc<dyn DataProvider>,
    removed: Vec<TriggerInstanceId>,
) -> Result<(), HotkeyError> {
    backend
        .trigger_instance_repo()
        .apply_changes(&TriggerChanges {
            removed,
            ..TriggerChanges::default()
        })
        .await
        .map_err(HotkeyError::from)
}

pub async fn cleanup_stale_combo_instances(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<(), HotkeyError> {
    let stale = hotkey_instances_for_combo(backend, combo_str).await?;
    drop_instances(backend, stale.iter().map(|instance| instance.id).collect()).await
}

async fn combo_edge_ids(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
    edge: HotkeyEdge,
) -> Result<Vec<TriggerInstanceId>, HotkeyError> {
    Ok(hotkey_instances_for_combo(backend, combo_str)
        .await?
        .iter()
        .filter(|instance| instance.kind_id == edge.kind_id())
        .map(|instance| instance.id)
        .collect())
}

#[cfg(test)]
async fn remove_combo_edge(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
    edge: HotkeyEdge,
) -> Result<(), HotkeyError> {
    drop_instances(backend, combo_edge_ids(backend, combo_str, edge).await?).await
}

fn binding_instance(combo: String, edge: HotkeyEdge) -> TriggerInstance {
    let mut overrides = BTreeMap::new();
    overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo.clone()));
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: edge.kind_id().to_owned(),
        name: combo,
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

pub fn bound_hotkey_combos(instances: &[TriggerInstance]) -> BTreeSet<String> {
    instances
        .iter()
        .filter(|instance| HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter_map(canonical_combo_of)
        .collect()
}

pub fn persisted_hotkey_combos(instances: &[TriggerInstance]) -> BTreeSet<String> {
    instances
        .iter()
        .filter(|instance| instance.enabled && HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter_map(canonical_combo_of)
        .collect()
}

pub async fn do_bind(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> Result<(), HotkeyError> {
    let combo = HotkeyCombo::parse(&combo_str)?;
    reconciler.claim(combo).await?;
    let stored = store_binding(&backend, combo_str, edge, action_id).await;
    reconciler.reconcile().await;
    stored
}

async fn store_binding(
    backend: &Arc<dyn DataProvider>,
    combo_str: String,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> Result<(), HotkeyError> {
    let removed = combo_edge_ids(backend, &combo_str, edge).await?;
    let instance = binding_instance(combo_str, edge);
    let linked = vec![(action_id, instance.id)];
    backend
        .trigger_instance_repo()
        .apply_changes(&TriggerChanges {
            removed,
            saved: vec![instance],
            linked,
            ..TriggerChanges::default()
        })
        .await
        .map_err(HotkeyError::from)
}

pub async fn delete_binding(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
) -> Result<(), HotkeyError> {
    let cleaned = cleanup_stale_combo_instances(&backend, &combo_str).await;
    reconciler.reconcile().await;
    cleaned
}

pub async fn delete_binding_half(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
) -> Result<(), HotkeyError> {
    drop_instances(&backend, vec![instance_id]).await
}

pub async fn set_binding_enabled(
    backend: Arc<dyn DataProvider>,
    instance_ids: Vec<TriggerInstanceId>,
    enabled: bool,
) -> Result<(), HotkeyError> {
    backend
        .trigger_instance_repo()
        .set_all_enabled(&instance_ids, enabled)
        .await
        .map_err(HotkeyError::from)
}

pub async fn rebind_combo(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    previous_combo: String,
    combo_str: String,
) -> Result<(), HotkeyError> {
    let target_combo = HotkeyCombo::parse(&combo_str)?;
    let previous_combo = HotkeyCombo::parse(&previous_combo)
        .map(|parsed| parsed.as_str().to_owned())
        .unwrap_or(previous_combo);
    if previous_combo == target_combo.as_str() {
        return Ok(());
    }
    reconciler.claim(target_combo.clone()).await?;
    let moved = move_combo_instances(&backend, &previous_combo, target_combo.as_str()).await;
    reconciler.reconcile().await;
    moved
}

async fn move_combo_instances(
    backend: &Arc<dyn DataProvider>,
    previous_combo: &str,
    target_combo: &str,
) -> Result<(), HotkeyError> {
    let siblings = hotkey_instances_for_combo(backend, previous_combo).await?;
    let changes = TriggerChanges {
        removed: displaced_ids(backend, target_combo, &siblings).await?,
        saved: siblings
            .into_iter()
            .map(|sibling| recombed(sibling, target_combo))
            .collect(),
        ..TriggerChanges::default()
    };
    backend
        .trigger_instance_repo()
        .apply_changes(&changes)
        .await
        .map_err(HotkeyError::from)
}

async fn displaced_ids(
    backend: &Arc<dyn DataProvider>,
    target_combo: &str,
    moving: &[TriggerInstance],
) -> Result<Vec<TriggerInstanceId>, HotkeyError> {
    Ok(hotkey_instances_for_combo(backend, target_combo)
        .await?
        .iter()
        .map(|stale| stale.id)
        .filter(|stale| !moving.iter().any(|sibling| sibling.id == *stale))
        .collect())
}

fn recombed(instance: TriggerInstance, combo: &str) -> TriggerInstance {
    let mut overrides = instance.overrides.clone();
    overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()));
    TriggerInstance {
        name: combo.to_owned(),
        overrides,
        ..instance
    }
}

pub struct BindingUpdate {
    pub instance_id: TriggerInstanceId,
    pub combo: String,
    pub edge: HotkeyEdge,
    pub action_id: ActionId,
}

pub async fn update_binding(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    update: BindingUpdate,
) -> Result<(), HotkeyError> {
    let combo = HotkeyCombo::parse(&update.combo)?;
    let source = backend
        .trigger_instance_repo()
        .get(update.instance_id)
        .await?
        .ok_or(HotkeyError::BindingNotFound)?;
    let previous_combo = canonical_combo_of(&source);
    if previous_combo.as_deref() != Some(combo.as_str()) {
        reconciler.claim(combo.clone()).await?;
    }
    let changes = binding_changes(&backend, source, previous_combo, &combo, &update).await;
    let applied = match changes {
        Ok(changes) => backend
            .trigger_instance_repo()
            .apply_changes(&changes)
            .await
            .map_err(HotkeyError::from),
        Err(refusal) => Err(refusal),
    };
    reconciler.reconcile().await;
    applied
}

async fn binding_changes(
    backend: &Arc<dyn DataProvider>,
    source: TriggerInstance,
    previous_combo: Option<String>,
    combo: &HotkeyCombo,
    update: &BindingUpdate,
) -> Result<TriggerChanges, HotkeyError> {
    let target_combo = combo.as_str();
    let moved = previous_combo.as_deref() != Some(target_combo);
    let edge_kind = update.edge.kind_id();
    let mut changes = TriggerChanges::default();

    let siblings = match &previous_combo {
        Some(previous) => hotkey_instances_for_combo(backend, previous).await?,
        None => vec![source.clone()],
    };
    if siblings
        .iter()
        .any(|sibling| sibling.id != source.id && sibling.kind_id == edge_kind)
    {
        return Err(edge_taken(target_combo));
    }
    if moved {
        changes.removed = displaced_ids(backend, target_combo, &siblings).await?;
    }
    for sibling in siblings {
        let mut moved_sibling = recombed(sibling, target_combo);
        if moved_sibling.id == source.id {
            edge_kind.clone_into(&mut moved_sibling.kind_id);
        }
        changes.saved.push(moved_sibling);
    }

    let linked = backend
        .trigger_instance_repo()
        .actions_using(update.instance_id)
        .await?;
    if !linked.contains(&update.action_id) {
        changes.unlinked = linked
            .into_iter()
            .map(|previous| (previous, update.instance_id))
            .collect();
        changes.linked = vec![(update.action_id, update.instance_id)];
    }
    Ok(changes)
}

fn edge_taken(combo: &str) -> HotkeyError {
    HotkeyError::EdgeTaken {
        combo: combo.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::event_kinds;

    async fn backend() -> (Arc<dyn DataProvider>, tempfile::TempDir) {
        let media_root = tempfile::tempdir().expect("a temporary media root");
        let backend = forge_storage_sqlite::SqliteBackend::open_for_test(
            "sqlite::memory:",
            [0x11; 32],
            media_root.path().to_owned(),
            None,
        )
        .await
        .expect("an in-memory backend");
        (Arc::new(backend), media_root)
    }

    async fn seed_instance(
        backend: &Arc<dyn DataProvider>,
        kind_id: &str,
        combo: &str,
    ) -> TriggerInstanceId {
        let mut instance = binding_instance(combo.to_owned(), HotkeyEdge::Press);
        instance.kind_id = kind_id.to_owned();
        backend
            .trigger_instance_repo()
            .save(&instance)
            .await
            .unwrap();
        instance.id
    }

    #[tokio::test]
    async fn remove_combo_edge_drops_only_the_named_edge() {
        let (backend, _media_root) = backend().await;
        let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, "Ctrl+F1").await;
        let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, "Ctrl+F1").await;

        remove_combo_edge(&backend, "Ctrl+F1", HotkeyEdge::Press)
            .await
            .unwrap();

        let repo = backend.trigger_instance_repo();
        assert!(repo.get(press).await.unwrap().is_none());
        assert!(repo.get(release).await.unwrap().is_some());
    }

    #[test]
    fn binding_instance_leaves_both_edges_without_a_cooldown() {
        for edge in [HotkeyEdge::Press, HotkeyEdge::Release] {
            assert_eq!(
                binding_instance("Ctrl+F1".to_owned(), edge).cooldown_secs,
                0,
                "a {edge:?} binding must not be rate-limited"
            );
        }
    }
}
