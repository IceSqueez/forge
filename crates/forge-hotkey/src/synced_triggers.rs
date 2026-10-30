use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use forge_storage::{
    ActionRepo, QueueRepo, StorageError, TriggerChanges, TriggerInstanceRepo, TriggerWiring,
};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId, Variant};

use crate::binding::{COMBO_FIELD, HotkeyEdge};
use crate::combo::HotkeyCombo;
use crate::reconciler::HotkeyReconciler;

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
    if let Some(Variant::String(raw)) = instance.overrides.get(COMBO_FIELD) {
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

    async fn count_of_kinds(&self, kind_ids: &[String]) -> Result<usize, StorageError> {
        self.inner.count_of_kinds(kind_ids).await
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

    async fn actions_by_instance(
        &self,
    ) -> Result<HashMap<TriggerInstanceId, Vec<ActionId>>, StorageError> {
        self.inner.actions_by_instance().await
    }

    async fn apply_changes(&self, changes: &TriggerChanges) -> Result<(), StorageError> {
        let saved = changes
            .saved
            .iter()
            .map(canonicalized)
            .collect::<Result<Vec<_>, _>>()?;
        let canonical = TriggerChanges {
            saved,
            ..changes.clone()
        };
        self.inner.apply_changes(&canonical).await?;
        self.sync().await;
        Ok(())
    }

    async fn commit_overlay_wiring(
        &self,
        wiring: &TriggerWiring,
        queues: Arc<dyn QueueRepo>,
        actions: Arc<dyn ActionRepo>,
    ) -> Result<(), StorageError> {
        let canonical = TriggerWiring {
            instance: canonicalized(&wiring.instance)?,
            ..wiring.clone()
        };
        self.inner
            .commit_overlay_wiring(&canonical, queues, actions)
            .await?;
        self.sync().await;
        Ok(())
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<(), StorageError> {
        self.inner.link_action(action_id, instance_id).await
    }

    async fn link_action_at(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError> {
        self.inner
            .link_action_at(action_id, instance_id, position)
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
        self.inner.set_enabled(id, enabled).await?;
        self.sync().await;
        Ok(())
    }

    async fn set_all_enabled(
        &self,
        ids: &[TriggerInstanceId],
        enabled: bool,
    ) -> Result<(), StorageError> {
        self.inner.set_all_enabled(ids, enabled).await?;
        self.sync().await;
        Ok(())
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
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{PermissionRung, PlatformScope};

    use super::*;
    use crate::vocabulary::event_kinds;

    const OTHER_KIND: &str = "midi.note_on";

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
        for kind in [event_kinds::GLOBAL_PRESSED, event_kinds::GLOBAL_RELEASED] {
            let saved = canonicalized(&instance(kind, Some("shift+ctrl+f5"))).unwrap();

            assert_eq!(stored_combo(&saved), Some("Ctrl+Shift+F5"), "kind {kind}");
        }
    }

    #[test]
    fn an_unparseable_hotkey_combo_is_refused_as_a_validation_failure_on_the_combo_field() {
        let refused = canonicalized(&instance(event_kinds::GLOBAL_PRESSED, Some("Ctrl+XYZ")));

        assert!(
            matches!(refused, Err(StorageError::ValidationFailed { ref field, .. }) if field == COMBO_FIELD),
            "got {refused:?}"
        );
    }

    #[test]
    fn a_blank_hotkey_combo_is_refused_on_the_combo_field() {
        for blank in ["", "   "] {
            let refused = canonicalized(&instance(event_kinds::GLOBAL_PRESSED, Some(blank)));

            assert!(
                matches!(refused, Err(StorageError::ValidationFailed { ref field, .. }) if field == COMBO_FIELD),
                "combo {blank:?}: got {refused:?}"
            );
        }
    }

    #[test]
    fn a_combo_is_left_untouched_when_the_trigger_is_not_a_hotkey_or_has_no_combo_yet() {
        for (kind, combo) in [
            (OTHER_KIND, Some("f5")),
            (OTHER_KIND, Some("Ctrl+XYZ")),
            (event_kinds::GLOBAL_PRESSED, None),
        ] {
            let original = instance(kind, combo);

            let saved = canonicalized(&original).unwrap();

            assert_eq!(saved, original, "kind {kind} with combo {combo:?}");
        }
    }
}
