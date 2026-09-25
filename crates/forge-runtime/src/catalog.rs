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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use async_trait::async_trait;
    use forge_storage::trigger_instance::MockTriggerInstanceRepo;
    use forge_storage::{ActionTelemetry, ExecutionStatus};
    use forge_types::{PermissionRung, QueueId, TriggerConfig, TriggerInstanceId};
    use time::OffsetDateTime;
    use tokio::sync::{Notify, Semaphore};

    use super::*;

    /// Action rows whose `list` copies the rows first and then, when gated, waits for a
    /// permit, so a test can land a write between the storage read and the snapshot.
    struct StubActions {
        rows: StdMutex<Vec<Action>>,
        lists: AtomicUsize,
        fail_next_list: AtomicBool,
        gated: bool,
        permits: Semaphore,
        entered: Notify,
    }

    impl StubActions {
        fn new(gated: bool) -> Self {
            Self {
                rows: StdMutex::default(),
                lists: AtomicUsize::new(0),
                fail_next_list: AtomicBool::new(false),
                gated,
                permits: Semaphore::new(0),
                entered: Notify::new(),
            }
        }

        fn put(&self, action: Action) {
            let mut rows = self.rows.lock().unwrap();
            rows.retain(|row| row.id != action.id);
            rows.push(action);
        }
    }

    #[async_trait]
    impl ActionRepo for StubActions {
        async fn list(&self) -> Result<Vec<Action>, StorageError> {
            self.lists.fetch_add(1, Ordering::SeqCst);
            if self.fail_next_list.swap(false, Ordering::SeqCst) {
                return Err(StorageError::Connection {
                    reason: "database is locked".to_owned(),
                });
            }
            let rows = self.rows.lock().unwrap().clone();
            self.entered.notify_one();
            if self.gated {
                self.permits.acquire().await.unwrap().forget();
            }
            Ok(rows)
        }
        async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
            Ok(None)
        }
        async fn save(&self, _action: &Action) -> Result<(), StorageError> {
            Ok(())
        }
        async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list_by_group<'a>(
            &'a self,
            _group: Option<&'a str>,
        ) -> Result<Vec<Action>, StorageError> {
            Ok(Vec::new())
        }
        async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
            Ok(ActionTelemetry::default())
        }
        async fn record_execution(
            &self,
            _action_id: ActionId,
            _started_at: OffsetDateTime,
            _duration_ms: u64,
            _status: ExecutionStatus,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn prune_executions_before(
            &self,
            _cutoff: OffsetDateTime,
        ) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    type Links = Arc<StdMutex<HashMap<ActionId, Vec<TriggerInstance>>>>;

    struct Rig {
        actions: Arc<StubActions>,
        links: Links,
        revision: CatalogRevision,
        catalog: Arc<Catalog>,
    }

    fn rig(actions: StubActions) -> Rig {
        let actions = Arc::new(actions);
        let links: Links = Arc::default();
        let mut instances = MockTriggerInstanceRepo::new();
        let served = Arc::clone(&links);
        instances
            .expect_list_for_action()
            .returning(move |id| Ok(served.lock().unwrap().get(&id).cloned().unwrap_or_default()));
        let revision = CatalogRevision::new();
        let catalog = Catalog::new(
            Arc::clone(&actions) as Arc<dyn ActionRepo>,
            Arc::new(instances),
            revision.clone(),
        );
        Rig {
            actions,
            links,
            revision,
            catalog,
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
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        }
    }

    fn instance(kind: &str) -> TriggerInstance {
        TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: kind.to_owned(),
            name: kind.to_owned(),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }
    }

    fn bound(rig: &Rig, action: Action, kind: &str) -> ActionId {
        let id = action.id;
        rig.links
            .lock()
            .unwrap()
            .entry(id)
            .or_default()
            .push(instance(kind));
        rig.actions.put(action);
        id
    }

    fn names(snapshot: &CatalogSnapshot, kind: &str) -> Vec<String> {
        snapshot
            .binding_indexes_for_kinds([kind])
            .into_iter()
            .map(|index| snapshot.binding(index).unwrap().action.name.clone())
            .collect()
    }

    #[tokio::test]
    async fn a_read_after_a_returned_write_sees_the_write() {
        let rig = rig(StubActions::new(false));
        bound(&rig, action("first"), "k");
        rig.catalog.current().await.unwrap();

        bound(&rig, action("second"), "k");
        rig.revision.advance();

        let snapshot = rig.catalog.current().await.unwrap();
        assert_eq!(names(&snapshot, "k"), ["first", "second"]);
    }

    #[tokio::test]
    async fn a_write_landing_during_a_rebuild_forces_the_next_read_to_rebuild() {
        let rig = rig(StubActions::new(true));
        bound(&rig, action("first"), "k");
        let reader = tokio::spawn({
            let catalog = Arc::clone(&rig.catalog);
            async move { catalog.current().await.map(|_| ()) }
        });
        rig.actions.entered.notified().await;

        bound(&rig, action("second"), "k");
        rig.revision.advance();
        rig.actions.permits.add_permits(usize::from(u8::MAX));
        reader.await.unwrap().unwrap();

        let snapshot = rig.catalog.current().await.unwrap();
        assert_eq!(
            names(&snapshot, "k"),
            ["first", "second"],
            "a snapshot read before the write finished must not be labelled with its revision",
        );
    }

    #[tokio::test]
    async fn concurrent_readers_of_a_stale_catalog_share_one_rebuild() {
        let rig = rig(StubActions::new(true));
        bound(&rig, action("only"), "k");
        let readers: Vec<_> = (0..8)
            .map(|_| {
                let catalog = Arc::clone(&rig.catalog);
                tokio::spawn(async move { catalog.current().await.map(|_| ()) })
            })
            .collect();
        rig.actions.entered.notified().await;
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }

        rig.actions.permits.add_permits(usize::from(u8::MAX));
        for reader in readers {
            reader.await.unwrap().unwrap();
        }

        assert_eq!(rig.actions.lists.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failed_rebuild_surfaces_the_error_and_the_next_read_retries() {
        let rig = rig(StubActions::new(false));
        bound(&rig, action("first"), "k");
        rig.catalog.current().await.unwrap();
        bound(&rig, action("second"), "k");
        rig.revision.advance();
        rig.actions.fail_next_list.store(true, Ordering::SeqCst);

        assert!(matches!(
            rig.catalog.current().await,
            Err(StorageError::Connection { .. })
        ));
        let snapshot = rig.catalog.current().await.unwrap();
        assert_eq!(names(&snapshot, "k"), ["first", "second"]);
    }

    #[tokio::test]
    async fn disabled_actions_and_disabled_instances_are_left_out() {
        let rig = rig(StubActions::new(false));
        let disabled_action = bound(
            &rig,
            Action {
                enabled: false,
                ..action("off")
            },
            "k",
        );
        let live = action("on");
        let live_id = live.id;
        rig.links
            .lock()
            .unwrap()
            .entry(live_id)
            .or_default()
            .push(TriggerInstance {
                enabled: false,
                ..instance("k")
            });
        rig.actions.put(live);

        let snapshot = rig.catalog.current().await.unwrap();
        assert_eq!(
            (
                snapshot.action(disabled_action).is_some(),
                snapshot.action(live_id).is_some(),
                names(&snapshot, "k").len(),
            ),
            (false, true, 0),
        );
    }

    #[tokio::test]
    async fn bindings_for_several_kinds_come_back_once_each_in_storage_order() {
        let rig = rig(StubActions::new(false));
        bound(&rig, action("a"), "x");
        bound(&rig, action("b"), "y");
        bound(&rig, action("c"), "x");

        let snapshot = rig.catalog.current().await.unwrap();
        let order: Vec<_> = snapshot
            .binding_indexes_for_kinds(["y", "x", "x"])
            .into_iter()
            .map(|index| snapshot.binding(index).unwrap().action.name.clone())
            .collect();
        assert_eq!(order, ["a", "b", "c"]);
    }
}
