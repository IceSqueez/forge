#![allow(clippy::unwrap_used)]
mod support;

use std::sync::Arc;

use forge_hotkey::testing::{RecordedCall, RecordingBackend};
use forge_hotkey::vocabulary::event_kinds;
use forge_hotkey::{
    BindingUpdate, HotkeyClient, HotkeyCombo, HotkeyEdge, HotkeyId, HotkeyReconciler,
    cleanup_stale_combo_instances, delete_binding, delete_binding_half, do_bind, load_bindings,
    persisted_hotkey_combos, rebind_combo, set_binding_enabled, update_binding,
};
use forge_storage::DataProvider;
use forge_types::{ActionId, TriggerInstanceId};
use support::{
    OTHER_KIND, clip, instance, recording_client, seed_action, seed_disabled_instance,
    seed_instance, store, stored_combo, stored_combos,
};

#[tokio::test]
async fn load_bindings_keeps_both_hotkey_edges_and_drops_everything_else() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F2")).await;
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, None).await;
    seed_instance(&backend, OTHER_KIND, Some("Ctrl+F3")).await;

    let rows = load_bindings(Arc::clone(&backend), Vec::new())
        .await
        .unwrap();

    let combos: Vec<&str> = rows.iter().map(|row| row.combo.as_str()).collect();
    assert_eq!(combos, ["Ctrl+F1", "Ctrl+F2"]);
}

#[tokio::test]
async fn load_bindings_sorts_rows_by_combo() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    for combo in ["Meta+B", "Alt+C", "Ctrl+A"] {
        seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some(combo)).await;
    }

    let rows = load_bindings(Arc::clone(&backend), Vec::new())
        .await
        .unwrap();

    let combos: Vec<&str> = rows.iter().map(|row| row.combo.as_str()).collect();
    assert_eq!(combos, ["Alt+C", "Ctrl+A", "Meta+B"]);
}

#[tokio::test]
async fn load_bindings_marks_a_row_registered_only_when_the_client_holds_its_combo() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F2")).await;

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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let bound = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F2")).await;
    backend
        .trigger_instance_repo()
        .link_action(action, bound)
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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    backend
        .trigger_instance_repo()
        .link_action(action, instance)
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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let other_combo = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F2")).await;
    let other_kind = seed_instance(&backend, OTHER_KIND, Some("Ctrl+F1")).await;

    cleanup_stale_combo_instances(&backend, "Ctrl+F1")
        .await
        .unwrap();

    let repo = backend.trigger_instance_repo();
    assert!(repo.get(other_combo).await.unwrap().is_some());
    assert!(repo.get(other_kind).await.unwrap().is_some());
}

#[tokio::test]
async fn load_bindings_yields_one_row_per_combo_keyed_by_the_press_half_when_there_is_one() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let hold_press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let hold_release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F1")).await;
    let press_only = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F2")).await;
    let release_only = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F3")).await;

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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    seed_disabled_instance(&backend, event_kinds::GLOBAL_RELEASED, "Ctrl+F1").await;

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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F1")).await;

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
async fn delete_binding_half_leaves_the_partner_half_and_its_action_link() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let start = seed_action(&backend, "Start").await;
    let stop = seed_action(&backend, "Stop").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(start, press).await.unwrap();
    repo.link_action(stop, release).await.unwrap();

    delete_binding_half(Arc::clone(&backend), press)
        .await
        .unwrap();

    assert!(repo.get(press).await.unwrap().is_none());
    assert_eq!(repo.actions_using(release).await.unwrap(), [stop]);
}

#[tokio::test]
async fn set_binding_enabled_applies_to_every_instance_it_is_given() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("Ctrl+F1")).await;

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
async fn set_binding_enabled_on_a_list_holding_an_unknown_id_toggles_nothing() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let known = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;

    let failed = set_binding_enabled(
        Arc::clone(&backend),
        vec![known, TriggerInstanceId::new()],
        false,
    )
    .await;

    let stored = backend.trigger_instance_repo().get(known).await.unwrap();
    assert!(failed.is_err());
    assert!(stored.unwrap().enabled);
}

