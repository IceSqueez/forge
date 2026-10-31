#![allow(clippy::unwrap_used)]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use forge_hotkey::ClipBindingsSource as _;
use forge_hotkey::testing::{RecordedCall, RecordingBackend};
use forge_hotkey::vocabulary::event_kinds;
use forge_hotkey::{
    CLIP_COMBO_FIELD, HotkeyClient, HotkeyCombo, HotkeyReconciler, HotkeySyncedClipsRepo,
    HotkeySyncedTriggerRepo,
};
use forge_storage::MockSoundboardClipsRepo;
use forge_storage::{
    MediaBlobId, MockTriggerInstanceRepo, SoundboardClipsRepo, StorageError, StoredClip,
    TriggerChanges, TriggerInstanceRepo, TriggerWiring,
};
use forge_types::{ClipId, Queue, QueueId, TriggerInstanceId};
use support::{
    Store, clip, instance, recording_client, registered, seed_action, store, stored_combo,
    with_combo,
};
use tokio::sync::Notify;

const STILL_WAITING: Duration = Duration::from_millis(50);

struct Synced {
    store: Store,
    stored: Arc<dyn TriggerInstanceRepo>,
    stored_clips: Arc<dyn SoundboardClipsRepo>,
    repo: Arc<dyn TriggerInstanceRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
    reconciler: Arc<HotkeyReconciler>,
    client: Arc<HotkeyClient>,
    backend: Arc<RecordingBackend>,
}

async fn synced() -> Synced {
    let store = store().await;
    let stored = store.provider.trigger_instance_repo();
    let stored_clips = store.provider.soundboard_clips_repo();
    let (client, backend) = recording_client();
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        Arc::clone(&stored),
        Arc::clone(&stored_clips),
    );
    let repo = HotkeySyncedTriggerRepo::wrap(Arc::clone(&stored), Some(Arc::clone(&reconciler)));
    let clips =
        HotkeySyncedClipsRepo::wrap(Arc::clone(&stored_clips), Some(Arc::clone(&reconciler)));
    Synced {
        store,
        stored,
        stored_clips,
        repo,
        clips,
        reconciler,
        client,
        backend,
    }
}

struct ListGate {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

struct ProbeClips {
    clips: std::sync::Mutex<Vec<StoredClip>>,
    unreadable: AtomicBool,
    gate: std::sync::Mutex<Option<ListGate>>,
}

impl ProbeClips {
    fn holding(clips: Vec<StoredClip>, gate: Option<ListGate>) -> Arc<Self> {
        Arc::new(Self {
            clips: std::sync::Mutex::new(clips),
            unreadable: AtomicBool::new(false),
            gate: std::sync::Mutex::new(gate),
        })
    }

    fn add(&self, clip: StoredClip) {
        self.clips.lock().unwrap().push(clip);
    }
}

#[async_trait]
impl SoundboardClipsRepo for ProbeClips {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        if self.unreadable.load(Ordering::SeqCst) {
            return Err(StorageError::Connection {
                reason: "disk gone".to_owned(),
            });
        }
        let listed = self.clips.lock().unwrap().clone();
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        Ok(listed)
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        Ok(self
            .clips
            .lock()
            .unwrap()
            .iter()
            .find(|clip| clip.id == id)
            .cloned())
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        self.add(clip.clone());
        Ok(())
    }

    async fn delete(&self, _id: ClipId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

fn triggers_listing(instances: Vec<forge_types::TriggerInstance>) -> Arc<dyn TriggerInstanceRepo> {
    let mut repo = MockTriggerInstanceRepo::new();
    repo.expect_list_all()
        .returning(move || Ok(instances.clone()));
    Arc::new(repo)
}

#[tokio::test]
async fn saving_a_hotkey_trigger_registers_its_combo_in_canonical_form() {
    let synced = synced().await;

    synced
        .repo
        .save(&instance(
            event_kinds::GLOBAL_PRESSED,
            Some("shift+ctrl+f5"),
        ))
        .await
        .unwrap();

    assert_eq!(registered(&synced.client), ["Ctrl+Shift+F5"]);
}

#[tokio::test]
async fn changing_a_triggers_combo_moves_the_registration_to_the_new_combo() {
    let synced = synced().await;
    let original = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&original).await.unwrap();

    synced.repo.save(&with_combo(original, "F6")).await.unwrap();

    assert_eq!(registered(&synced.client), ["F6"]);
}

#[tokio::test]
async fn removing_the_last_trigger_on_a_combo_releases_its_registration() {
    for removal in ["delete", "archive"] {
        let synced = synced().await;
        let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
        synced.repo.save(&trigger).await.unwrap();

        let removed = match removal {
            "delete" => synced.repo.delete(trigger.id).await,
            _ => synced.repo.archive(trigger.id).await,
        };

        assert!(removed.unwrap(), "{removal} reports the row removed");
        assert!(registered(&synced.client).is_empty(), "after {removal}");
    }
}

#[tokio::test]
async fn restoring_an_archived_hotkey_trigger_registers_its_combo_again() {
    let synced = synced().await;
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&trigger).await.unwrap();
    synced.repo.archive(trigger.id).await.unwrap();

