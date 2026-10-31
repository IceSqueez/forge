use super::mapping_store::{MIDI_KNOWN_DEVICES, load_mappings, save_mapping};
use super::selection::{follow_device, resolve_selection, visible_mappings};
use crate::midi_mapping_modal::MappingDraft;
use forge_types::{PermissionRung, PlatformScope, TriggerInstance};
use std::sync::Mutex;

use async_trait::async_trait;
use forge_storage::{ActionTelemetry, ExecutionStatus, StorageError};
use forge_types::{Action, ExecutionMode, QueueId, TriggerConfig, Variant};
use time::OffsetDateTime;

use super::*;
use forge_midi::vocabulary::event_kinds::{CONTROL_CHANGE, NOTE_ON};

#[derive(Default)]
struct TriggerStore {
    instances: Vec<TriggerInstance>,
    links: Vec<(ActionId, TriggerInstanceId, i64)>,
    unlinks: usize,
    fail_save: bool,
}

#[derive(Default)]
struct FakeTriggerRepo {
    state: Mutex<TriggerStore>,
}

impl FakeTriggerRepo {
    fn seed(&self, instance: TriggerInstance) {
        self.lock().instances.push(instance);
    }

    fn seed_link(&self, action_id: ActionId, instance_id: TriggerInstanceId, position: i64) {
        self.lock().links.push((action_id, instance_id, position));
    }

    fn failing_save() -> Self {
        let repo = Self::default();
        repo.lock().fail_save = true;
        repo
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TriggerStore> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn instances(&self) -> Vec<TriggerInstance> {
        self.lock().instances.clone()
    }

    fn links(&self) -> Vec<(ActionId, TriggerInstanceId, i64)> {
        self.lock().links.clone()
    }

    fn unlinks(&self) -> usize {
        self.lock().unlinks
    }
}

#[async_trait]
impl TriggerInstanceRepo for FakeTriggerRepo {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(self.instances())
    }

    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        let store = self.lock();
        Ok(store
            .links
            .iter()
            .filter(|(action, _, _)| *action == action_id)
            .filter_map(|(_, instance_id, _)| {
                store
                    .instances
                    .iter()
                    .find(|i| i.id == *instance_id)
                    .cloned()
            })
            .collect())
    }

    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError> {
        Ok(self
            .lock()
            .links
            .iter()
            .filter(|(_, linked, _)| *linked == instance_id)
            .map(|(action, _, _)| *action)
            .collect())
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError> {
        let mut store = self.lock();
        let position = store
            .links
            .iter()
            .filter(|(action, _, _)| *action == action_id)
            .count() as i64;
        store.links.push((action_id, instance_id, position));
        Ok(())
    }

    async fn link_action_at(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError> {
        self.lock().links.push((action_id, instance_id, position));
        Ok(())
    }

    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        let mut store = self.lock();
        store.unlinks += 1;
        let before = store.links.len();
        store
            .links
            .retain(|(action, linked, _)| !(*action == action_id && *linked == instance_id));
        Ok(store.links.len() != before)
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        Ok(self.lock().instances.iter().find(|i| i.id == id).cloned())
    }

    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        let mut store = self.lock();
        if store.fail_save {
            return Err(StorageError::Connection {
                reason: "forced write failure".to_owned(),
            });
        }
        match store.instances.iter_mut().find(|i| i.id == instance.id) {
            Some(existing) => *existing = instance.clone(),
            None => store.instances.push(instance.clone()),
        }
        Ok(())
    }

    async fn delete(&self, _id: TriggerInstanceId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn upsert_default(
        &self,
        _kind_id: &str,
        _name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn set_enabled(
        &self,
        _id: TriggerInstanceId,
        _enabled: bool,
    ) -> Result<(), StorageError> {
        Err(StorageError::NotReady)
    }
}

#[derive(Default)]
struct FakeActionRepo {
    actions: Mutex<Vec<Action>>,
}

impl FakeActionRepo {
    fn with(actions: Vec<Action>) -> Self {
        Self {
            actions: Mutex::new(actions),
        }
    }
}

#[async_trait]
impl ActionRepo for FakeActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self
            .actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone())
    }

    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self
            .actions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|a| a.id == id)
            .cloned())
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Err(StorageError::NotReady)
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn record_execution(
        &self,
        _action_id: ActionId,
        _started_at: OffsetDateTime,
        _duration_ms: u64,
        _status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Err(StorageError::NotReady)
    }

    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Err(StorageError::NotReady)
    }
}