#[test]
fn persisted_hotkey_combos_skips_disabled_instances() {
    let mut disabled = instance(event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1"));
    disabled.enabled = false;
    let enabled = instance(event_kinds::GLOBAL_RELEASED, Some("Ctrl+F2"));

    let combos = persisted_hotkey_combos(&[disabled, enabled]);

    assert_eq!(combos.into_iter().collect::<Vec<_>>(), ["Ctrl+F2"]);
}

#[test]
fn persisted_hotkey_combos_yields_one_entry_per_combo_across_both_edges() {
    let instances = [
        instance(event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")),
        instance(event_kinds::GLOBAL_RELEASED, Some("Ctrl+F1")),
        instance(event_kinds::GLOBAL_RELEASED, Some("Ctrl+F2")),
        instance(event_kinds::GLOBAL_PRESSED, None),
        instance(OTHER_KIND, Some("Ctrl+F3")),
    ];

    let combos = persisted_hotkey_combos(&instances);

    let listed: Vec<&str> = combos.iter().map(String::as_str).collect();
    assert_eq!(
        listed,
        ["Ctrl+F1", "Ctrl+F2"],
        "a hold's halves share one OS registration, and a release-only combo still needs one"
    );
}

#[test]
fn persisted_hotkey_combos_folds_hand_typed_spellings_into_the_canonical_combo() {
    let instances = [
        instance(event_kinds::GLOBAL_PRESSED, Some("f5")),
        instance(event_kinds::GLOBAL_RELEASED, Some("F5")),
        instance(event_kinds::GLOBAL_PRESSED, Some("shift+ctrl+1")),
    ];

    let combos = persisted_hotkey_combos(&instances);

    let listed: Vec<&str> = combos.iter().map(String::as_str).collect();
    assert_eq!(listed, ["Ctrl+Shift+1", "F5"]);
}

#[tokio::test]
async fn load_bindings_joins_a_hand_typed_half_with_its_canonical_partner_in_one_row() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("f5")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F5")).await;

    let rows = load_bindings(Arc::clone(&backend), vec![(HotkeyId(1), "F5".to_owned())])
        .await
        .unwrap();

    let summary: Vec<(&str, bool)> = rows
        .iter()
        .map(|row| (row.combo.as_str(), row.registered))
        .collect();
    assert_eq!(summary, [("F5", true)]);
}

#[tokio::test]
async fn cleanup_stale_combo_instances_also_removes_a_hand_typed_spelling_of_the_combo() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("f5")).await;

    cleanup_stale_combo_instances(&backend, "F5").await.unwrap();

    let left = backend.trigger_instance_repo().list_all().await.unwrap();
    assert!(
        left.is_empty(),
        "a legacy f5 row outlived the F5 cleanup: {left:?}"
    );
}

struct Reconciled {
    reconciler: Arc<HotkeyReconciler>,
    client: Arc<HotkeyClient>,
    recorder: Arc<RecordingBackend>,
}

impl Reconciled {
    fn id_of(&self, combo: &str) -> HotkeyId {
        self.client
            .registered_combos()
            .into_iter()
            .find(|(_, known)| known.as_str() == combo)
            .map(|(id, _)| id)
            .unwrap()
    }

    fn registered(&self) -> Vec<String> {
        let mut combos: Vec<String> = self
            .client
            .registered_combos()
            .into_iter()
            .map(|(_, combo)| combo.as_str().to_owned())
            .collect();
        combos.sort();
        combos
    }
}

async fn reconciled(backend: &Arc<dyn DataProvider>) -> Reconciled {
    let (client, recorder) = recording_client();
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        backend.trigger_instance_repo(),
        backend.soundboard_clips_repo(),
    );
    reconciler.reconcile().await;
    Reconciled {
        reconciler,
        client,
        recorder,
    }
}