    synced.repo.restore(trigger.id).await.unwrap();

    assert_eq!(registered(&synced.client), ["F5"]);
}

#[tokio::test]
async fn removing_one_half_of_a_hold_keeps_the_registration_the_other_half_uses() {
    let synced = synced().await;
    let press = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&press).await.unwrap();
    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_RELEASED, Some("f5")))
        .await
        .unwrap();

    synced.repo.delete(press.id).await.unwrap();

    let released = synced
        .backend
        .calls()
        .into_iter()
        .any(|call| matches!(call, RecordedCall::Unregister(_)));
    assert!(!released, "the release half still needs the OS grab");
    assert_eq!(registered(&synced.client), ["F5"]);
}

#[tokio::test]
async fn disabling_a_hotkey_trigger_releases_its_registration_until_it_is_enabled_again() {
    let synced = synced().await;
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&trigger).await.unwrap();

    synced.repo.set_enabled(trigger.id, false).await.unwrap();
    let while_disabled = registered(&synced.client);
    synced.repo.set_enabled(trigger.id, true).await.unwrap();

    assert!(while_disabled.is_empty(), "got {while_disabled:?}");
    assert_eq!(registered(&synced.client), ["F5"]);
}

#[tokio::test]
async fn disabling_both_halves_of_a_hold_unregisters_the_combo_once() {
    let synced = synced().await;
    let press = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    let release = instance(event_kinds::GLOBAL_RELEASED, Some("F5"));
    synced.repo.save(&press).await.unwrap();
    synced.repo.save(&release).await.unwrap();

    synced.repo.set_enabled(press.id, false).await.unwrap();
    let after_one_half = registered(&synced.client);
    synced.repo.set_enabled(release.id, false).await.unwrap();

    let unregisters = synced
        .backend
        .calls()
        .into_iter()
        .filter(|call| matches!(call, RecordedCall::Unregister(_)))
        .count();
    assert_eq!(
        after_one_half,
        ["F5"],
        "the other half still needs the grab"
    );
    assert_eq!(unregisters, 1);
    assert!(registered(&synced.client).is_empty());
}

#[tokio::test]
async fn disabling_a_trigger_keeps_the_combo_a_clip_is_bound_to() {
    let synced = synced().await;
    synced.clips.save(&clip(Some("F5"))).await.unwrap();
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&trigger).await.unwrap();

    synced.repo.set_enabled(trigger.id, false).await.unwrap();

    assert_eq!(registered(&synced.client), ["F5"]);
}

#[tokio::test]
async fn applying_changes_stores_combos_canonically_and_registers_them() {
    let synced = synced().await;
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("shift+ctrl+f5"));

    synced
        .repo
        .apply_changes(&TriggerChanges {
            saved: vec![trigger.clone()],
            ..TriggerChanges::default()
        })
        .await
        .unwrap();

    let stored = synced.stored.get(trigger.id).await.unwrap().unwrap();
    assert_eq!(stored_combo(&stored), Some("Ctrl+Shift+F5"));
    assert_eq!(registered(&synced.client), ["Ctrl+Shift+F5"]);
}

