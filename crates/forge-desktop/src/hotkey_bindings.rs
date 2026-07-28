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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_hotkey::HotkeyError;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ExecutionMode, QueueId};
    use gpui::Modifiers;

    use super::*;

    const TEST_KEY: [u8; 32] = [0x11; 32];
    const OTHER_KIND: &str = "midi.note_on";

    async fn provider() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
                .await
                .expect("in-memory backend opens"),
        )
    }

    async fn default_queue(backend: &Arc<dyn DataProvider>) -> QueueId {
        backend
            .queue_repo()
            .get_by_name("Default")
            .await
            .unwrap()
            .expect("migrations seed the default queue")
            .id
    }

    async fn seed_action(backend: &Arc<dyn DataProvider>, name: &str) -> ActionId {
        let action = Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id: default_queue(backend).await,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };
        let id = action.id;
        backend.action_repo().save(&action).await.unwrap();
        id
    }

    async fn seed_instance(
        backend: &Arc<dyn DataProvider>,
        kind_id: &str,
        combo: Option<&str>,
    ) -> TriggerInstanceId {
        let mut overrides = BTreeMap::new();
        if let Some(combo) = combo {
            overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()));
        }
        let instance = TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: combo.unwrap_or("no combo").to_owned(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: true,
        };
        let id = instance.id;
        backend
            .trigger_instance_repo()
            .save(&instance)
            .await
            .unwrap();
        id
    }

    fn keystroke(modifiers: Modifiers, key: &str) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    #[test]
    fn keystroke_to_combo_maps_each_gpui_modifier_flag_to_its_own_combo_token() {
        let cases = [
            (Modifiers::default(), "A"),
            (
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                "Ctrl+A",
            ),
            (
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                "Shift+A",
            ),
            (
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
                "Alt+A",
            ),
            (
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
                "Meta+A",
            ),
            (
                Modifiers {
                    function: true,
                    ..Default::default()
                },
                "A",
            ),
            (
                Modifiers {
                    control: true,
                    shift: true,
                    alt: true,
                    platform: true,
                    function: false,
                },
                "Ctrl+Shift+Alt+Meta+A",
            ),
        ];

        for (modifiers, expected) in cases {
            assert_eq!(
                keystroke_to_combo(&keystroke(modifiers, "a")).as_deref(),
                Some(expected),
                "wrong combo for {modifiers:?}"
            );
        }
    }

    #[test]
    fn keystroke_to_combo_rejects_keystrokes_the_combo_grammar_cannot_express() {
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        let cases = [
            (Modifiers::default(), ""),
            (ctrl, ""),
            (ctrl, "f13"),
            (ctrl, ";"),
            (Modifiers::default(), "shift"),
            (ctrl, "shift"),
        ];

        for (modifiers, key) in cases {
            assert_eq!(
                keystroke_to_combo(&keystroke(modifiers, key)),
                None,
                "expected no combo for key {key:?} with {modifiers:?}"
            );
        }
    }

    #[test]
    fn combo_keys_yields_one_cap_per_token_and_never_an_empty_cap() {
        let cases: [(&str, Vec<&str>); 4] = [
            ("Ctrl+Shift+F5", vec!["Ctrl", "Shift", "F5"]),
            ("A", vec!["A"]),
            ("Ctrl+", vec!["Ctrl"]),
            ("", vec![]),
        ];

        for (combo, expected) in cases {
            assert_eq!(combo_keys(combo), expected, "wrong caps for {combo:?}");
        }
    }

    #[test]
    fn is_already_registered_matches_the_conflict_error_and_no_other_hotkey_error() {
        let conflict = HotkeyError::AlreadyRegistered {
            combo: "Ctrl+F1".to_owned(),
        };
        assert!(is_already_registered(&conflict.to_string()));

        for other in [
            HotkeyError::InvalidCombo("Ctrl+".to_owned()),
            HotkeyError::PortalUnavailable {
                reason: "no session".to_owned(),
            },
            HotkeyError::PermissionDenied,
            HotkeyError::Backend("device busy".to_owned()),
            HotkeyError::SupervisorUnavailable,
        ] {
            assert!(
                !is_already_registered(&other.to_string()),
                "misread {other:?} as a combo conflict"
            );
        }
    }

    #[tokio::test]
    async fn load_bindings_keeps_only_hotkey_instances_that_carry_a_combo_override() {
        let backend = provider().await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, None).await;
        seed_instance(&backend, OTHER_KIND, Some("Ctrl+F2")).await;

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        let combos: Vec<&str> = rows.iter().map(|row| row.combo.as_str()).collect();
        assert_eq!(combos, ["Ctrl+F1"]);
    }

    #[tokio::test]
    async fn load_bindings_sorts_rows_by_combo() {
        let backend = provider().await;
        for combo in ["Meta+B", "Alt+C", "Ctrl+A"] {
            seed_instance(&backend, HOTKEY_PRESSED_KIND, Some(combo)).await;
        }

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        let combos: Vec<&str> = rows.iter().map(|row| row.combo.as_str()).collect();
        assert_eq!(combos, ["Alt+C", "Ctrl+A", "Meta+B"]);
    }

    #[tokio::test]
    async fn load_bindings_marks_a_row_registered_only_when_the_client_holds_its_combo() {
        let backend = provider().await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F2")).await;

        let rows = load_bindings(
            Arc::clone(&backend),
            vec![(HotkeyId(1), "Ctrl+F1".to_owned())],
        )
        .await
        .unwrap();

        let flags: Vec<(&str, bool)> = rows
            .iter()
            .map(|row| (row.combo.as_str(), row.registered))
            .collect();
        assert_eq!(flags, [("Ctrl+F1", true), ("Ctrl+F2", false)]);
    }

    #[tokio::test]
    async fn load_bindings_resolves_the_linked_action_and_leaves_unlinked_rows_bare() {
        let backend = provider().await;
        let action = seed_action(&backend, "Play sound").await;
        let bound = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F2")).await;
        backend
            .trigger_instance_repo()
            .link_action(action, bound, 0)
            .await
            .unwrap();

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        assert_eq!(rows[0].action, Some((action, "Play sound".to_owned())));
        assert_eq!(rows[1].action, None);
    }

    #[tokio::test]
    async fn cleanup_stale_combo_instances_unlinks_the_action_before_deleting_the_instance() {
        let backend = provider().await;
        let action = seed_action(&backend, "Play sound").await;
        let instance = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        backend
            .trigger_instance_repo()
            .link_action(action, instance, 0)
            .await
            .unwrap();

        cleanup_stale_combo_instances(&backend, "Ctrl+F1")
            .await
            .unwrap();

        assert!(
            backend
                .trigger_instance_repo()
                .get(instance)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn cleanup_stale_combo_instances_spares_other_combos_and_other_trigger_kinds() {
        let backend = provider().await;
        let other_combo = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F2")).await;
        let other_kind = seed_instance(&backend, OTHER_KIND, Some("Ctrl+F1")).await;

        cleanup_stale_combo_instances(&backend, "Ctrl+F1")
            .await
            .unwrap();

        let repo = backend.trigger_instance_repo();
        assert!(repo.get(other_combo).await.unwrap().is_some());
        assert!(repo.get(other_kind).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn relink_action_moves_the_binding_from_the_previous_action_to_the_new_one() {
        let backend = provider().await;
        let old_action = seed_action(&backend, "Old").await;
        let new_action = seed_action(&backend, "New").await;
        let instance = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let repo = backend.trigger_instance_repo();
        repo.link_action(old_action, instance, 0).await.unwrap();

        relink_action(Arc::clone(&backend), instance, new_action)
            .await
            .unwrap();

        assert_eq!(repo.actions_using(instance).await.unwrap(), [new_action]);
    }

    #[tokio::test]
    async fn relink_action_leaves_the_existing_link_in_place_when_the_action_is_unchanged() {
        let backend = provider().await;
        let action = seed_action(&backend, "Play sound").await;
        let instance = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let repo = backend.trigger_instance_repo();
        repo.link_action(action, instance, 0).await.unwrap();
        let mut expected = vec![instance];
        for (offset, combo) in ["Ctrl+F2", "Ctrl+F3"].into_iter().enumerate() {
            let sibling = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some(combo)).await;
            repo.link_action(action, sibling, offset as i64 + 1)
                .await
                .unwrap();
            expected.push(sibling);
        }

        relink_action(Arc::clone(&backend), instance, action)
            .await
            .unwrap();

        let order: Vec<TriggerInstanceId> = repo
            .list_for_action(action)
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(
            order, expected,
            "a no-op relink must not unlink and re-append the binding"
        );
    }

    #[tokio::test]
    async fn relink_action_appends_the_binding_after_the_actions_existing_triggers() {
        let backend = provider().await;
        let action = seed_action(&backend, "Play sound").await;
        let repo = backend.trigger_instance_repo();
        for (position, combo) in ["Ctrl+F2", "Ctrl+F3"].into_iter().enumerate() {
            let existing = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some(combo)).await;
            repo.link_action(action, existing, position as i64)
                .await
                .unwrap();
        }
        let instance = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;

        relink_action(Arc::clone(&backend), instance, action)
            .await
            .unwrap();

        let last = repo
            .list_for_action(action)
            .await
            .unwrap()
            .pop()
            .map(|i| i.id);
        assert_eq!(last, Some(instance));
    }
}