async fn seed_clip_on(backend: &Arc<dyn DataProvider>, combo: &str) {
    backend
        .soundboard_clips_repo()
        .save(&clip(Some(combo)))
        .await
        .unwrap();
}

#[tokio::test]
async fn rebind_combo_registers_the_new_combo_before_releasing_the_old_one() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let old_id = rig.id_of("F1");
    let before = rig.recorder.calls().len();

    rebind_combo(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F1".to_owned(),
        "F2".to_owned(),
    )
    .await
    .unwrap();

    let calls = rig.recorder.calls().split_off(before);
    assert!(
        matches!(
            calls.as_slice(),
            [
                RecordedCall::Register(_, new),
                RecordedCall::Unregister(old),
            ] if new.as_str() == "F2" && *old == old_id
        ),
        "got {calls:?}"
    );
}

#[tokio::test]
async fn rebind_combo_keeps_the_old_binding_live_and_stored_when_the_new_combo_is_refused() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let old_id = rig.id_of("F1");
    rig.recorder
        .fail_next_register(&HotkeyCombo::parse("F2").unwrap());

    let outcome = rebind_combo(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F1".to_owned(),
        "F2".to_owned(),
    )
    .await;

    assert!(outcome.is_err());
    let live: Vec<(HotkeyId, String)> = rig
        .client
        .registered_combos()
        .into_iter()
        .map(|(id, combo)| (id, combo.as_str().to_owned()))
        .collect();
    assert_eq!(live, [(old_id, "F1".to_owned())]);
    let stored = backend.trigger_instance_repo().list_all().await.unwrap();
    assert_eq!(
        stored_combos(&stored),
        [(
            event_kinds::GLOBAL_PRESSED.to_owned(),
            Some("F1".to_owned())
        )]
    );
}

#[tokio::test]
async fn rebind_combo_moves_both_halves_of_a_hold_and_keeps_their_action_links() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Push to talk").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(action, press).await.unwrap();
    let rig = reconciled(&backend).await;

    rebind_combo(
        rig.reconciler,
        Arc::clone(&backend),
        "F1".to_owned(),
        "F2".to_owned(),
    )
    .await
    .unwrap();

    let stored = repo.list_all().await.unwrap();
    assert_eq!(
        stored_combos(&stored),
        [
            (
                event_kinds::GLOBAL_PRESSED.to_owned(),
                Some("F2".to_owned())
            ),
            (
                event_kinds::GLOBAL_RELEASED.to_owned(),
                Some("F2".to_owned())
            ),
        ]
    );
    assert_eq!(repo.actions_using(press).await.unwrap(), [action]);
}

#[tokio::test]
async fn rebind_combo_failing_midway_leaves_every_binding_and_link_as_it_was() {
    let scratch = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        scratch.path().join("qcg_atomic.db").display()
    );
    let backend: Arc<dyn DataProvider> = Arc::new(
        forge_storage_sqlite::SqliteBackend::open_for_test(
            &url,
            [0x11; 32],
            scratch.path().to_owned(),
            None,
        )
        .await
        .unwrap(),
    );
    let held = seed_action(&backend, "Push to talk").await;
    let displaced_action = seed_action(&backend, "Scene swap").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let displaced = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F2")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(held, press).await.unwrap();
    repo.link_action(displaced_action, displaced).await.unwrap();
    let rig = reconciled(&backend).await;
    let before = stored_combos(&repo.list_all().await.unwrap());
    let raw = sqlx::SqlitePool::connect(&url).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER qcg_refuse_released_move BEFORE UPDATE ON trigger_instances
         WHEN NEW.kind_id = '{}' AND NEW.name = 'F2'
         BEGIN SELECT RAISE(ABORT, 'refused'); END",
        event_kinds::GLOBAL_RELEASED
    )))
    .execute(&raw)
    .await
    .unwrap();
    raw.close().await;

    let outcome = rebind_combo(
        rig.reconciler,
        Arc::clone(&backend),
        "F1".to_owned(),
        "F2".to_owned(),
    )
    .await;

    assert!(outcome.is_err());
    assert_eq!(stored_combos(&repo.list_all().await.unwrap()), before);
    assert_eq!(repo.actions_using(press).await.unwrap(), [held]);
    assert_eq!(
        repo.actions_using(displaced).await.unwrap(),
        [displaced_action]
    );
}

