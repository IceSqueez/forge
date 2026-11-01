#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
#![allow(dead_code)]

use std::collections::BTreeMap;

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, PermissionRung, TriggerInstance,
    TriggerInstanceId,
};
use tempfile::TempDir;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0x42; 32];
const REFERENCE_EPOCH_SECS: i64 = 1_800_000_000;

pub(crate) struct Store {
    pub(crate) backend: SqliteBackend,
    _media_root: TempDir,
}

pub(crate) async fn store() -> Store {
    let media_root = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", TEST_KEY, media_root.path().into(), None)
            .await
            .unwrap();
    Store {
        backend,
        _media_root: media_root,
    }
}

pub(crate) fn at(offset_secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(REFERENCE_EPOCH_SECS + offset_secs).unwrap()
}

pub(crate) fn action(name: &str) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: DEFAULT_QUEUE_ID,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

pub(crate) async fn saved_action(store: &Store, name: &str) -> ActionId {
    let action = action(name);
    store.backend.action_repo().save(&action).await.unwrap();
    action.id
}

pub(crate) fn trigger(name: &str) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "core.test".to_owned(),
        name: name.to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}
