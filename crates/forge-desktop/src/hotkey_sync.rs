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