fn action(name: &str) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions: vec![],
    }
}

fn stored_instance(kind_id: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: "Stored name".to_owned(),
        overrides: TriggerConfig::new(),
        enabled: false,
        user_defined: false,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 45,
        cooldown_global: false,
        permission_rung: PermissionRung::Everyone,
    }
}

fn note_signal(selector: i64) -> MidiSignal {
    MidiSignal {
        kind_id: NOTE_ON.to_owned(),
        selector: Some(selector),
        channel: Some(3),
        device: Some("Keys".to_owned()),
    }
}

fn draft(
    instance_id: Option<TriggerInstanceId>,
    signal: MidiSignal,
    action_id: ActionId,
) -> MappingDraft {
    MappingDraft {
        instance_id,
        signal,
        action_id,
        name: "Draft name".to_owned(),
    }
}

#[tokio::test]
async fn save_mapping_creates_an_enabled_user_defined_instance_for_a_new_draft() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);

    save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id))
        .await
        .unwrap();

    let stored = triggers.instances();
    assert_eq!(stored.len(), 1);
    let created = &stored[0];
    assert!(created.enabled, "a fresh mapping must be active");
    assert!(created.user_defined, "a fresh mapping must be user defined");
    assert_eq!(created.name, "Draft name");
    assert_eq!(created.kind_id, NOTE_ON);
    assert_eq!(created.overrides.get("note"), Some(&Variant::Int(60)));
    assert_eq!(created.overrides.get("channel"), Some(&Variant::Int(3)));
    assert_eq!(
        created.overrides.get("device"),
        Some(&Variant::String("Keys".to_owned()))
    );
}

#[tokio::test]
async fn save_mapping_links_a_new_mapping_to_the_drafted_action() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);
    let existing = stored_instance("core.manual");
    triggers.seed_link(target.id, existing.id, 0);
    triggers.seed(existing);

    save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id))
        .await
        .unwrap();

    let created = triggers.instances()[1].id;
    assert!(
        triggers
            .links()
            .iter()
            .any(|(action, instance, _)| *action == target.id && *instance == created),
        "the new mapping must be linked to the drafted action"
    );
}

#[tokio::test]
async fn save_mapping_renames_from_the_draft_and_keeps_cooldowns_when_editing() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);
    let existing = stored_instance(NOTE_ON);
    let existing_id = existing.id;
    triggers.seed(existing);

    let mut signal = note_signal(7);
    signal.kind_id = CONTROL_CHANGE.to_owned();
    save_mapping(
        &triggers,
        &actions,
        draft(Some(existing_id), signal, target.id),
    )
    .await
    .unwrap();

    let saved = triggers.instances().remove(0);
    assert_eq!(saved.name, "Draft name");
    assert_eq!(saved.cooldown_secs, 45);
    assert!(!saved.cooldown_global);
    assert!(
        !saved.enabled,
        "editing must not silently re-enable a mapping"
    );
    assert_eq!(saved.kind_id, CONTROL_CHANGE);
    assert_eq!(saved.overrides.get("controller"), Some(&Variant::Int(7)));
}

#[tokio::test]
async fn save_mapping_leaves_the_link_untouched_when_the_action_did_not_change() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);
    let existing = stored_instance(NOTE_ON);
    let existing_id = existing.id;
    triggers.seed(existing);
    triggers.seed_link(target.id, existing_id, 4);

    save_mapping(
        &triggers,
        &actions,
        draft(Some(existing_id), note_signal(61), target.id),
    )
    .await
    .unwrap();

    assert_eq!(
        triggers.unlinks(),
        0,
        "an unchanged action must not be relinked"
    );
    assert_eq!(triggers.links(), vec![(target.id, existing_id, 4)]);
}

