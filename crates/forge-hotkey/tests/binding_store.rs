#![allow(clippy::unwrap_used)]
mod support;

use std::sync::Arc;

use forge_hotkey::testing::{RecordedCall, RecordingBackend};
use forge_hotkey::{
    HotkeyClient, HotkeyCombo, HotkeyEdge, HotkeyId, HotkeyReconciler,
    cleanup_stale_combo_instances, delete_binding, delete_binding_half, do_bind, event_kinds,
    load_bindings, persisted_hotkey_combos, rebind_combo, relink_action, set_binding_edge,
    set_binding_enabled,
};
use forge_storage::DataProvider;
use forge_types::TriggerInstanceId;
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
async fn relink_action_moves_the_binding_from_the_previous_action_to_the_new_one() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let old_action = seed_action(&backend, "Old").await;
    let new_action = seed_action(&backend, "New").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(old_action, instance).await.unwrap();

    relink_action(Arc::clone(&backend), instance, new_action)
        .await
        .unwrap();

    assert_eq!(repo.actions_using(instance).await.unwrap(), [new_action]);
}

#[tokio::test]
async fn relink_action_leaves_the_existing_link_in_place_when_the_action_is_unchanged() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;
    let repo = backend.trigger_instance_repo();
    repo.link_action(action, instance).await.unwrap();
    let mut expected = vec![instance];
    for (offset, combo) in ["Ctrl+F2", "Ctrl+F3"].into_iter().enumerate() {
        let sibling = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some(combo)).await;
        repo.link_action_at(action, sibling, offset as i64 + 1)
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
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let action = seed_action(&backend, "Play sound").await;
    let repo = backend.trigger_instance_repo();
    for (position, combo) in ["Ctrl+F2", "Ctrl+F3"].into_iter().enumerate() {
        let existing = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some(combo)).await;
        repo.link_action_at(action, existing, position as i64)
            .await
            .unwrap();
    }
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;

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
async fn set_binding_edge_rewrites_the_kind_and_keeps_the_combo_override() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);
    let instance = seed_instance(&backend, event_kinds::GLOBAL_PRESSED, Some("Ctrl+F1")).await;

    set_binding_edge(Arc::clone(&backend), instance, HotkeyEdge::Release)
        .await
        .unwrap();

    let stored = backend
        .trigger_instance_repo()
        .get(instance)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.kind_id, event_kinds::GLOBAL_RELEASED);
    assert_eq!(
        stored_combo(&stored),
        Some("Ctrl+F1"),
        "moving an edge must not lose the combo the instance matches on"
    );
}

#[tokio::test]
async fn set_binding_edge_reports_an_instance_that_no_longer_exists() {
    let store = store().await;
    let backend = Arc::clone(&store.provider);

    let result = set_binding_edge(
        Arc::clone(&backend),
        TriggerInstanceId::new(),
        HotkeyEdge::Release,
    )
    .await;

    assert!(result.is_err());
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