#[tokio::test]
async fn a_refused_write_leaves_the_client_registrations_untouched() {
    let unparseable = instance(event_kinds::GLOBAL_PRESSED, Some("Ctrl+XYZ"));
    let valid = instance(event_kinds::GLOBAL_PRESSED, Some("F9"));
    let failure = || StorageError::Connection {
        reason: "disk gone".to_owned(),
    };
    for write in [
        "invalid combo",
        "invalid combo in a batch",
        "save",
        "batch",
        "delete",
        "archive",
        "restore",
        "set enabled",
    ] {
        let mut inner = MockTriggerInstanceRepo::new();
        let listed = valid.clone();
        inner
            .expect_list_all()
            .returning(move || Ok(vec![listed.clone()]));
        inner.expect_save().returning(move |_| Err(failure()));
        inner.expect_delete().returning(move |_| Err(failure()));
        inner.expect_archive().returning(move |_| Err(failure()));
        inner.expect_restore().returning(move |_| Err(failure()));
        inner
            .expect_apply_changes()
            .returning(move |_| Err(failure()));
        inner
            .expect_set_enabled()
            .returning(move |_, _| Err(failure()));
        let (client, backend) = recording_client();
        let inner: Arc<dyn TriggerInstanceRepo> = Arc::new(inner);
        let reconciler = HotkeyReconciler::new(
            client,
            Arc::clone(&inner),
            Arc::new(MockSoundboardClipsRepo::new()),
        );
        let repo = HotkeySyncedTriggerRepo::wrap(inner, Some(reconciler));

        let batch_of = |instance: &forge_types::TriggerInstance| TriggerChanges {
            saved: vec![instance.clone()],
            ..TriggerChanges::default()
        };
        let outcome = match write {
            "invalid combo" => repo.save(&unparseable).await.map(|_| ()),
            "invalid combo in a batch" => repo.apply_changes(&batch_of(&unparseable)).await,
            "save" => repo.save(&valid).await.map(|_| ()),
            "batch" => repo.apply_changes(&batch_of(&valid)).await,
            "set enabled" => repo.set_enabled(valid.id, false).await,
            "delete" => repo.delete(valid.id).await.map(|_| ()),
            "archive" => repo.archive(valid.id).await.map(|_| ()),
            _ => repo.restore(valid.id).await.map(|_| ()),
        };

        assert!(outcome.is_err(), "{write} must report the failure");
        assert!(backend.calls().is_empty(), "{write} touched the client");
    }
}

#[tokio::test]
async fn a_combo_the_os_refuses_is_still_stored_but_left_unregistered() {
    let synced = synced().await;
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("f7"));
    synced
        .backend
        .fail_next_register(&HotkeyCombo::parse("F7").unwrap());

    let saved = synced.repo.save(&trigger).await;

    assert!(saved.is_ok(), "got {saved:?}");
    let stored = synced.stored.get(trigger.id).await.unwrap().unwrap();
    assert_eq!(stored_combo(&stored), Some("F7"));
    assert!(registered(&synced.client).is_empty());
}

#[tokio::test]
async fn the_test_client_answers_a_disable_enable_cycle_with_real_backend_calls() {
    let (client, backend) = recording_client();
    let combo = HotkeyCombo::parse("F7").unwrap();
    let id = client.register(combo.clone()).await.unwrap();

    client.disable().await.unwrap();
    client.enable().await.unwrap();

    assert_eq!(
        backend.calls(),
        [
            RecordedCall::Register(id, combo.clone()),
            RecordedCall::Unregister(id),
            RecordedCall::Register(id, combo),
        ]
    );
}

#[tokio::test]
async fn a_combo_refused_on_enable_is_registered_again_by_the_next_reconcile() {
    let synced = synced().await;
    let combo = HotkeyCombo::parse("F7").unwrap();
    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F7")))
        .await
        .unwrap();
    synced.client.disable().await.unwrap();

    synced.backend.fail_next_register(&combo);
    let failures = synced.client.enable().await.unwrap();
    let after_refusal = registered(&synced.client);
    synced.reconciler.reconcile().await;

    assert_eq!(failures.len(), 1);
    assert!(after_refusal.is_empty(), "got {after_refusal:?}");
    assert_eq!(registered(&synced.client), ["F7"]);
}

#[tokio::test]
async fn a_refused_combo_whose_trigger_is_gone_is_not_taken_back_on_enable() {
    let synced = synced().await;
    let combo = HotkeyCombo::parse("F7").unwrap();
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F7"));
    synced.repo.save(&trigger).await.unwrap();
    synced.client.disable().await.unwrap();
    synced.backend.fail_next_register(&combo);
    synced.client.enable().await.unwrap();

    synced.repo.delete(trigger.id).await.unwrap();
    synced.client.enable().await.unwrap();

    assert!(registered(&synced.client).is_empty());
}

#[tokio::test]
async fn a_combo_the_os_refused_is_registered_by_the_next_trigger_write() {
    let synced = synced().await;
    synced
        .backend
        .fail_next_register(&HotkeyCombo::parse("F7").unwrap());
    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F7")))
        .await
        .unwrap();

    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F8")))
        .await
        .unwrap();

    assert_eq!(registered(&synced.client), ["F7", "F8"]);
}

