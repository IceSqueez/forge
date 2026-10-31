#![expect(clippy::expect_used, reason = "test fixtures")]
#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;
use std::path::Path;

use forge_storage::DataProvider;
pub(crate) use forge_storage_sqlite::test_support::{
    Sandboxed, TEST_KEY, sandboxed_backend_at, sandboxed_file_backend,
};
use forge_storage_sqlite::{SqliteBackend, SqlitePools, apply_migrations, connect_pools};
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, QueueId, TriggerInstance, TriggerInstanceId,
};
use tempfile::TempDir;

pub(crate) const DEFAULT_QUEUE: &str = "00000000000000000000000000";
pub(crate) const MILLIS_PER_SECOND: i64 = 1_000;
pub(crate) const MILLIS_PER_MINUTE: i64 = 60 * MILLIS_PER_SECOND;
const DATABASE_FILE: &str = "forge.db";
const MEDIA_DIR: &str = "media";
const DEFAULT_QUEUE_NAME: &str = "Default";

pub(crate) async fn empty_pools() -> Sandboxed<SqlitePools> {
    let root = tempfile::tempdir().expect("a temporary database directory");
    let url = database_url(&root);
    let pools = connect_pools(&url).await.expect("the production pools");
    Sandboxed::new(pools, root)
}

pub(crate) async fn migrated_pools() -> Sandboxed<SqlitePools> {
    let pools = empty_pools().await;
    apply_migrations(pools.writer()).await.expect("migrations");
    pools
}

pub(crate) async fn migrated_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    sandboxed_file_backend(key).await
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
