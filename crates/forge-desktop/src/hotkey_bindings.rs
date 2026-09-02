use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use forge_hotkey::{DEFAULT_HOLD_CEILING_SECS, HotkeyClient, HotkeyCombo, HotkeyId};
use forge_platform_core::{BuiltinHealth, HealthValue};
use forge_storage::{DataProvider, SettingsRepo, StorageError};
use forge_types::{
    ActionId, PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId, Variant,
};
use gpui::Keystroke;

pub const HOTKEY_ENABLED_KEY: &str = "hotkey.enabled";
pub const HOTKEY_HOLD_CEILING_KEY: &str = "hotkey.hold_ceiling_secs";
pub const HOTKEY_PRESSED_KIND: &str = "hotkey.global.pressed";
pub const HOTKEY_RELEASED_KIND: &str = "hotkey.global.released";
pub const HOTKEY_EVENT_PREFIX: &str = "hotkey.";

const COMBO_FIELD: &str = "combo";
const CONFLICTS_METRIC: &str = "CONFLICTS";
const CEILING_OFF: u64 = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HotkeyEdge {
    Press,
    Release,
}

impl HotkeyEdge {
    pub fn kind_id(self) -> &'static str {
        match self {
            HotkeyEdge::Press => HOTKEY_PRESSED_KIND,
            HotkeyEdge::Release => HOTKEY_RELEASED_KIND,
        }
    }

    pub fn from_kind(kind_id: &str) -> Option<Self> {
        match kind_id {
            HOTKEY_PRESSED_KIND => Some(HotkeyEdge::Press),
            HOTKEY_RELEASED_KIND => Some(HotkeyEdge::Release),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            HotkeyEdge::Press => HotkeyEdge::Release,
            HotkeyEdge::Release => HotkeyEdge::Press,
        }
    }
}

pub struct BindingHalf {
    pub instance_id: TriggerInstanceId,
    pub enabled: bool,
    pub action: Option<(ActionId, String)>,
}

pub struct BindingRow {
    /// Row identity for menus and capture targets: the press half when the row has one, else the release half.
    pub key: TriggerInstanceId,
    pub combo: String,
    pub registered: bool,
    pub press: Option<BindingHalf>,
    pub release: Option<BindingHalf>,
}

impl BindingRow {
    pub fn half(&self, edge: HotkeyEdge) -> Option<&BindingHalf> {
        match edge {
            HotkeyEdge::Press => self.press.as_ref(),
            HotkeyEdge::Release => self.release.as_ref(),
        }
    }

    pub fn is_hold(&self) -> bool {
        self.press.is_some() && self.release.is_some()
    }

    pub fn is_release_edge(&self) -> bool {
        self.press.is_none() && self.release.is_some()
    }

    /// Off unless every half the row carries is enabled, so a half-disabled hold reads as stopped.
    pub fn enabled(&self) -> bool {
        self.halves().all(|(_, half)| half.enabled)
    }

    pub fn halves(&self) -> impl Iterator<Item = (HotkeyEdge, &BindingHalf)> {
        [
            (HotkeyEdge::Press, self.press.as_ref()),
            (HotkeyEdge::Release, self.release.as_ref()),
        ]
        .into_iter()
        .filter_map(|(edge, half)| half.map(|half| (edge, half)))
    }

    pub fn primary_action(&self) -> Option<&(ActionId, String)> {
        self.press
            .as_ref()
            .or(self.release.as_ref())
            .and_then(|half| half.action.as_ref())
    }

    /// The edge with no half bound yet; `None` once the combo carries both.
    pub fn free_edge(&self) -> Option<HotkeyEdge> {
        match (self.press.is_some(), self.release.is_some()) {
            (true, false) => Some(HotkeyEdge::Release),
            (false, true) => Some(HotkeyEdge::Press),
            _ => None,
        }
    }
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