#[tokio::test]
async fn rebind_combo_from_another_spelling_of_the_target_keeps_the_binding_and_its_link() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Push to talk").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(action, press).await.unwrap();
    let rig = reconciled(&backend).await;

    rebind_combo(
        rig.reconciler,
        Arc::clone(&backend),
        "ctrl+f1".to_owned(),
        "Ctrl+F1".to_owned(),
    )
    .await
    .unwrap();

    let kept = repo.get(press).await.unwrap();
    assert_eq!(kept.as_ref().and_then(stored_combo), Some("Ctrl+F1"));
    assert_eq!(repo.actions_using(press).await.unwrap(), [action]);
}

#[tokio::test]
async fn rebind_combo_from_another_spelling_moves_the_binding_and_its_link() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Push to talk").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(action, press).await.unwrap();
    let rig = reconciled(&backend).await;

    rebind_combo(
        rig.reconciler,
        Arc::clone(&backend),
        "ctrl+f1".to_owned(),
        "F2".to_owned(),
    )
    .await
    .unwrap();

    let moved = repo.get(press).await.unwrap();
    assert_eq!(moved.as_ref().and_then(stored_combo), Some("F2"));
    assert_eq!(repo.actions_using(press).await.unwrap(), [action]);
}

#[tokio::test]
async fn rebind_combo_to_the_same_combo_leaves_the_client_and_storage_alone() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let before = rig.recorder.calls();

    rebind_combo(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F1".to_owned(),
        "F1".to_owned(),
    )
    .await
    .unwrap();

    assert_eq!(rig.recorder.calls(), before);
    let kept = backend.trigger_instance_repo().get(press).await.unwrap();
    assert!(kept.is_some(), "rebinding onto itself dropped the binding");
}
#[tokio::test]
async fn do_bind_registers_the_combo_and_stores_the_binding() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Scene swap").await;
    let rig = reconciled(&backend).await;

    do_bind(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F3".to_owned(),
        HotkeyEdge::Press,
        action,
    )
    .await
    .unwrap();

    let stored = backend.trigger_instance_repo().list_all().await.unwrap();
    assert_eq!(
        stored_combos(&stored),
        [(
            event_kinds::GLOBAL_PRESSED.to_owned(),
            Some("F3".to_owned())
        )]
    );
    assert_eq!(rig.registered(), ["F3"]);
}

#[tokio::test]
async fn do_bind_keeps_the_old_binding_when_the_new_link_cannot_land() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let reconciled = reconciled(&backend).await;
    let action = seed_action(&backend, "Play sound").await;
    do_bind(
        Arc::clone(&reconciled.reconciler),
        Arc::clone(&backend),
        "Ctrl+F1".to_owned(),
        HotkeyEdge::Press,
        action,
    )
    .await
    .unwrap();
    let repo = backend.trigger_instance_repo();
    let bound = repo.list_for_action(action).await.unwrap();

    let failed = do_bind(
        Arc::clone(&reconciled.reconciler),
        Arc::clone(&backend),
        "Ctrl+F1".to_owned(),
        HotkeyEdge::Press,
        ActionId::new(),
    )
    .await;

    assert!(failed.is_err());
    assert_eq!(repo.list_for_action(action).await.unwrap(), bound);
    assert_eq!(reconciled.registered(), ["Ctrl+F1"]);
}

#[tokio::test]
async fn do_bind_with_a_combo_the_os_refuses_stores_nothing() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Scene swap").await;
    let rig = reconciled(&backend).await;
    rig.recorder
        .fail_next_register(&HotkeyCombo::parse("F3").unwrap());

    let outcome = do_bind(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F3".to_owned(),
        HotkeyEdge::Press,
        action,
    )
    .await;

    assert!(outcome.is_err());
    let stored = backend.trigger_instance_repo().list_all().await.unwrap();
    assert!(stored.is_empty(), "stored {stored:?}");
}