#[tokio::test]
async fn a_trigger_write_leaves_the_combos_clips_are_bound_to_registered() {
    let synced = synced().await;
    synced.clips.save(&clip(Some("F9"))).await.unwrap();
    let trigger = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&trigger).await.unwrap();

    synced.repo.delete(trigger.id).await.unwrap();

    assert_eq!(registered(&synced.client), ["F9"]);
}

#[tokio::test]
async fn a_clip_write_leaves_the_combos_triggers_are_bound_to_registered() {
    let synced = synced().await;
    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F5")))
        .await
        .unwrap();
    let bound = clip(Some("F9"));
    synced.clips.save(&bound).await.unwrap();

    synced.clips.delete(bound.id).await.unwrap();

    assert_eq!(registered(&synced.client), ["F5"]);
}

#[tokio::test]
async fn reconcile_registers_the_union_of_stored_trigger_and_clip_combos() {
    let synced = synced().await;
    synced
        .stored
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F5")))
        .await
        .unwrap();
    for combo in ["f5", "shift+f6"] {
        synced.stored_clips.save(&clip(Some(combo))).await.unwrap();
    }

    synced.reconciler.reconcile().await;

    assert_eq!(registered(&synced.client), ["F5", "Shift+F6"]);
}

#[tokio::test]
async fn saving_a_clip_registers_its_combo_in_canonical_form() {
    let synced = synced().await;

    synced
        .clips
        .save(&clip(Some("shift+ctrl+f9")))
        .await
        .unwrap();

    assert_eq!(registered(&synced.client), ["Ctrl+Shift+F9"]);
}

#[tokio::test]
async fn changing_a_clips_combo_moves_the_registration_to_the_new_combo() {
    let synced = synced().await;
    let original = clip(Some("F9"));
    synced.clips.save(&original).await.unwrap();

    let moved = StoredClip {
        hotkey: Some("F10".to_owned()),
        ..original
    };
    synced.clips.save(&moved).await.unwrap();

    assert_eq!(registered(&synced.client), ["F10"]);
}

#[tokio::test]
async fn a_clip_that_loses_its_combo_releases_the_registration() {
    for removal in ["delete", "blank", "none"] {
        let synced = synced().await;
        let bound = clip(Some("F9"));
        synced.clips.save(&bound).await.unwrap();

        match removal {
            "delete" => {
                synced.clips.delete(bound.id).await.unwrap();
            }
            "blank" => {
                let blanked = StoredClip {
                    hotkey: Some("   ".to_owned()),
                    ..bound
                };
                synced.clips.save(&blanked).await.unwrap();
            }
            _ => {
                let cleared = StoredClip {
                    hotkey: None,
                    ..bound
                };
                synced.clips.save(&cleared).await.unwrap();
            }
        }

        assert!(registered(&synced.client).is_empty(), "after {removal}");
    }
}

#[tokio::test]
async fn a_clip_is_stored_with_its_combo_canonical_and_a_blank_combo_as_no_binding() {
    let synced = synced().await;
    for (typed, expected) in [
        (Some("shift+ctrl+f5"), Some("Ctrl+Shift+F5")),
        (Some(" F5 "), Some("F5")),
        (Some(""), None),
        (Some("   "), None),
        (None, None),
    ] {
        let saved = clip(typed);

        synced.clips.save(&saved).await.unwrap();

        let stored = synced.stored_clips.get(saved.id).await.unwrap().unwrap();
        assert_eq!(stored.hotkey.as_deref(), expected, "typed {typed:?}");
    }
}

#[tokio::test]
async fn a_clip_with_an_unparseable_combo_is_refused_unstored_and_leaves_the_client_alone() {
    let synced = synced().await;
    let refused = clip(Some("Ctrl+XYZ"));

    let outcome = synced.clips.save(&refused).await;

    assert!(
        matches!(outcome, Err(StorageError::ValidationFailed { ref field, .. }) if field == CLIP_COMBO_FIELD),
        "got {outcome:?}"
    );
    assert!(synced.stored_clips.get(refused.id).await.unwrap().is_none());
    assert!(synced.backend.calls().is_empty());
}

#[tokio::test]
async fn a_clip_write_publishes_the_combo_to_clip_snapshot_the_dispatcher_reads() {
    let synced = synced().await;
    let snapshot = synced.reconciler.clip_bindings();
    let bound = clip(Some("f9"));

    synced.clips.save(&bound).await.unwrap();

    assert_eq!(snapshot.current().get("F9"), Some(&vec![bound.id]));
}

