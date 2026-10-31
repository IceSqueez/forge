#![allow(dead_code, clippy::expect_used)]

use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::Path;

use forge_storage::DataProvider;
use forge_storage_sqlite::{SqliteBackend, SqlitePools, apply_migrations, connect_pools};
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, QueueId, TriggerInstance, TriggerInstanceId,
};
use tempfile::TempDir;

pub(crate) const TEST_KEY: [u8; 32] = [0xab; 32];
const DATABASE_FILE: &str = "forge.db";
const MEDIA_DIR: &str = "media";
const DEFAULT_QUEUE_NAME: &str = "Default";

pub(crate) struct Sandboxed<T> {
    inner: T,
    _temp_root: TempDir,
}

impl<T> Sandboxed<T> {
    pub(crate) fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
        Sandboxed {
            inner: wrap(self.inner),
            _temp_root: self._temp_root,
        }
    }
}

impl<T> Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub(crate) async fn sandboxed_backend(url: &str, key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = SqliteBackend::open_for_test(url, key, media_root.path().to_owned(), None)
        .await
        .expect("a backend");
    Sandboxed {
        inner: backend,
        _temp_root: media_root,
    }
}

pub(crate) async fn empty_pools() -> Sandboxed<SqlitePools> {
    let root = tempfile::tempdir().expect("a temporary database directory");
    let url = database_url(&root);
    let pools = connect_pools(&url).await.expect("the production pools");
    Sandboxed {
        inner: pools,
        _temp_root: root,
    }
}

pub(crate) async fn migrated_pools() -> Sandboxed<SqlitePools> {
    let pools = empty_pools().await;
    apply_migrations(pools.writer()).await.expect("migrations");
    pools
}

pub(crate) async fn migrated_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let root = tempfile::tempdir().expect("a temporary database directory");
    let url = database_url(&root);
    let backend = SqliteBackend::open_for_test(&url, key, root.path().join(MEDIA_DIR), None)
        .await
        .expect("a backend");
    Sandboxed {
        inner: backend,
        _temp_root: root,
    }
}

pub(crate) async fn setup() -> Sandboxed<SqliteBackend> {
    migrated_backend(TEST_KEY).await
}

pub(crate) fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

pub(crate) fn database_url(root: &TempDir) -> String {
    sqlite_url(&root.path().join(DATABASE_FILE))
}

pub(crate) async fn default_queue_id(backend: &SqliteBackend) -> QueueId {
    backend
        .queue_repo()
        .get_by_name(DEFAULT_QUEUE_NAME)
        .await
        .expect("get default queue")
        .expect("default queue seeded by migration")
        .id
}

pub(crate) fn action_named(name: &str, queue_id: QueueId) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

pub(crate) async fn insert_action(backend: &SqliteBackend, name: &str) -> ActionId {
    let action = action_named(name, default_queue_id(backend).await);
    backend
        .action_repo()
        .save(&action)
        .await
        .expect("save action");
    action.id
}

pub(crate) fn trigger_instance(kind_id: &str, name: &str, user_defined: bool) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: name.to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}