#[tokio::test]
async fn save_mapping_moves_the_link_when_the_draft_points_at_another_action() {
    let triggers = FakeTriggerRepo::default();
    let previous = action("Old target");
    let next = action("New target");
    let actions = FakeActionRepo::with(vec![previous.clone(), next.clone()]);
    let existing = stored_instance(NOTE_ON);
    let existing_id = existing.id;
    triggers.seed(existing);
    triggers.seed_link(previous.id, existing_id, 0);

    save_mapping(
        &triggers,
        &actions,
        draft(Some(existing_id), note_signal(61), next.id),
    )
    .await
    .unwrap();

    assert_eq!(triggers.links(), vec![(next.id, existing_id, 0)]);
}

#[tokio::test]
async fn save_mapping_reports_a_missing_instance_instead_of_recreating_it() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);

    let result = save_mapping(
        &triggers,
        &actions,
        draft(Some(TriggerInstanceId::new()), note_signal(60), target.id),
    )
    .await;

    assert!(result.is_err());
    assert!(triggers.instances().is_empty());
}

#[tokio::test]
async fn save_mapping_surfaces_a_repo_write_failure_as_an_error() {
    let triggers = FakeTriggerRepo::failing_save();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);

    let result = save_mapping(&triggers, &actions, draft(None, note_signal(60), target.id)).await;

    assert!(result.is_err());
    assert!(
        triggers.links().is_empty(),
        "a failed write must not link the action"
    );
}

#[tokio::test]
async fn load_mappings_returns_only_midi_input_rows_with_their_linked_action() {
    let triggers = FakeTriggerRepo::default();
    let target = action("Play sound");
    let actions = FakeActionRepo::with(vec![target.clone()]);
    let other_family = stored_instance("twitch.chat");
    triggers.seed(other_family);
    let midi = stored_instance(NOTE_ON);
    let midi_id = midi.id;
    triggers.seed(midi);
    triggers.seed_link(target.id, midi_id, 0);

    let rows = load_mappings(&triggers, &actions).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, midi_id);
    assert_eq!(
        rows[0].action.as_ref().map(|(_, name)| name.as_str()),
        Some("Play sound")
    );
}

#[tokio::test]
async fn load_mappings_reports_a_row_whose_linked_action_no_longer_exists_as_unassigned() {
    let triggers = FakeTriggerRepo::default();
    let actions = FakeActionRepo::default();
    let midi = stored_instance(NOTE_ON);
    let midi_id = midi.id;
    triggers.seed(midi);
    triggers.seed_link(ActionId::new(), midi_id, 0);

    let rows = load_mappings(&triggers, &actions).await.unwrap();

    assert!(rows[0].action.is_none());
}

fn mapping_row(name: &str, device: Option<&str>) -> MappingRow {
    MappingRow {
        id: TriggerInstanceId::new(),
        name: name.to_owned(),
        signal: MidiSignal {
            kind_id: NOTE_ON.to_owned(),
            selector: Some(60),
            channel: Some(1),
            device: device.map(str::to_owned),
        },
        action: None,
    }
}

fn devices(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

#[test]
fn visible_mappings_shows_unbound_rows_next_to_the_selected_devices_own_rows() {
    let rows = vec![
        mapping_row("keys", Some("Keys")),
        mapping_row("pad", Some("Pad")),
        mapping_row("any", None),
    ];

    for (selected, expected) in [
        (Some("Keys"), vec!["keys", "any"]),
        (Some("Pad"), vec!["pad", "any"]),
        (Some("Ghost"), vec!["any"]),
        (None, vec!["keys", "pad", "any"]),
    ] {
        let names: Vec<&str> = visible_mappings(&rows, selected)
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names, expected, "selected {selected:?}");
    }
}

