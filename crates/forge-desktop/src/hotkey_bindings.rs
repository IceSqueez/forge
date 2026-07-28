use std::collections::BTreeMap;
use std::sync::Arc;

use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyId};
use forge_platform_core::{BuiltinHealth, HealthValue};
use forge_storage::DataProvider;
use forge_types::{ActionId, PlatformScope, TriggerInstance, TriggerInstanceId, Variant};
use gpui::Keystroke;

pub const HOTKEY_ENABLED_KEY: &str = "hotkey.enabled";
pub const HOTKEY_PRESSED_KIND: &str = "hotkey.global.pressed";
pub const HOTKEY_EVENT_PREFIX: &str = "hotkey.";

const COMBO_FIELD: &str = "combo";
const CONFLICTS_METRIC: &str = "CONFLICTS";

pub struct BindingRow {
    pub instance_id: TriggerInstanceId,
    pub combo: String,
    pub enabled: bool,
    pub registered: bool,
    pub action: Option<(ActionId, String)>,
}

pub fn combo_keys(combo: &str) -> Vec<&str> {
    combo.split('+').filter(|part| !part.is_empty()).collect()
}

pub fn registered_combos(client: &HotkeyClient) -> Vec<(HotkeyId, String)> {
    client
        .registered_combos()
        .into_iter()
        .map(|(id, combo)| (id, combo.as_str().to_owned()))
        .collect()
}

/// Reads the count off the builtin health surface; `HotkeyClient` keeps no other public window onto it.
pub fn conflict_count(client: &HotkeyClient) -> usize {
    client
        .metrics()
        .into_iter()
        .find(|metric| metric.label == CONFLICTS_METRIC)
        .and_then(|metric| match metric.value {
            HealthValue::Text { primary, .. } => primary.parse::<usize>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}

pub fn keystroke_to_combo(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if key.is_empty() {
        return None;
    }
    let modifiers = &keystroke.modifiers;
    let mut parts: Vec<&str> = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.platform {
        parts.push("Meta");
    }
    let raw = if parts.is_empty() {
        key.to_owned()
    } else {
        format!("{}+{key}", parts.join("+"))
    };
    HotkeyCombo::parse(&raw).ok().map(|c| c.as_str().to_owned())
}

pub fn is_already_registered(err: &str) -> bool {
    err.contains("already registered")
}

fn combo_of(instance: &TriggerInstance) -> Option<&String> {
    match instance.overrides.get(COMBO_FIELD) {
        Some(Variant::String(combo)) => Some(combo),
        _ => None,
    }
}

pub async fn load_bindings(
    backend: Arc<dyn DataProvider>,
    registered: Vec<(HotkeyId, String)>,
) -> Result<Vec<BindingRow>, String> {
    let triggers = backend.trigger_instance_repo();
    let actions = backend.action_repo();
    let instances = triggers.list_all().await.map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    for instance in instances {
        if instance.kind_id != HOTKEY_PRESSED_KIND {
            continue;
        }
        let Some(combo) = combo_of(&instance).cloned() else {
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
        rows.push(BindingRow {
            instance_id: instance.id,
            registered: registered.iter().any(|(_, known)| known == &combo),
            combo,
            enabled: instance.enabled,
            action,
        });
    }
    rows.sort_by(|a, b| a.combo.cmp(&b.combo));
    Ok(rows)
}

pub async fn cleanup_stale_combo_instances(
    backend: &Arc<dyn DataProvider>,
    combo_str: &str,
) -> Result<(), String> {
    let instances = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .map_err(|e| e.to_string())?;

    for instance in instances {
        if instance.kind_id != HOTKEY_PRESSED_KIND {
            continue;
        }
        if combo_of(&instance).is_none_or(|existing| existing != combo_str) {
            continue;
        }
        let action_ids = backend
            .trigger_instance_repo()
            .actions_using(instance.id)
            .await
            .map_err(|e| e.to_string())?;
        for aid in action_ids {
            backend
                .trigger_instance_repo()
                .unlink_action(aid, instance.id)
                .await
                .map_err(|e| e.to_string())?;
        }
        backend
            .trigger_instance_repo()
            .delete(instance.id)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub async fn do_bind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    action_id: ActionId,
) -> Result<(), String> {
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    client.register(combo).await.map_err(|e| e.to_string())?;

    cleanup_stale_combo_instances(&backend, &combo_str).await?;

    let mut overrides = BTreeMap::new();
    overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo_str.clone()));
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: HOTKEY_PRESSED_KIND.to_owned(),
        name: combo_str,
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
    };
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .map_err(|e| e.to_string())?;
    backend
        .trigger_instance_repo()
        .link_action(action_id, instance.id, 0)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn do_unbind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    hotkey_id: HotkeyId,
) -> Result<(), String> {
    let combo = client
        .registered_combos()
        .into_iter()
        .find(|(id, _)| *id == hotkey_id)
        .map(|(_, combo)| combo.as_str().to_owned());

    client
        .unregister(hotkey_id)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(combo) = combo {
        cleanup_stale_combo_instances(&backend, &combo).await?;
    }

    Ok(())
}

pub async fn do_replace(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    existing_id: HotkeyId,
    combo_str: String,
    action_id: ActionId,
) -> Result<(), String> {
    client
        .unregister(existing_id)
        .await
        .map_err(|e| e.to_string())?;
    do_bind(client, backend, combo_str, action_id).await
}

pub async fn delete_binding(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
) -> Result<(), String> {
    if let Some((id, _)) = client
        .registered_combos()
        .into_iter()
        .find(|(_, combo)| combo.as_str() == combo_str)
    {
        client.unregister(id).await.map_err(|e| e.to_string())?;
    }
    cleanup_stale_combo_instances(&backend, &combo_str).await
}

/// Leaves the OS registration in place: the combo still fires `hotkey.global.pressed`, and the trigger evaluator is what skips a disabled instance.
pub async fn set_binding_enabled(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
    enabled: bool,
) -> Result<(), String> {
    backend
        .trigger_instance_repo()
        .set_enabled(instance_id, enabled)
        .await
        .map_err(|e| e.to_string())
}

pub async fn rebind_combo(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
    previous_combo: String,
    combo_str: String,
) -> Result<(), String> {
    if previous_combo == combo_str {
        return Ok(());
    }
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    if let Some((id, _)) = client
        .registered_combos()
        .into_iter()
        .find(|(_, known)| known.as_str() == previous_combo)
    {
        client.unregister(id).await.map_err(|e| e.to_string())?;
    }
    client.register(combo).await.map_err(|e| e.to_string())?;

    cleanup_stale_combo_instances(&backend, &combo_str).await?;

    let repo = backend.trigger_instance_repo();
    let source = repo
        .get(instance_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "trigger instance not found".to_owned())?;
    let mut overrides = source.overrides.clone();
    overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo_str.clone()));
    let updated = TriggerInstance {
        name: combo_str,
        overrides,
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
    let position = repo
        .list_for_action(action_id)
        .await
        .map_err(|e| e.to_string())?
        .len() as i64;
    repo.link_action(action_id, instance_id, position)
        .await
        .map_err(|e| e.to_string())
}