#[tokio::test]
async fn an_unreadable_clip_table_keeps_the_clip_combos_registered() {
    let probe = ProbeClips::holding(vec![clip(Some("F9"))], None);
    let (client, _backend) = recording_client();
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        triggers_listing(Vec::new()),
        Arc::clone(&probe) as Arc<dyn SoundboardClipsRepo>,
    );
    reconciler.reconcile().await;
    probe.unreadable.store(true, Ordering::SeqCst);

    reconciler.reconcile().await;

    assert_eq!(registered(&client), ["F9"]);
}

#[tokio::test(start_paused = true)]
async fn a_reconcile_waits_for_the_one_in_flight_so_a_stale_snapshot_never_lands_last() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let probe = ProbeClips::holding(
        Vec::new(),
        Some(ListGate {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        }),
    );
    let (client, _backend) = recording_client();
    let reconciler = HotkeyReconciler::new(
        Arc::clone(&client),
        triggers_listing(vec![instance(event_kinds::GLOBAL_PRESSED, Some("F5"))]),
        Arc::clone(&probe) as Arc<dyn SoundboardClipsRepo>,
    );
    let stale = tokio::spawn({
        let reconciler = Arc::clone(&reconciler);
        async move { reconciler.reconcile().await }
    });
    entered.notified().await;
    probe.add(clip(Some("F9")));

    let mut fresh = tokio::spawn({
        let reconciler = Arc::clone(&reconciler);
        async move { reconciler.reconcile().await }
    });
    let overtook = tokio::time::timeout(STILL_WAITING, &mut fresh)
        .await
        .is_ok();
    release.notify_one();
    stale.await.unwrap();
    if !overtook {
        fresh.await.unwrap();
    }

    assert!(
        !overtook,
        "a second run finished while the first was mid-diff"
    );
    assert_eq!(registered(&client), ["F5", "F9"]);
}

#[tokio::test]
async fn only_a_binding_added_after_boot_switches_the_disabled_engine_on() {
    let synced = synced().await;
    let starts = Arc::new(AtomicUsize::new(0));
    synced.reconciler.enable_engine_on_new_bindings({
        let starts = Arc::clone(&starts);
        move || {
            starts.fetch_add(1, Ordering::SeqCst);
        }
    });
    synced
        .stored
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F5")))
        .await
        .unwrap();

    synced.reconciler.reconcile().await;
    synced.reconciler.reconcile().await;
    let started_at_boot = starts.load(Ordering::SeqCst);
    synced
        .repo
        .save(&instance(event_kinds::GLOBAL_PRESSED, Some("F6")))
        .await
        .unwrap();
    let started_on_new_binding = starts.load(Ordering::SeqCst);

    assert_eq!(
        started_at_boot, 0,
        "existing bindings must not override the stored choice"
    );
    assert_eq!(
        started_on_new_binding, 1,
        "a new binding must switch the engine on"
    );
}

#[tokio::test]
async fn re_enabling_a_binding_leaves_a_stored_engine_off_choice_alone() {
    let synced = synced().await;
    let starts = Arc::new(AtomicUsize::new(0));
    synced.reconciler.enable_engine_on_new_bindings({
        let starts = Arc::clone(&starts);
        move || {
            starts.fetch_add(1, Ordering::SeqCst);
        }
    });
    let binding = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.stored.save(&binding).await.unwrap();
    synced.reconciler.reconcile().await;

    synced.repo.set_enabled(binding.id, false).await.unwrap();
    synced.repo.set_enabled(binding.id, true).await.unwrap();

    assert_eq!(starts.load(Ordering::SeqCst), 0);
}

fn new_queue() -> Queue {
    Queue {
        id: QueueId::new(),
        name: "alerts".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    }
}

impl Synced {
    async fn wiring(&self, combo: &str) -> TriggerWiring {
        let backend = &self.store.provider;
        let action_id = seed_action(backend, "Play sound").await;
        let action = backend.action_repo().get(action_id).await.unwrap().unwrap();
        TriggerWiring {
            created_queue: None,
            action,
            instance: instance(event_kinds::GLOBAL_PRESSED, Some(combo)),
        }
    }

    async fn commit_wiring(&self, wiring: &TriggerWiring) -> Result<(), StorageError> {
        let backend = &self.store.provider;
        self.repo
            .commit_overlay_wiring(wiring, backend.queue_repo(), backend.action_repo())
            .await
    }