    let mut grouped: BTreeMap<String, (Option<BindingHalf>, Option<BindingHalf>)> = BTreeMap::new();
    for instance in instances {
        let Some(edge) = HotkeyEdge::from_kind(&instance.kind_id) else {
            continue;
        };
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
        .filter(|instance| combo_of(instance).is_some_and(|existing| existing == combo_str))
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

/// Prunes both edges of the combo, so removing a binding never orphans its partner half.
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

/// A second half re-uses the combo's existing registration; registering it twice is an error.
async fn ensure_registered(client: &HotkeyClient, combo: HotkeyCombo) -> Result<(), String> {
    if client
        .registered_combos()
        .iter()
        .any(|(_, known)| known.as_str() == combo.as_str())
    {
        return Ok(());
    }
    client
        .register(combo)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
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
        // A cooldown on the release half would swallow the stop of a hold.
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

/// One entry per combo across both edges: a hold's halves share a single OS registration.
pub fn persisted_hotkey_combos(instances: &[TriggerInstance]) -> BTreeSet<String> {
    instances
        .iter()
        .filter(|instance| HotkeyEdge::from_kind(&instance.kind_id).is_some())
        .filter_map(combo_of)
        .cloned()
        .collect()
}

pub async fn do_bind(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
    combo_str: String,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> Result<(), String> {
    let combo = HotkeyCombo::parse(&combo_str).map_err(|e| e.to_string())?;
    ensure_registered(&client, combo).await?;

    remove_combo_edge(&backend, &combo_str, edge).await?;

    let instance = binding_instance(combo_str, edge);
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

/// Keeps the OS registration, which the surviving partner half still needs.
pub async fn delete_binding_half(
    backend: Arc<dyn DataProvider>,
    instance_id: TriggerInstanceId,
) -> Result<(), String> {
    drop_instance(&backend, instance_id).await
}

/// Leaves the OS registration in place: the combo still fires, and the trigger evaluator is what skips a disabled instance.
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

/// Moves every half of the combo, so rebinding a hold keeps its press and release on one combo.
pub async fn rebind_combo(
    client: Arc<HotkeyClient>,
    backend: Arc<dyn DataProvider>,
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
    ensure_registered(&client, combo).await?;

    cleanup_stale_combo_instances(&backend, &combo_str).await?;

    let repo = backend.trigger_instance_repo();
    for source in hotkey_instances_for_combo(&backend, &previous_combo).await? {
        let mut overrides = source.overrides.clone();
        overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo_str.clone()));
        let updated = TriggerInstance {
            name: combo_str.clone(),
            overrides,
            ..source
        };
        repo.save(&updated).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Callers must free the target edge first; two instances of one edge on a combo hide one another.
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

/// `None` is the off state: no ceiling closes a hold the OS never reported releasing.
pub async fn load_hold_ceiling(repo: &dyn SettingsRepo) -> Option<u64> {
    match repo.get_string(HOTKEY_HOLD_CEILING_KEY).await {
        Ok(Some(raw)) => match raw.trim().parse::<u64>() {
            Ok(CEILING_OFF) => None,
            Ok(secs) => Some(secs),
            Err(_) => Some(DEFAULT_HOLD_CEILING_SECS),
        },
        Ok(None) => Some(DEFAULT_HOLD_CEILING_SECS),
        Err(e) => {
            tracing::warn!(error = %e, "failed to read the hotkey hold ceiling");
            Some(DEFAULT_HOLD_CEILING_SECS)
        }
    }
}

pub async fn save_hold_ceiling(
    repo: &dyn SettingsRepo,
    secs: Option<u64>,
) -> Result<(), StorageError> {
    repo.set_string(
        HOTKEY_HOLD_CEILING_KEY,
        &secs.unwrap_or(CEILING_OFF).to_string(),
    )
    .await
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

    fn instance_literal(kind_id: &str, combo: Option<&str>) -> TriggerInstance {
        let mut overrides = BTreeMap::new();
        if let Some(combo) = combo {
            overrides.insert(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()));
        }
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: combo.unwrap_or("no combo").to_owned(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    async fn seed_instance(
        backend: &Arc<dyn DataProvider>,
        kind_id: &str,
        combo: Option<&str>,
    ) -> TriggerInstanceId {
        let instance = instance_literal(kind_id, combo);
        let id = instance.id;
        backend
            .trigger_instance_repo()
            .save(&instance)
            .await
            .unwrap();
        id
    }

    async fn seed_disabled_instance(
        backend: &Arc<dyn DataProvider>,
        kind_id: &str,
        combo: &str,
    ) -> TriggerInstanceId {
        let id = seed_instance(backend, kind_id, Some(combo)).await;
        backend
            .trigger_instance_repo()
            .set_enabled(id, false)
            .await
            .unwrap();
        id
    }

    fn settings_of(backend: &Arc<dyn DataProvider>) -> Arc<dyn SettingsRepo> {
        Arc::clone(backend) as Arc<dyn SettingsRepo>
    }

    fn keystroke(modifiers: Modifiers, key: &str) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    fn test_half(action: Option<(ActionId, &str)>) -> BindingHalf {
        BindingHalf {
            instance_id: TriggerInstanceId::new(),
            enabled: true,
            action: action.map(|(id, name)| (id, name.to_owned())),
        }
    }

    fn test_row(press: Option<BindingHalf>, release: Option<BindingHalf>) -> BindingRow {
        let key = press
            .as_ref()
            .or(release.as_ref())
            .map(|half| half.instance_id)
            .expect("a row always carries at least one half");
        BindingRow {
            key,
            combo: "Ctrl+F1".to_owned(),
            registered: true,
            press,
            release,
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

    #[tokio::test]
    async fn load_bindings_keeps_both_hotkey_edges_and_drops_everything_else() {
        let backend = provider().await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F2")).await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, None).await;
        seed_instance(&backend, OTHER_KIND, Some("Ctrl+F3")).await;

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        let combos: Vec<&str> = rows.iter().map(|row| row.combo.as_str()).collect();
        assert_eq!(combos, ["Ctrl+F1", "Ctrl+F2"]);
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

        assert_eq!(
            rows[0].primary_action(),
            Some(&(action, "Play sound".to_owned()))
        );
        assert_eq!(rows[1].primary_action(), None);
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

    #[test]
    fn binding_row_shape_drives_the_badge_and_the_free_edge() {
        let cases = [
            (
                "press only",
                test_row(Some(test_half(None)), None),
                (false, false, Some(HotkeyEdge::Release)),
            ),
            (
                "release only",
                test_row(None, Some(test_half(None))),
                (false, true, Some(HotkeyEdge::Press)),
            ),
            (
                "both halves",
                test_row(Some(test_half(None)), Some(test_half(None))),
                (true, false, None),
            ),
        ];

        for (case, row, expected) in cases {
            assert_eq!(
                (row.is_hold(), row.is_release_edge(), row.free_edge()),
                expected,
                "wrong (hold, release-edge, free-edge) for a {case} row"
            );
        }
    }

    #[test]
    fn binding_row_primary_action_reads_the_press_half_whenever_the_row_has_one() {
        let start = ActionId::new();
        let stop = ActionId::new();
        let cases = [
            (
                "a hold reports the press half's action",
                test_row(
                    Some(test_half(Some((start, "Start")))),
                    Some(test_half(Some((stop, "Stop")))),
                ),
                Some((start, "Start".to_owned())),
            ),
            (
                "a release-only row reports its own action",
                test_row(None, Some(test_half(Some((stop, "Stop"))))),
                Some((stop, "Stop".to_owned())),
            ),
            (
                // Why: the press half is the row's target even when it is unlinked, so a hold
                // whose start was unbound from the Triggers screen reads as unassigned rather
                // than silently advertising its stop action as the thing the combo runs.
                "an unlinked press half is not backfilled from the release half",
                test_row(Some(test_half(None)), Some(test_half(Some((stop, "Stop"))))),
                None,
            ),
        ];

        for (case, row, expected) in cases {
            assert_eq!(row.primary_action().cloned(), expected, "wrong when {case}");
        }
    }

    #[tokio::test]
    async fn load_bindings_yields_one_row_per_combo_keyed_by_the_press_half_when_there_is_one() {
        let backend = provider().await;
        let hold_press = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let hold_release = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F1")).await;
        let press_only = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F2")).await;
        let release_only = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F3")).await;

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        let shape: Vec<(&str, TriggerInstanceId, bool)> = rows
            .iter()
            .map(|row| (row.combo.as_str(), row.key, row.is_hold()))
            .collect();
        assert_eq!(
            shape,
            [
                ("Ctrl+F1", hold_press, true),
                ("Ctrl+F2", press_only, false),
                ("Ctrl+F3", release_only, false),
            ]
        );
        assert_eq!(
            rows[0].half(HotkeyEdge::Release).map(|h| h.instance_id),
            Some(hold_release),
            "the release half must stay reachable on the grouped row"
        );
    }

    #[tokio::test]
    async fn load_bindings_reads_a_hold_with_a_disabled_release_half_as_off() {
        let backend = provider().await;
        seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        seed_disabled_instance(&backend, HOTKEY_RELEASED_KIND, "Ctrl+F1").await;

        let rows = load_bindings(Arc::clone(&backend), Vec::new())
            .await
            .unwrap();

        assert!(
            !rows[0].enabled(),
            "a hold that starts but cannot stop must not read as on"
        );
    }

    #[tokio::test]
    async fn cleanup_stale_combo_instances_prunes_both_edges_of_the_combo() {
        let backend = provider().await;
        let press = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let release = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F1")).await;

        cleanup_stale_combo_instances(&backend, "Ctrl+F1")
            .await
            .unwrap();

        let repo = backend.trigger_instance_repo();
        assert!(repo.get(press).await.unwrap().is_none());
        assert!(
            repo.get(release).await.unwrap().is_none(),
            "the release half must not be orphaned by a combo cleanup"
        );
    }

    #[tokio::test]
    async fn remove_combo_edge_drops_only_the_named_edge() {
        let backend = provider().await;
        let press = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let release = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F1")).await;

        remove_combo_edge(&backend, "Ctrl+F1", HotkeyEdge::Press)
            .await
            .unwrap();

        let repo = backend.trigger_instance_repo();
        assert!(repo.get(press).await.unwrap().is_none());
        assert!(repo.get(release).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn delete_binding_half_leaves_the_partner_half_and_its_action_link() {
        let backend = provider().await;
        let start = seed_action(&backend, "Start").await;
        let stop = seed_action(&backend, "Stop").await;
        let press = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let release = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F1")).await;
        let repo = backend.trigger_instance_repo();
        repo.link_action(start, press, 0).await.unwrap();
        repo.link_action(stop, release, 0).await.unwrap();

        delete_binding_half(Arc::clone(&backend), press)
            .await
            .unwrap();

        assert!(repo.get(press).await.unwrap().is_none());
        assert_eq!(repo.actions_using(release).await.unwrap(), [stop]);
    }

    #[tokio::test]
    async fn set_binding_enabled_applies_to_every_instance_it_is_given() {
        let backend = provider().await;
        let press = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;
        let release = seed_instance(&backend, HOTKEY_RELEASED_KIND, Some("Ctrl+F1")).await;

        set_binding_enabled(Arc::clone(&backend), vec![press, release], false)
            .await
            .unwrap();

        let repo = backend.trigger_instance_repo();
        let states: Vec<bool> = vec![
            repo.get(press).await.unwrap().unwrap().enabled,
            repo.get(release).await.unwrap().unwrap().enabled,
        ];
        assert_eq!(states, [false, false]);
    }

    #[tokio::test]
    async fn set_binding_edge_rewrites_the_kind_and_keeps_the_combo_override() {
        let backend = provider().await;
        let instance = seed_instance(&backend, HOTKEY_PRESSED_KIND, Some("Ctrl+F1")).await;

        set_binding_edge(Arc::clone(&backend), instance, HotkeyEdge::Release)
            .await
            .unwrap();

        let stored = backend
            .trigger_instance_repo()
            .get(instance)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.kind_id, HOTKEY_RELEASED_KIND);
        assert_eq!(
            combo_of(&stored).map(String::as_str),
            Some("Ctrl+F1"),
            "moving an edge must not lose the combo the instance matches on"
        );
    }

    #[tokio::test]
    async fn set_binding_edge_reports_an_instance_that_no_longer_exists() {
        let backend = provider().await;

        let result = set_binding_edge(
            Arc::clone(&backend),
            TriggerInstanceId::new(),
            HotkeyEdge::Release,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_hold_ceiling_treats_zero_as_off_and_falls_back_to_the_default() {
        let backend = provider().await;
        let settings = settings_of(&backend);

        assert_eq!(
            load_hold_ceiling(settings.as_ref()).await,
            Some(DEFAULT_HOLD_CEILING_SECS),
            "a fresh install runs with the ceiling on"
        );

        let cases = [
            ("0", None),
            ("1", Some(1)),
            ("45", Some(45)),
            (" 45 ", Some(45)),
            ("3600", Some(3600)),
            ("", Some(DEFAULT_HOLD_CEILING_SECS)),
            ("-1", Some(DEFAULT_HOLD_CEILING_SECS)),
            ("30s", Some(DEFAULT_HOLD_CEILING_SECS)),
        ];
        for (stored, expected) in cases {
            settings
                .set_string(HOTKEY_HOLD_CEILING_KEY, stored)
                .await
                .unwrap();
            assert_eq!(
                load_hold_ceiling(settings.as_ref()).await,
                expected,
                "wrong ceiling for stored {stored:?}"
            );
        }
    }

    #[tokio::test]
    async fn save_hold_ceiling_round_trips_the_off_state_through_a_stored_zero() {
        let backend = provider().await;
        let settings = settings_of(&backend);

        save_hold_ceiling(settings.as_ref(), None).await.unwrap();
        assert_eq!(load_hold_ceiling(settings.as_ref()).await, None);

        save_hold_ceiling(settings.as_ref(), Some(45))
            .await
            .unwrap();
        assert_eq!(load_hold_ceiling(settings.as_ref()).await, Some(45));
    }

    #[test]
    fn binding_instance_leaves_both_edges_without_a_cooldown() {
        // Why: a cooldown on the release half silently swallows the stop of a hold, so the
        // combo starts and never stops. The bind path must never seed a non-zero default.
        for edge in [HotkeyEdge::Press, HotkeyEdge::Release] {
            assert_eq!(
                binding_instance("Ctrl+F1".to_owned(), edge).cooldown_secs,
                0,
                "a {edge:?} binding must not be rate-limited"
            );
        }
    }

    #[test]
    fn persisted_hotkey_combos_yields_one_entry_per_combo_across_both_edges() {
        let instances = [
            instance_literal(HOTKEY_PRESSED_KIND, Some("Ctrl+F1")),
            instance_literal(HOTKEY_RELEASED_KIND, Some("Ctrl+F1")),
            instance_literal(HOTKEY_RELEASED_KIND, Some("Ctrl+F2")),
            instance_literal(HOTKEY_PRESSED_KIND, None),
            instance_literal(OTHER_KIND, Some("Ctrl+F3")),
        ];

        let combos = persisted_hotkey_combos(&instances);

        let listed: Vec<&str> = combos.iter().map(String::as_str).collect();
        assert_eq!(
            listed,
            ["Ctrl+F1", "Ctrl+F2"],
            "a hold's halves share one OS registration, and a release-only combo still needs one"
        );
    }
}
