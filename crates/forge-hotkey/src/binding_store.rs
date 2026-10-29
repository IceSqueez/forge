use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_types::{
    ActionId, PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId, Variant,
};

use crate::backend::HotkeyId;
use crate::binding::{BindingHalf, BindingRow, COMBO_FIELD, HotkeyEdge};
use crate::combo::HotkeyCombo;
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
) -> Result<Vec<BindingRow>, String> {
    let triggers = backend.trigger_instance_repo();
    let actions = backend.action_repo();
    let instances = triggers.list_all().await.map_err(|e| e.to_string())?;

    let mut grouped: BTreeMap<String, (Option<BindingHalf>, Option<BindingHalf>)> = BTreeMap::new();
    for instance in instances {
        let Some(edge) = HotkeyEdge::from_kind(&instance.kind_id) else {
            continue;
        };
        let Some(combo) = canonical_combo_of(&instance) else {
            continue;
        };
        let linked = triggers
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        let mut action = None;
        if let Some(first) = linked.first()
            && let Some(found) = actions.get(*first).await.map_err(|e| e.to_string())?
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
) -> Result<Vec<TriggerInstance>, String> {
    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;
    Ok(instances
        .into_iter()
        .filter(|instance| HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter(|instance| {
            canonical_combo_of(instance).is_some_and(|existing| existing == combo_str)
        })
        .collect())
}

async fn drop_instance(
    backend: &Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
) -> Result<(), String> {
    let repo = backend.trigger_instance_repo();
    let action_ids = repo
        .actions_using(instance_id)
        .await
        .map_err(|e| e.to_string())?;
    for aid in action_ids {
        repo.unlink_action(aid, instance_id)
            .await
            .map_err(|e| e.to_string())?;
    }
    repo.delete(instance_id)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn cleanup_stale_combo_instances(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<(), String> {
    for instance in hotkey_instances_for_combo(backend, combo_str).await? {
        drop_instance(backend, instance.id).await?;
    }
    Ok(())
}

async fn remove_combo_edge(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
    edge: HotkeyEdge,
) -> Result<(), String> {
    for instance in hotkey_instances_for_combo(backend, combo_str).await? {
        if instance.kind_id == edge.kind_id() {
            drop_instance(backend, instance.id).await?;
        }
    }
    Ok(())
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

pub fn persisted_hotkey_combos(instances: &[TriggerInstance]) -> BTreeSet<String> {
    instances
        .iter()
        .filter(|instance| HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter_map(canonical_combo_of)
        .collect()
}

pub async fn do_bind(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> Result<(), String> {
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    reconciler.claim(combo).await.map_err(|e| e.to_string())?;
    let stored = store_binding(&backend, combo_str, edge, action_id).await;
    reconciler.reconcile().await;
    stored
}

async fn store_binding(
    backend: &Arc<dyn DataProvider>,
    combo_str: String,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> Result<(), String> {
    remove_combo_edge(backend, &combo_str, edge).await?;

    let repo = backend.trigger_instance_repo();
    let instance = binding_instance(combo_str, edge);
    repo.save(&instance).await.map_err(|e| e.to_string())?;
    repo.link_action(action_id, instance.id)
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_binding(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
) -> Result<(), String> {
    let cleaned = cleanup_stale_combo_instances(&backend, &combo_str).await;
    reconciler.reconcile().await;
    cleaned
}

pub async fn delete_binding_half(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
) -> Result<(), String> {
    drop_instance(&backend, instance_id).await
}

pub async fn set_binding_enabled(
    backend: Arc<dyn DataProvider>,
    instance_ids: Vec<TriggerInstanceId>,
    enabled: bool,
) -> Result<(), String> {
    let repo = backend.trigger_instance_repo();
    for instance_id in instance_ids {
        repo.set_enabled(instance_id, enabled)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn rebind_combo(
    reconciler: Arc<HotkeyReconciler>,
    backend: Arc<dyn DataProvider>,
    previous_combo: String,
    combo_str: String,
) -> Result<(), String> {
    if previous_combo == combo_str {
        return Ok(());
    }
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    reconciler.claim(combo).await.map_err(|e| e.to_string())?;
    let moved = move_combo_instances(&backend, &previous_combo, &combo_str).await;
    reconciler.reconcile().await;
    moved
}

async fn move_combo_instances(
    backend: &Arc<dyn DataProvider>,
    previous_combo: &str,
    combo_str: &str,
) -> Result<(), String> {
    cleanup_stale_combo_instances(backend, combo_str).await?;

    let repo = backend.trigger_instance_repo();
    for source in hotkey_instances_for_combo(backend, previous_combo).await? {
        let mut overrides = source.overrides.clone();
        overrides.insert(
            COMBO_FIELD.to_owned(),
            Variant::String(combo_str.to_owned()),
        );
        let updated = TriggerInstance {
            name: combo_str.to_owned(),
            overrides,
            ..source
        };
        repo.save(&updated).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub async fn set_binding_edge(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
    edge: HotkeyEdge,
) -> Result<(), String> {
    let repo = backend.trigger_instance_repo();
    let source = repo
        .get(instance_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "trigger instance not found".to_owned())?;
    if source.kind_id == edge.kind_id() {
        return Ok(());
    }
    let updated = TriggerInstance {
        kind_id: edge.kind_id().to_owned(),
        ..source
    };
    repo.save(&updated).await.map_err(|e| e.to_string())
}

pub async fn relink_action(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
    action_id: ActionId,
) -> Result<(), String> {
    let repo = backend.trigger_instance_repo();
    let linked = repo
        .actions_using(instance_id)
        .await
        .map_err(|e| e.to_string())?;
    if linked.contains(&action_id) {
        return Ok(());
    }
    for previous in linked {
        repo.unlink_action(previous, instance_id)
            .await
            .map_err(|e| e.to_string())?;
    }
    repo.link_action(action_id, instance_id)
        .await
        .map_err(|e| e.to_string())
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