    async fn stored_blob(&self) -> MediaBlobId {
        self.store
            .provider
            .media_repo()
            .store("ding.ogg", b"OggS\x00\x02\x00\x00synced clip".to_vec())
            .await
            .unwrap()
            .id
    }
}

#[tokio::test]
async fn an_overlay_wiring_through_the_synced_repo_registers_its_combo_in_canonical_form() {
    let synced = synced().await;
    let wiring = synced.wiring("shift+ctrl+f5").await;

    synced.commit_wiring(&wiring).await.unwrap();

    assert_eq!(registered(&synced.client), ["Ctrl+Shift+F5"]);
}

#[tokio::test]
async fn an_overlay_wiring_through_the_synced_repo_stays_one_transaction() {
    let synced = synced().await;
    let queue = new_queue();
    let mut wiring = synced.wiring("F5").await;
    wiring.action.queue_id = QueueId::new();
    wiring.created_queue = Some(queue.clone());

    let failed = synced.commit_wiring(&wiring).await;

    let kept = synced.store.provider.queue_repo().get(queue.id).await;
    assert!(failed.is_err());
    assert_eq!(kept.unwrap(), None);
}

#[tokio::test]
async fn disabling_a_list_through_the_synced_repo_releases_each_registration() {
    let synced = synced().await;
    let first = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    let second = instance(event_kinds::GLOBAL_PRESSED, Some("F6"));
    synced.repo.save(&first).await.unwrap();
    synced.repo.save(&second).await.unwrap();

    synced
        .repo
        .set_all_enabled(&[first.id, second.id], false)
        .await
        .unwrap();

    assert!(registered(&synced.client).is_empty());
}

#[tokio::test]
async fn a_list_holding_an_unknown_id_through_the_synced_repo_toggles_nothing() {
    let synced = synced().await;
    let known = instance(event_kinds::GLOBAL_PRESSED, Some("F5"));
    synced.repo.save(&known).await.unwrap();

    let failed = synced
        .repo
        .set_all_enabled(&[known.id, TriggerInstanceId::new()], false)
        .await;

    assert!(failed.is_err());
    assert_eq!(registered(&synced.client), ["F5"]);
    assert!(synced.stored.get(known.id).await.unwrap().unwrap().enabled);
}

#[tokio::test]
async fn saving_a_clip_with_its_source_through_the_synced_repo_registers_its_combo() {
    let synced = synced().await;
    let blob = synced.stored_blob().await;
    let clip = clip(Some("shift+ctrl+f5"));

    synced
        .clips
        .save_with_source(&clip, &blob, synced.store.provider.media_repo())
        .await
        .unwrap();

    assert_eq!(registered(&synced.client), ["Ctrl+Shift+F5"]);
}

#[tokio::test]
async fn saving_a_clip_with_a_missing_source_through_the_synced_repo_stays_one_transaction() {
    let synced = synced().await;
    let clip = clip(Some("F5"));
    let unknown = MediaBlobId::from_stored(format!("sha256-{}", "ab".repeat(32))).unwrap();

    let failed = synced
        .clips
        .save_with_source(&clip, &unknown, synced.store.provider.media_repo())
        .await;

    assert!(failed.is_err());
    assert_eq!(synced.stored_clips.get(clip.id).await.unwrap(), None);
}

#[tokio::test]
async fn deleting_a_clip_with_its_sources_through_the_synced_repo_reaches_the_inner_repo_whole() {
    let id = ClipId::new();
    let mut inner = MockSoundboardClipsRepo::new();
    inner
        .expect_delete_with_sources()
        .times(1)
        .returning(|_, _| Ok(true));
    let wrapped = HotkeySyncedClipsRepo::wrap(Arc::new(inner), None);
    let store = store().await;

    let deleted = wrapped
        .delete_with_sources(id, store.provider.media_repo())
        .await
        .unwrap();

    assert!(deleted);
}

#[tokio::test]
async fn deleting_a_clip_with_its_sources_through_the_synced_repo_releases_its_combo() {
    let synced = synced().await;
    let blob = synced.stored_blob().await;
    let clip = clip(Some("F5"));
    let media = synced.store.provider.media_repo();
    synced
        .clips
        .save_with_source(&clip, &blob, media.clone())
        .await
        .unwrap();

    synced
        .clips
        .delete_with_sources(clip.id, media)
        .await
        .unwrap();

    assert!(registered(&synced.client).is_empty());
}