#[test]
fn resolve_selection_keeps_a_listed_device_and_otherwise_falls_back_to_the_first() {
    for (known, current, expected) in [
        (devices(&["A", "B"]), None, Some("A")),
        (devices(&["A", "B"]), Some("B"), Some("B")),
        (devices(&["A", "B"]), Some("Gone"), Some("A")),
        (devices(&[]), Some("A"), None),
        (devices(&[]), None, None),
    ] {
        assert_eq!(
            resolve_selection(&known, current).as_deref(),
            expected,
            "known {known:?} current {current:?}"
        );
    }
}

#[test]
fn follow_device_switches_only_to_a_known_device_that_is_not_already_selected() {
    let known = devices(&["A", "B"]);

    for (current, saved, expected) in [
        (Some("A"), Some("B"), Some("B")),
        (None, Some("B"), Some("B")),
        (Some("A"), Some("A"), None),
        (Some("A"), Some("Ghost"), None),
        (Some("A"), None, None),
    ] {
        assert_eq!(
            follow_device(&known, current, saved).as_deref(),
            expected,
            "current {current:?} saved {saved:?}"
        );
    }
}

#[gpui::test]
fn the_crumb_leads_back_to_the_midi_category_on_the_hub(cx: &mut gpui::TestAppContext) {
    let rt = crate::test_support::runtime();
    let bus = crate::test_support::quiet_bus(&rt);
    let storage = rt.block_on(crate::test_support::sandboxed_provider());
    let (settings, _writes) = crate::test_support::test_backend();
    let trigger_repo = storage.trigger_instance_repo();
    let action_repo = storage.action_repo();
    let handle = rt.handle().clone();

    let category = forge_midi::MIDI_INTEGRATION.category;
    let targets = crate::test_support::hub_crumb_targets(cx, Some(category), |_, cx| {
        MidiScreenView::new(
            SwitchableMidiSink::new(),
            None,
            trigger_repo,
            action_repo,
            settings as Arc<dyn SettingsRepo>,
            bus,
            handle,
            cx,
        )
    });

    assert_eq!(
        targets,
        vec![crate::navigation::Screen::Integrations(Some(category))]
    );
}

const STORED_DEVICES: &str = r#"["Keys B"]"#;
const KNOWN_PORT: &str = "Keys B";
const NEW_PORT: &str = "Pads A";

struct DeviceHarness {
    rt: tokio::runtime::Runtime,
    backend: Arc<crate::test_support::TestBackend>,
    writes: tokio::sync::mpsc::UnboundedReceiver<crate::test_support::SettingWrite>,
}