#[tokio::test]
async fn delete_binding_releases_a_combo_nothing_else_binds() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F5")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F5")).await;
    let rig = reconciled(&backend).await;

    delete_binding(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F5".to_owned(),
    )
    .await
    .unwrap();

    assert!(rig.registered().is_empty());
}

#[tokio::test]
async fn delete_binding_keeps_the_registration_a_clip_is_still_bound_to() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F5")).await;
    seed_clip_on(&backend, "F5").await;
    let rig = reconciled(&backend).await;

    delete_binding(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F5".to_owned(),
    )
    .await
    .unwrap();

    assert_eq!(rig.registered(), ["F5"]);
}

#[tokio::test]
async fn rebind_combo_off_a_combo_a_clip_uses_keeps_that_combo_registered() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    seed_clip_on(&backend, "F1").await;
    let rig = reconciled(&backend).await;

    rebind_combo(
        Arc::clone(&rig.reconciler),
        Arc::clone(&backend),
        "F1".to_owned(),
        "F2".to_owned(),
    )
    .await
    .unwrap();

    assert_eq!(rig.registered(), ["F1", "F2"]);
}

fn update_to(
    instance_id: TriggerInstanceId,
    combo: &str,
    edge: HotkeyEdge,
    action_id: ActionId,
) -> BindingUpdate {
    BindingUpdate {
        instance_id,
        combo: combo.to_owned(),
        edge,
        action_id,
    }
}

async fn apply_update(
    backend: &Arc<dyn DataProvider>,
    rig: &Reconciled,
    update: BindingUpdate,
) -> Result<(), String> {
    update_binding(Arc::clone(&rig.reconciler), Arc::clone(backend), update).await
}

async fn stored_kind_and_combo(
    backend: &Arc<dyn DataProvider>,
    instance: TriggerInstanceId,
) -> Option<(String, Option<String>)> {
    backend
        .trigger_instance_repo()
        .get(instance)
        .await
        .unwrap()
        .map(|stored| {
            (
                stored.kind_id.clone(),
                stored_combo(&stored).map(str::to_owned),
            )
        })
}

fn pressed_on(combo: &str) -> Option<(String, Option<String>)> {
    Some((
        event_kinds::GLOBAL_PRESSED.to_owned(),
        Some(combo.to_owned()),
    ))
}

#[tokio::test]
async fn update_binding_changes_combo_edge_and_action_as_one_write() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let old_action = seed_action(&backend, "Old").await;
    let new_action = seed_action(&backend, "New").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(old_action, instance).await.unwrap();
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Release, new_action),
    )
    .await
    .unwrap();

    let stored = repo.get(instance).await.unwrap().unwrap();
    assert_eq!(
        (
            stored.kind_id.as_str(),
            stored_combo(&stored),
            stored.name.as_str()
        ),
        (event_kinds::GLOBAL_RELEASED, Some("F2"), "F2")
    );
    assert_eq!(repo.actions_using(instance).await.unwrap(), [new_action]);
    assert_eq!(rig.registered(), ["F2"]);
}

#[tokio::test]
async fn update_binding_registers_the_new_combo_before_releasing_the_old_one() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let old_id = rig.id_of("F1");
    let before = rig.recorder.calls().len();

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    let calls = rig.recorder.calls().split_off(before);
    assert!(
        matches!(
            calls.as_slice(),
            [
                RecordedCall::Register(_, new),
                RecordedCall::Unregister(old),
            ] if new.as_str() == "F2" && *old == old_id
        ),
        "got {calls:?}"
    );
}

#[tokio::test]
async fn update_binding_stores_the_canonical_spelling_of_a_hand_typed_combo() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "shift+ctrl+f5", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("Ctrl+Shift+F5")
    );
}

#[tokio::test]
async fn update_binding_gives_a_combo_to_a_binding_that_had_none() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, None).await;
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("F2")
    );
    assert_eq!(rig.registered(), ["F2"]);
}

