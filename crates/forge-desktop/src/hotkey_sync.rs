use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use forge_hotkey::{HotkeyClient, HotkeyCombo, HotkeyError};
use forge_storage::{SoundboardClipsRepo, StorageError, TriggerInstanceRepo};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId, Variant};
use tokio::sync::{Mutex, watch};

use crate::clip_hotkeys::{ClipBindings, clip_bindings_of};
use crate::hotkey_bindings::{COMBO_FIELD, HotkeyEdge, persisted_hotkey_combos};

/// The only owner of OS hotkey registrations; runs are serialized so concurrent writes never interleave diffs.
pub struct HotkeyReconciler {
    client: Arc<HotkeyClient>,
    triggers: Arc<dyn TriggerInstanceRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
    serial: Mutex<()>,
    clip_bindings: watch::Sender<Arc<ClipBindings>>,
}

impl HotkeyReconciler {
    pub fn new(
        client: Arc<HotkeyClient>,
        triggers: Arc<dyn TriggerInstanceRepo>,
        clips: Arc<dyn SoundboardClipsRepo>,
    ) -> Arc<Self> {
        let (clip_bindings, _) = watch::channel(Arc::new(ClipBindings::new()));
        Arc::new(Self {
            client,
            triggers,
            clips,
            serial: Mutex::new(()),
            clip_bindings,
        })
    }

    pub fn client(&self) -> &Arc<HotkeyClient> {
        &self.client
    }

    pub fn clip_bindings(&self) -> watch::Receiver<Arc<ClipBindings>> {
        self.clip_bindings.subscribe()
    }

    /// An unreadable source leaves every registration as it is rather than releasing what it would keep.
    pub async fn reconcile(&self) {
        let _serial = self.serial.lock().await;
        let instances = match self.triggers.list_all().await {
            Ok(instances) => instances,
            Err(e) => {
                tracing::warn!(error = %e, "could not load hotkey triggers to sync registrations");
                return;
            }
        };
        let clips = match self.clips.list().await {
            Ok(clips) => clips,
            Err(e) => {
                tracing::warn!(error = %e, "could not load clip hotkeys to sync registrations");
                return;
            }
        };
        let bindings = clip_bindings_of(&clips);
        let mut wanted = persisted_hotkey_combos(&instances);
        wanted.extend(bindings.keys().cloned());
        self.clip_bindings.send_replace(Arc::new(bindings));
        apply_wanted(&self.client, &wanted).await;
    }

    /// Registers ahead of the write, so a combo the OS refuses fails the write instead of being stored unregistered.
    pub async fn claim(&self, combo: HotkeyCombo) -> Result<(), HotkeyError> {
        let _serial = self.serial.lock().await;
        if self
            .client
            .registered_combos()
            .iter()
            .any(|(_, known)| known.as_str() == combo.as_str())
        {
            return Ok(());
        }
        self.client.register(combo).await.map(|_| ())
    }
}

async fn apply_wanted(client: &HotkeyClient, wanted: &BTreeSet<String>) {
    let registered = client.registered_combos();

    for (id, combo) in &registered {
        if !wanted.contains(combo.as_str())
            && let Err(e) = client.unregister(*id).await
        {
            tracing::warn!(combo = %combo, error = %e, "could not release a hotkey nothing binds");
        }
    }

    for raw in wanted.iter().filter(|raw| !raw.is_empty()) {
        if registered.iter().any(|(_, known)| known.as_str() == raw) {
            continue;
        }
        let outcome = match HotkeyCombo::parse(raw) {
            Ok(combo) => client.register(combo).await.map(|_| ()),
            Err(e) => Err(e),
        };
        if let Err(e) = outcome {
            tracing::warn!(combo = %raw, error = %e, "could not register a stored hotkey binding");
        }
    }
}

/// Keeps the OS hotkey registrations in step with the stored hotkey triggers for screens that
/// edit trigger instances generically; hotkey combos are canonicalised on the way in.
pub struct HotkeySyncedTriggerRepo {
    inner: Arc<dyn TriggerInstanceRepo>,
    reconciler: Arc<HotkeyReconciler>,
}