impl DeviceHarness {
    fn new(stored: &str, cx: &mut gpui::TestAppContext) -> Self {
        crate::test_support::install_presentation(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let (backend, writes) = crate::test_support::test_backend();
        backend.seed_raw(&MIDI_KNOWN_DEVICES, stored);
        Self {
            rt: crate::test_support::runtime(),
            backend,
            writes,
        }
    }

    fn open(&self, cx: &mut gpui::TestAppContext) -> Entity<MidiScreenView> {
        let settings = Arc::clone(&self.backend) as Arc<dyn SettingsRepo>;
        let bus = crate::test_support::quiet_bus(&self.rt);
        let handle = self.rt.handle().clone();
        let view = cx.update(|cx| {
            cx.new(|cx| {
                MidiScreenView::new(
                    SwitchableMidiSink::new(),
                    None,
                    Arc::new(FakeTriggerRepo::default()),
                    Arc::new(FakeActionRepo::with(Vec::new())),
                    settings,
                    bus,
                    handle,
                    cx,
                )
            })
        });
        self.settle(cx);
        view
    }

    fn settle(&self, cx: &mut gpui::TestAppContext) {
        for _ in 0..3 {
            crate::test_support::pump(&self.rt);
            cx.run_until_parked();
        }
    }

    fn device_writes(&mut self) -> Vec<Vec<String>> {
        crate::test_support::writes_to(&mut self.writes, MIDI_KNOWN_DEVICES.name())
            .iter()
            .map(|raw| serde_json::from_str(raw).unwrap())
            .collect()
    }
}

fn plug_in(view: &Entity<MidiScreenView>, cx: &mut gpui::TestAppContext) {
    view.update(cx, |view, cx| {
        view.live_ports = vec![NEW_PORT.to_owned()];
        view.remember_live_ports(cx);
    });
}

fn device_failure(view: &Entity<MidiScreenView>, cx: &mut gpui::TestAppContext) -> Option<String> {
    view.read_with(cx, |view, _| view.known_devices.failure())
}

fn merged() -> Vec<String> {
    vec![KNOWN_PORT.to_owned(), NEW_PORT.to_owned()]
}

#[gpui::test]
fn a_live_port_after_a_failed_load_writes_nothing_over_the_known_devices(
    cx: &mut gpui::TestAppContext,
) {
    let unreachable = crate::test_support::shown_in_english(StorageError::Connection {
        reason: crate::test_support::SETTINGS_READ_FAILED.to_owned(),
    });
    let undecodable = crate::test_support::shown_in_english(StorageError::UndecodableSetting {
        key: MIDI_KNOWN_DEVICES.name().to_owned(),
    });
    for (fault, stored, failing_read, reason) in [
        ("unreachable store", STORED_DEVICES, true, unreachable),
        ("undecodable value", "not json", false, undecodable),
    ] {
        let mut harness = DeviceHarness::new(stored, cx);
        if failing_read {
            harness.backend.fault_reads(
                MIDI_KNOWN_DEVICES.name(),
                crate::test_support::ReadFault::Fails,
            );
        }
        let view = harness.open(cx);

        plug_in(&view, cx);
        harness.settle(cx);

        assert_eq!(
            (harness.device_writes(), device_failure(&view, cx)),
            (Vec::<Vec<String>>::new(), Some(reason)),
            "{fault}"
        );
    }
}

#[gpui::test]
fn a_port_seen_while_the_load_is_pending_is_merged_and_written_once_the_load_lands(
    cx: &mut gpui::TestAppContext,
) {
    let mut harness = DeviceHarness::new(STORED_DEVICES, cx);
    let gate = Arc::new(tokio::sync::Notify::new());
    harness.backend.fault_reads(
        MIDI_KNOWN_DEVICES.name(),
        crate::test_support::ReadFault::Held(Arc::clone(&gate)),
    );
    let view = harness.open(cx);

    plug_in(&view, cx);
    harness.settle(cx);
    let while_pending = harness.device_writes();
    harness.backend.heal_reads(MIDI_KNOWN_DEVICES.name());
    gate.notify_one();
    harness.settle(cx);

    assert_eq!(
        (while_pending, harness.device_writes()),
        (Vec::<Vec<String>>::new(), vec![merged()])
    );
}

#[gpui::test]
fn retry_after_a_failed_load_restores_the_known_devices_and_a_new_port_keeps_them(
    cx: &mut gpui::TestAppContext,
) {
    let mut harness = DeviceHarness::new(STORED_DEVICES, cx);
    harness.backend.fault_reads(
        MIDI_KNOWN_DEVICES.name(),
        crate::test_support::ReadFault::Fails,
    );
    let view = harness.open(cx);
    harness.backend.heal_reads(MIDI_KNOWN_DEVICES.name());

    view.update(cx, |view, cx| view.load_known_devices(cx));
    harness.settle(cx);
    plug_in(&view, cx);
    harness.settle(cx);

    assert_eq!(harness.device_writes(), vec![merged()]);
}

#[gpui::test]
fn a_failed_device_write_shows_the_error_without_reloading_into_another_write(
    cx: &mut gpui::TestAppContext,
) {
    let read_only = crate::test_support::shown_in_english(StorageError::Connection {
        reason: crate::test_support::SETTINGS_WRITE_FAILED.to_owned(),
    });
    let harness = DeviceHarness::new(STORED_DEVICES, cx);
    harness.backend.fail_writes(MIDI_KNOWN_DEVICES.name());
    let view = harness.open(cx);
    let reload_gate = Arc::new(tokio::sync::Notify::new());
    harness.backend.fault_reads(
        MIDI_KNOWN_DEVICES.name(),
        crate::test_support::ReadFault::Held(reload_gate),
    );

    plug_in(&view, cx);
    harness.settle(cx);

    assert_eq!(device_failure(&view, cx), Some(read_only));
}