#[tokio::test]
async fn update_binding_leaves_the_link_order_alone_when_the_action_is_unchanged() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(action, instance).await.unwrap();
    let sibling = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F2")).await;
    repo.link_action(action, sibling).await.unwrap();
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F3", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    let order: Vec<TriggerInstanceId> = repo
        .list_for_action(action)
        .await
        .unwrap()
        .into_iter()
        .map(|i| i.id)
        .collect();
    assert_eq!(order, [instance, sibling]);
}

#[tokio::test]
async fn update_binding_to_a_new_action_appends_after_that_actions_triggers() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let repo = backend.trigger_instance_repo();
    let existing = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F2")).await;
    repo.link_action(action, existing).await.unwrap();
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F1", HotkeyEdge::Press, action),
    )
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

#[tokio::test]
async fn update_binding_moves_both_halves_of_a_hold_and_keeps_the_partner_link() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press_action = seed_action(&backend, "Down").await;
    let release_action = seed_action(&backend, "Up").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(press_action, press).await.unwrap();
    repo.link_action(release_action, release).await.unwrap();
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(press, "F2", HotkeyEdge::Press, press_action),
    )
    .await
    .unwrap();

    assert_eq!(
        stored_kind_and_combo(&backend, release).await,
        Some((
            event_kinds::GLOBAL_RELEASED.to_owned(),
            Some("F2".to_owned())
        ))
    );
    assert_eq!(repo.actions_using(release).await.unwrap(), [release_action]);
    assert_eq!(rig.registered(), ["F2"]);
}

#[tokio::test]
async fn update_binding_refuses_an_edge_the_combo_already_binds_and_writes_nothing() {
    for target in ["F1", "F2"] {
        let store = store().await;
        let backend = Arc::clone(&store.provider);
        let action = seed_action(&backend, "Play sound").await;
        let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
        seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
        let rig = reconciled(&backend).await;

        let refused = apply_update(
            &backend,
            &rig,
            update_to(press, target, HotkeyEdge::Release, action),
        )
        .await;

        assert!(refused.is_err(), "target {target}");
        assert_eq!(
            stored_kind_and_combo(&backend, press).await,
            pressed_on("F1"),
            "target {target}"
        );
        assert_eq!(rig.registered(), ["F1"], "target {target}");
    }
}

#[tokio::test]
async fn update_binding_moves_onto_an_edge_only_another_combo_binds() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let other = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F2")).await;
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(press, "F1", HotkeyEdge::Release, action),
    )
    .await
    .unwrap();

    assert_eq!(
        stored_kind_and_combo(&backend, press).await,
        Some((
            event_kinds::GLOBAL_RELEASED.to_owned(),
            Some("F1".to_owned())
        ))
    );
    assert_eq!(
        stored_kind_and_combo(&backend, other).await,
        Some((
            event_kinds::GLOBAL_RELEASED.to_owned(),
            Some("F2".to_owned())
        ))
    );
}

#[tokio::test]
async fn update_binding_on_an_instance_that_no_longer_exists_touches_nothing() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let rig = reconciled(&backend).await;

    let result = apply_update(
        &backend,
        &rig,
        update_to(TriggerInstanceId::new(), "F2", HotkeyEdge::Press, action),
    )
    .await;

    assert!(result.is_err());
    assert!(rig.recorder.calls().is_empty());
}

#[tokio::test]
async fn update_binding_with_an_unparseable_combo_touches_nothing() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;

    let result = apply_update(
        &backend,
        &rig,
        update_to(instance, "Ctrl+XYZ", HotkeyEdge::Press, action),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("F1")
    );
    assert_eq!(rig.registered(), ["F1"]);
}

