use std::sync::Arc;

use async_trait::async_trait;
use forge_hotkey::{HotkeyClient, HotkeyCombo};
use forge_storage::{StorageError, TriggerInstanceRepo};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId, Variant};

use crate::hotkey_bindings::{COMBO_FIELD, HotkeyEdge, persisted_hotkey_combos};

/// Keeps the OS hotkey registrations in step with the stored hotkey triggers for screens that
/// edit trigger instances generically; hotkey combos are canonicalised on the way in.
pub struct HotkeySyncedTriggerRepo {
    inner: Arc<dyn TriggerInstanceRepo>,
    client: Arc<HotkeyClient>,
}

impl HotkeySyncedTriggerRepo {
    pub fn wrap(
        inner: Arc<dyn TriggerInstanceRepo>,
        client: Option<Arc<HotkeyClient>>,
    ) -> Arc<dyn TriggerInstanceRepo> {
        match client {
            Some(client) => Arc::new(Self { inner, client }),
            None => inner,
        }
    }

    async fn sync(&self) {
        sync_registrations(&self.client, self.inner.as_ref()).await;
    }
}

/// Registers every stored combo the client lacks and releases every registration no stored
/// trigger references any more; failures are logged and never undo the write that triggered it.
pub async fn sync_registrations(client: &HotkeyClient, repo: &dyn TriggerInstanceRepo) {
    let instances = match repo.list_all().await {
        Ok(instances) => instances,
        Err(e) => {
            tracing::warn!(error = %e, "could not load hotkey triggers to sync registrations");
            return;
        }
    };
    let wanted = persisted_hotkey_combos(&instances);
    let registered = client.registered_combos();

    for (id, combo) in &registered {
        if !wanted.contains(combo.as_str())
            && let Err(e) = client.unregister(*id).await
        {
            tracing::warn!(combo = %combo, error = %e, "could not release a hotkey no trigger uses");
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
            tracing::warn!(combo = %raw, error = %e, "could not register a stored hotkey trigger");
        }
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

    use forge_events::{Event, EventPublisher};
    use forge_hotkey::HotkeyConfig;
    use forge_hotkey::testing::{RecordedCall, RecordingBackend, test_client};
    use forge_storage::{DataProvider, MockTriggerInstanceRepo};
    use forge_types::{PermissionRung, PlatformScope};

    use super::*;
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

    struct Synced {
        _storage: Sandboxed<Arc<dyn DataProvider>>,
        stored: Arc<dyn TriggerInstanceRepo>,
        repo: Arc<dyn TriggerInstanceRepo>,
        client: Arc<HotkeyClient>,
        backend: Arc<RecordingBackend>,
    }

    async fn synced() -> Synced {
        let storage = sandboxed_backend("sqlite::memory:", TEST_KEY)
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let stored = storage_repo(&storage);
        let (client, backend) = recording_client();
        let repo = HotkeySyncedTriggerRepo::wrap(Arc::clone(&stored), Some(Arc::clone(&client)));
        Synced {
            _storage: storage,
            stored,
            repo,
            client,
            backend,
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
            let repo = HotkeySyncedTriggerRepo::wrap(Arc::new(inner), Some(Arc::clone(&client)));

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
}