impl HotkeySyncedTriggerRepo {
    pub fn wrap(
        inner: Arc<dyn TriggerInstanceRepo>,
        reconciler: Option<Arc<HotkeyReconciler>>,
    ) -> Arc<dyn TriggerInstanceRepo> {
        match reconciler {
            Some(reconciler) => Arc::new(Self { inner, reconciler }),
            None => inner,
        }
    }

    async fn sync(&self) {
        self.reconciler.reconcile().await;
    }
}

fn canonicalized(instance: &TriggerInstance) -> Result<TriggerInstance, StorageError> {
    let mut canonical = instance.clone();
    if HotkeyEdge::from_kind(&instance.kind_id).is_none() {
        return Ok(canonical);
    }
    if let Some(Variant::String(raw)) = instance.overrides.get(COMBO_FIELD)
        && !raw.trim().is_empty()
    {
        let combo = HotkeyCombo::parse(raw).map_err(|e| StorageError::ValidationFailed {
            field: COMBO_FIELD.to_owned(),
            reason: e.to_string(),
        })?;
        canonical.overrides.insert(
            COMBO_FIELD.to_owned(),
            Variant::String(combo.as_str().to_owned()),
        );
    }
    Ok(canonical)
}

#[async_trait]
impl TriggerInstanceRepo for HotkeySyncedTriggerRepo {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_all().await
    }

    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_user_defined().await
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_for_action(action_id).await
    }

    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError> {
        self.inner.actions_using(instance_id).await
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError> {
        self.inner
            .link_action(action_id, instance_id, position)
            .await
    }

    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        self.inner.unlink_action(action_id, instance_id).await
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        self.inner.get(id).await
    }

    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        let canonical = canonicalized(instance)?;
        self.inner.save(&canonical).await?;
        self.sync().await;
        Ok(())
    }

    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let deleted = self.inner.delete(id).await?;
        self.sync().await;
        Ok(deleted)
    }

    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        self.inner.upsert_default(kind_id, name).await
    }

    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError> {
        self.inner.set_enabled(id, enabled).await
    }

    async fn archive(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let archived = self.inner.archive(id).await?;
        self.sync().await;
        Ok(archived)
    }

    async fn restore(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let restored = self.inner.restore(id).await?;
        self.sync().await;
        Ok(restored)
    }

    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        self.inner.list_archived().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use forge_events::{Event, EventPublisher};
    use forge_hotkey::HotkeyConfig;
    use forge_hotkey::testing::{RecordedCall, RecordingBackend, test_client};
    use forge_soundboard::ClipLibrary;
    use forge_storage::soundboard::MockSoundboardClipsRepo;
    use forge_storage::{DataProvider, MockTriggerInstanceRepo, StoredClip};
    use forge_types::{ClipId, OutputDevice, PermissionRung, PlatformScope};
    use time::OffsetDateTime;
    use tokio::sync::Notify;

    use super::*;
    use crate::clip_hotkeys::HotkeySyncedClipsRepo;
    use crate::hotkey_bindings::{HOTKEY_PRESSED_KIND, HOTKEY_RELEASED_KIND};
    use crate::test_support::{Sandboxed, sandboxed_backend};

    const OTHER_KIND: &str = "midi.note_on";
    const TEST_KEY: [u8; 32] = [0x11; 32];

    struct SilentPublisher;

    impl EventPublisher for SilentPublisher {
        fn publish(&self, _: Event) {}
    }

    fn recording_client() -> (Arc<HotkeyClient>, Arc<RecordingBackend>) {
        test_client(HotkeyConfig::default(), Arc::new(SilentPublisher))
    }

    const EVENT_DEADLINE: Duration = Duration::from_secs(2);
    const STILL_WAITING: Duration = Duration::from_millis(50);

    struct Synced {
        storage: Sandboxed<Arc<dyn DataProvider>>,
        stored: Arc<dyn TriggerInstanceRepo>,
        stored_clips: Arc<dyn SoundboardClipsRepo>,
        repo: Arc<dyn TriggerInstanceRepo>,
        clips: Arc<dyn SoundboardClipsRepo>,
        reconciler: Arc<HotkeyReconciler>,
        client: Arc<HotkeyClient>,
        backend: Arc<RecordingBackend>,
    }

    async fn storage() -> Sandboxed<Arc<dyn DataProvider>> {
        sandboxed_backend("sqlite::memory:", TEST_KEY)
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
    }

    async fn synced() -> Synced {
        let storage = storage().await;
        let stored = storage_repo(&storage);
        let stored_clips = clips_repo(&storage);
        let (client, backend) = recording_client();
        let reconciler = HotkeyReconciler::new(
            Arc::clone(&client),
            Arc::clone(&stored),
            Arc::clone(&stored_clips),
        );
        let repo =
            HotkeySyncedTriggerRepo::wrap(Arc::clone(&stored), Some(Arc::clone(&reconciler)));
        let clips =
            HotkeySyncedClipsRepo::wrap(Arc::clone(&stored_clips), Some(Arc::clone(&reconciler)));
        Synced {
            storage,
            stored,
            stored_clips,
            repo,
            clips,
            reconciler,
            client,
            backend,
        }
    }

    fn clips_repo(storage: &Sandboxed<Arc<dyn DataProvider>>) -> Arc<dyn SoundboardClipsRepo> {
        let provider: &Arc<dyn DataProvider> = storage;
        provider.soundboard_clips_repo()
    }

    fn clip(hotkey: Option<&str>) -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: "airhorn".to_owned(),
            file_path: "/clips/airhorn.wav".into(),
            volume: 1.0,
            output_device: OutputDevice::Default,
            hotkey: hotkey.map(str::to_owned),
            created_at: OffsetDateTime::now_utc(),
            category: String::new(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    fn storage_repo(storage: &Sandboxed<Arc<dyn DataProvider>>) -> Arc<dyn TriggerInstanceRepo> {
        let provider: &Arc<dyn DataProvider> = storage;
        provider.trigger_instance_repo()
    }

    fn registered(client: &HotkeyClient) -> Vec<String> {
        let mut combos: Vec<String> = client
            .registered_combos()
            .into_iter()
            .map(|(_, combo)| combo.as_str().to_owned())
            .collect();
        combos.sort();
        combos
    }

    fn with_combo(mut instance: TriggerInstance, combo: &str) -> TriggerInstance {
        instance
            .overrides
            .insert(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()));
        instance
    }

    fn instance(kind_id: &str, combo: Option<&str>) -> TriggerInstance {
        let overrides = combo
            .map(|combo| {
                BTreeMap::from([(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()))])
            })
            .unwrap_or_default();
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind_id.to_owned(),
            name: "trigger".to_owned(),
            overrides,
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn stored_combo(instance: &TriggerInstance) -> Option<&str> {
        match instance.overrides.get(COMBO_FIELD) {
            Some(Variant::String(combo)) => Some(combo),
            _ => None,
        }
    }

    #[test]
    fn a_hotkey_trigger_is_saved_with_its_combo_in_canonical_form() {
        for kind in [HOTKEY_PRESSED_KIND, HOTKEY_RELEASED_KIND] {
            let saved = canonicalized(&instance(kind, Some("shift+ctrl+f5"))).unwrap();

            assert_eq!(stored_combo(&saved), Some("Ctrl+Shift+F5"), "kind {kind}");
        }
    }

    #[test]
    fn an_unparseable_hotkey_combo_is_refused_as_a_validation_failure_on_the_combo_field() {
        let refused = canonicalized(&instance(HOTKEY_PRESSED_KIND, Some("Ctrl+XYZ")));

        assert!(
            matches!(refused, Err(StorageError::ValidationFailed { ref field, .. }) if field == COMBO_FIELD),
            "got {refused:?}"
        );
    }

    #[test]
    fn a_combo_is_left_untouched_when_the_trigger_is_not_a_hotkey_or_has_no_combo_yet() {
        for (kind, combo) in [
            (OTHER_KIND, Some("f5")),
            (OTHER_KIND, Some("Ctrl+XYZ")),
            (HOTKEY_PRESSED_KIND, Some("")),
            (HOTKEY_PRESSED_KIND, Some("   ")),
            (HOTKEY_PRESSED_KIND, None),
        ] {
            let original = instance(kind, combo);

            let saved = canonicalized(&original).unwrap();

            assert_eq!(saved, original, "kind {kind} with combo {combo:?}");
        }
    }

    #[tokio::test]
    async fn saving_a_hotkey_trigger_registers_its_combo_in_canonical_form() {
        let synced = synced().await;

        synced
            .repo
            .save(&instance(HOTKEY_PRESSED_KIND, Some("shift+ctrl+f5")))
            .await
            .unwrap();

        assert_eq!(registered(&synced.client), ["Ctrl+Shift+F5"]);
    }

    #[tokio::test]
    async fn changing_a_triggers_combo_moves_the_registration_to_the_new_combo() {
        let synced = synced().await;
        let original = instance(HOTKEY_PRESSED_KIND, Some("F5"));
        synced.repo.save(&original).await.unwrap();

        synced.repo.save(&with_combo(original, "F6")).await.unwrap();

        assert_eq!(registered(&synced.client), ["F6"]);
    }

    #[tokio::test]
    async fn removing_the_last_trigger_on_a_combo_releases_its_registration() {
        for removal in ["delete", "archive"] {
            let synced = synced().await;
            let trigger = instance(HOTKEY_PRESSED_KIND, Some("F5"));
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
        let trigger = instance(HOTKEY_PRESSED_KIND, Some("F5"));
        synced.repo.save(&trigger).await.unwrap();
        synced.repo.archive(trigger.id).await.unwrap();

        synced.repo.restore(trigger.id).await.unwrap();

        assert_eq!(registered(&synced.client), ["F5"]);
    }

    #[tokio::test]
    async fn removing_one_half_of_a_hold_keeps_the_registration_the_other_half_uses() {
        let synced = synced().await;
        let press = instance(HOTKEY_PRESSED_KIND, Some("F5"));
        synced.repo.save(&press).await.unwrap();
        synced
            .repo
            .save(&instance(HOTKEY_RELEASED_KIND, Some("f5")))
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
    async fn disabling_a_hotkey_trigger_keeps_its_registration() {
        let synced = synced().await;
        let trigger = instance(HOTKEY_PRESSED_KIND, Some("F5"));
        synced.repo.save(&trigger).await.unwrap();

        synced.repo.set_enabled(trigger.id, false).await.unwrap();

        assert_eq!(registered(&synced.client), ["F5"]);
    }

    #[tokio::test]
    async fn a_refused_write_leaves_the_client_registrations_untouched() {
        let unparseable = instance(HOTKEY_PRESSED_KIND, Some("Ctrl+XYZ"));
        let valid = instance(HOTKEY_PRESSED_KIND, Some("F9"));
        let failure = || StorageError::Connection {
            reason: "disk gone".to_owned(),
        };
        for write in ["invalid combo", "save", "delete", "archive", "restore"] {
            let mut inner = MockTriggerInstanceRepo::new();
            let listed = valid.clone();
            inner
                .expect_list_all()
                .returning(move || Ok(vec![listed.clone()]));
            inner.expect_save().returning(move |_| Err(failure()));
            inner.expect_delete().returning(move |_| Err(failure()));
            inner.expect_archive().returning(move |_| Err(failure()));
            inner.expect_restore().returning(move |_| Err(failure()));
            let (client, backend) = recording_client();
            let inner: Arc<dyn TriggerInstanceRepo> = Arc::new(inner);
            let reconciler = HotkeyReconciler::new(
                client,
                Arc::clone(&inner),
                Arc::new(MockSoundboardClipsRepo::new()),
            );
            let repo = HotkeySyncedTriggerRepo::wrap(inner, Some(reconciler));

            let outcome = match write {
                "invalid combo" => repo.save(&unparseable).await.map(|_| ()),
                "save" => repo.save(&valid).await.map(|_| ()),
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
        let trigger = instance(HOTKEY_PRESSED_KIND, Some("f7"));
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
    async fn a_combo_the_os_refused_is_registered_by_the_next_trigger_write() {
        let synced = synced().await;
        synced
            .backend
            .fail_next_register(&HotkeyCombo::parse("F7").unwrap());
        synced
            .repo
            .save(&instance(HOTKEY_PRESSED_KIND, Some("F7")))
            .await
            .unwrap();

        synced
            .repo
            .save(&instance(HOTKEY_PRESSED_KIND, Some("F8")))
            .await
            .unwrap();

        assert_eq!(registered(&synced.client), ["F7", "F8"]);
    }
    struct ListGate {
        entered: Arc<Notify>,
        release: Arc<Notify>,
    }

    struct ProbeClips {
        inner: Arc<dyn SoundboardClipsRepo>,
        unreadable: AtomicBool,
        gate: std::sync::Mutex<Option<ListGate>>,
    }

    impl ProbeClips {
        fn over(inner: Arc<dyn SoundboardClipsRepo>, gate: Option<ListGate>) -> Arc<Self> {
            Arc::new(Self {
                inner,
                unreadable: AtomicBool::new(false),
                gate: std::sync::Mutex::new(gate),
            })
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
            let listed = self.inner.list().await;
            let gate = self.gate.lock().unwrap().take();
            if let Some(gate) = gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
            listed
        }

        async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
            self.inner.get(id).await
        }

        async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
            self.inner.save(clip).await
        }

        async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
            self.inner.delete(id).await
        }
    }

    #[tokio::test]
    async fn a_trigger_write_leaves_the_combos_clips_are_bound_to_registered() {
        let synced = synced().await;
        synced.clips.save(&clip(Some("F9"))).await.unwrap();
        let trigger = instance(HOTKEY_PRESSED_KIND, Some("F5"));
        synced.repo.save(&trigger).await.unwrap();

        synced.repo.delete(trigger.id).await.unwrap();

        assert_eq!(registered(&synced.client), ["F9"]);
    }

    #[tokio::test]
    async fn a_clip_write_leaves_the_combos_triggers_are_bound_to_registered() {
        let synced = synced().await;
        synced
            .repo
            .save(&instance(HOTKEY_PRESSED_KIND, Some("F5")))
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
            .save(&instance(HOTKEY_PRESSED_KIND, Some("F5")))
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
    async fn a_clip_imported_through_the_library_registers_its_combo() {
        let synced = synced().await;
        let provider: &Arc<dyn DataProvider> = &synced.storage;
        let library = ClipLibrary::new(Arc::clone(&synced.clips), provider.media_repo());
        let source = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        std::fs::write(source.path(), b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0").unwrap();
        let imported = StoredClip {
            file_path: source.path().to_owned(),
            ..clip(Some("F9"))
        };

        library.save_clip(&imported).await.unwrap();

        assert_eq!(registered(&synced.client), ["F9"]);
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
            matches!(outcome, Err(StorageError::ValidationFailed { ref field, .. }) if field == crate::clip_hotkeys::CLIP_COMBO_FIELD),
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

        assert_eq!(snapshot.borrow().get("F9"), Some(&vec![bound.id]));
    }

    #[tokio::test]
    async fn an_unreadable_clip_table_keeps_the_clip_combos_registered() {
        let storage = storage().await;
        let stored_clips = clips_repo(&storage);
        stored_clips.save(&clip(Some("F9"))).await.unwrap();
        let probe = ProbeClips::over(stored_clips, None);
        let (client, _backend) = recording_client();
        let reconciler = HotkeyReconciler::new(
            Arc::clone(&client),
            storage_repo(&storage),
            Arc::clone(&probe) as Arc<dyn SoundboardClipsRepo>,
        );
        reconciler.reconcile().await;
        probe.unreadable.store(true, Ordering::SeqCst);

        reconciler.reconcile().await;

        assert_eq!(registered(&client), ["F9"]);
    }

    #[tokio::test]
    async fn a_reconcile_waits_for_the_one_in_flight_so_a_stale_snapshot_never_lands_last() {
        let storage = storage().await;
        let stored = storage_repo(&storage);
        let stored_clips = clips_repo(&storage);
        stored
            .save(&instance(HOTKEY_PRESSED_KIND, Some("F5")))
            .await
            .unwrap();
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let probe = ProbeClips::over(
            Arc::clone(&stored_clips),
            Some(ListGate {
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
            }),
        );
        let (client, _backend) = recording_client();
        let reconciler = HotkeyReconciler::new(Arc::clone(&client), stored, probe);
        let stale = tokio::spawn({
            let reconciler = Arc::clone(&reconciler);
            async move { reconciler.reconcile().await }
        });
        tokio::time::timeout(EVENT_DEADLINE, entered.notified())
            .await
            .unwrap();
        stored_clips.save(&clip(Some("F9"))).await.unwrap();

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
}
