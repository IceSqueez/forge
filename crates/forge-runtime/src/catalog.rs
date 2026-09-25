use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use forge_storage::{ActionRepo, CatalogRevision, DataProvider, StorageError, TriggerInstanceRepo};
use forge_types::{Action, ActionId, TriggerInstance};
use tokio::sync::Mutex;

pub struct CatalogBinding {
    pub action: Arc<Action>,
    pub instance: TriggerInstance,
}

/// Enabled actions and their enabled trigger instances, in storage list order.
pub struct CatalogSnapshot {
    revision: u64,
    actions: HashMap<ActionId, Arc<Action>>,
    bindings: Vec<CatalogBinding>,
    bindings_by_kind: HashMap<String, Vec<usize>>,
}

impl CatalogSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn action(&self, id: ActionId) -> Option<&Arc<Action>> {
        self.actions.get(&id)
    }

    pub fn binding(&self, index: usize) -> Option<&CatalogBinding> {
        self.bindings.get(index)
    }

    /// Ascending, so bindings come back in the same order as the storage listing.
    pub fn binding_indexes_for_kinds<'a>(
        &self,
        kind_ids: impl IntoIterator<Item = &'a str>,
    ) -> Vec<usize> {
        let mut indexes: Vec<usize> = kind_ids
            .into_iter()
            .filter_map(|kind| self.bindings_by_kind.get(kind))
            .flatten()
            .copied()
            .collect();
        indexes.sort_unstable();
        indexes.dedup();
        indexes
    }

    async fn load(
        revision: u64,
        actions: &dyn ActionRepo,
        trigger_instances: &dyn TriggerInstanceRepo,
    ) -> Result<Self, StorageError> {
        let mut snapshot = Self {
            revision,
            actions: HashMap::new(),
            bindings: Vec::new(),
            bindings_by_kind: HashMap::new(),
        };
        for action in actions.list().await? {
            if !action.enabled {
                continue;
            }
            let action = Arc::new(action);
            for instance in trigger_instances.list_for_action(action.id).await? {
                if !instance.enabled {
                    continue;
                }
                snapshot
                    .bindings_by_kind
                    .entry(instance.kind_id.clone())
                    .or_default()
                    .push(snapshot.bindings.len());
                snapshot.bindings.push(CatalogBinding {
                    action: Arc::clone(&action),
                    instance,
                });
            }
            snapshot.actions.insert(action.id, action);
        }
        Ok(snapshot)
    }
}

pub struct Catalog {
    actions: Arc<dyn ActionRepo>,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    revision: CatalogRevision,
    built: ArcSwapOption<CatalogSnapshot>,
    rebuild: Mutex<()>,
}

impl Catalog {
    pub fn new(
        actions: Arc<dyn ActionRepo>,
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        revision: CatalogRevision,
    ) -> Arc<Self> {
        Arc::new(Self {
            actions,
            trigger_instances,
            revision,
            built: ArcSwapOption::empty(),
            rebuild: Mutex::new(()),
        })
    }

    pub fn from_provider(provider: &dyn DataProvider) -> Arc<Self> {
        Self::new(
            provider.action_repo(),
            provider.trigger_instance_repo(),
            provider.catalog_revision(),
        )
    }

    /// Reflects every catalog write that returned before this call; rebuilds from storage first if one did.
    pub async fn current(&self) -> Result<Arc<CatalogSnapshot>, StorageError> {
        if let Some(fresh) = self.built_at(self.revision.current()) {
            return Ok(fresh);
        }
        let _single_flight = self.rebuild.lock().await;
        let wanted = self.revision.current();
        if let Some(fresh) = self.built_at(wanted) {
            return Ok(fresh);
        }
        let snapshot = Arc::new(
            CatalogSnapshot::load(
                wanted,
                self.actions.as_ref(),
                self.trigger_instances.as_ref(),
            )
            .await?,
        );
        self.built.store(Some(Arc::clone(&snapshot)));
        Ok(snapshot)
    }

    fn built_at(&self, revision: u64) -> Option<Arc<CatalogSnapshot>> {
        self.built
            .load_full()
            .filter(|snapshot| snapshot.revision == revision)
    }
}