#[tokio::test]
async fn update_binding_keeps_the_old_binding_when_the_os_refuses_the_new_combo() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let old_action = seed_action(&backend, "Old").await;
    let new_action = seed_action(&backend, "New").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(old_action, instance).await.unwrap();
    let rig = reconciled(&backend).await;
    rig.recorder
        .fail_next_register(&HotkeyCombo::parse("F2").unwrap());

    let result = apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Release, new_action),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("F1")
    );
    assert_eq!(repo.actions_using(instance).await.unwrap(), [old_action]);
    assert_eq!(rig.registered(), ["F1"]);
}

#[tokio::test]
async fn a_failed_update_binding_write_rolls_back_every_part_and_releases_the_claim() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let old_action = seed_action(&backend, "Old").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let displaced = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F2")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(old_action, instance).await.unwrap();
    let rig = reconciled(&backend).await;
    let action_without_a_row = ActionId::new();

    let result = apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Press, action_without_a_row),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("F1")
    );
    assert_eq!(
        stored_kind_and_combo(&backend, displaced).await,
        pressed_on("F2")
    );
    assert_eq!(repo.actions_using(instance).await.unwrap(), [old_action]);
    assert_eq!(rig.registered(), ["F1", "F2"]);
}

#[tokio::test]
async fn a_failed_update_binding_write_releases_a_claim_nothing_else_uses() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let rig = reconciled(&backend).await;

    let result = apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Press, ActionId::new()),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(rig.registered(), ["F1"]);
}

#[tokio::test]
async fn update_binding_onto_a_used_combo_displaces_the_stale_rows_and_their_links() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let stale_action = seed_action(&backend, "Stale").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let stale_press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F2")).await;
    let stale_release = seed_disabled_instance(&backend, event_kinds::GLOBAL_RELEASED, "F2").await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(stale_action, stale_press).await.unwrap();
    let rig = reconciled(&backend).await;

    apply_update(
        &backend,
        &rig,
        update_to(instance, "F2", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    assert!(repo.get(stale_press).await.unwrap().is_none());
    assert!(repo.get(stale_release).await.unwrap().is_none());
    assert!(repo.list_for_action(stale_action).await.unwrap().is_empty());
    assert_eq!(
        stored_kind_and_combo(&backend, instance).await,
        pressed_on("F2")
    );
    assert_eq!(rig.registered(), ["F2"]);
}

#[tokio::test]
async fn update_binding_on_the_same_combo_spares_the_partner_half_and_the_client() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let before = rig.recorder.calls().len();

    apply_update(
        &backend,
        &rig,
        update_to(press, "F1", HotkeyEdge::Press, action),
    )
    .await
    .unwrap();

    assert!(
        backend
            .trigger_instance_repo()
            .get(release)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(rig.recorder.calls().len(), before);
}

#[tokio::test]
async fn disabling_both_halves_through_the_store_releases_the_combo_on_the_next_reconcile() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    let release = seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let rig = reconciled(&backend).await;
    let id = rig.id_of("F1");

    set_binding_enabled(Arc::clone(&backend), vec![press, release], false)
        .await
        .unwrap();
    rig.reconciler.reconcile().await;

    assert_eq!(
        rig.recorder.calls().last(),
        Some(&RecordedCall::Unregister(id))
    );
    assert!(rig.registered().is_empty());
}

#[tokio::test]
async fn disabling_one_half_keeps_the_grab_the_other_half_needs() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    seed_instance(&backend, event_kinds::GLOBAL_RELEASED, Some("F1")).await;
    let rig = reconciled(&backend).await;

    set_binding_enabled(Arc::clone(&backend), vec![press], false)
        .await
        .unwrap();
    rig.reconciler.reconcile().await;

    assert_eq!(rig.registered(), ["F1"]);
}

#[tokio::test]
async fn disabling_a_binding_keeps_the_combo_a_clip_is_bound_to() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let press = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("F1")).await;
    seed_clip_on(&backend, "F1").await;
    let rig = reconciled(&backend).await;

    set_binding_enabled(Arc::clone(&backend), vec![press], false)
        .await
        .unwrap();
    rig.reconciler.reconcile().await;

    assert_eq!(rig.registered(), ["F1"]);
}
