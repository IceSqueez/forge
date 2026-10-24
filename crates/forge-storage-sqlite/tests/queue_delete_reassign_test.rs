#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{ActionRepo, QueueRepo, StorageError};
use forge_storage_sqlite::{SqliteActionRepo, SqliteQueueRepo, apply_migrations, connect};
use forge_types::{Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, Queue, QueueId};
use sqlx::SqlitePool;
use tempfile::TempDir;

struct Db {
    pool: SqlitePool,
    queues: SqliteQueueRepo,
    actions: SqliteActionRepo,
    _dir: TempDir,
}

async fn db() -> Db {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    apply_migrations(&pool).await.expect("migrate");
    Db {
        queues: SqliteQueueRepo::new(pool.clone()),
        actions: SqliteActionRepo::new(pool.clone()),
        pool,
        _dir: dir,
    }
}

fn queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: 1,
    }
}

fn action(name: &str, queue_id: QueueId) -> Action {
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
        sub_actions: Vec::new(),
    }
}

impl Db {
    async fn with_queue(&self, name: &str) -> QueueId {
        let q = queue(name);
        self.queues.save(&q).await.unwrap();
        q.id
    }

    async fn with_action(&self, name: &str, queue_id: QueueId) -> ActionId {
        let a = action(name, queue_id);
        self.actions.save(&a).await.unwrap();
        a.id
    }

    async fn queue_of(&self, id: ActionId) -> Option<QueueId> {
        if let Some(live) = self.actions.get(id).await.unwrap() {
            return Some(live.queue_id);
        }
        self.actions
            .list_archived()
            .await
            .unwrap()
            .into_iter()
            .find(|a| a.id == id)
            .map(|a| a.queue_id)
    }

    async fn queue_exists(&self, id: QueueId) -> bool {
        self.queues.get(id).await.unwrap().is_some()
    }
}

#[tokio::test]
async fn deleting_a_queue_moves_its_live_and_archived_actions_to_the_default_and_removes_it() {
    let db = db().await;
    let doomed = db.with_queue("Alerts").await;
    let kept = db.with_queue("Chat").await;
    let live = db.with_action("Live", doomed).await;
    let archived = db.with_action("Archived", doomed).await;
    let elsewhere = db.with_action("Elsewhere", kept).await;
    assert!(db.actions.archive(archived).await.unwrap());

    let moved = db.queues.delete_reassigning_actions(doomed).await.unwrap();

    assert_eq!(
        (
            moved,
            db.queue_exists(doomed).await,
            db.queue_of(live).await,
            db.queue_of(archived).await,
            db.queue_of(elsewhere).await,
        ),
        (
            Some(2),
            false,
            Some(DEFAULT_QUEUE_ID),
            Some(DEFAULT_QUEUE_ID),
            Some(kept)
        )
    );
}

#[tokio::test]
async fn deleting_a_queue_that_does_not_exist_reports_none() {
    let db = db().await;

    let moved = db
        .queues
        .delete_reassigning_actions(QueueId::new())
        .await
        .unwrap();

    assert_eq!(moved, None);
}

#[tokio::test]
async fn neither_delete_path_removes_the_default_queue() {
    let db = db().await;
    let parked = db.with_action("Parked", DEFAULT_QUEUE_ID).await;

    let plain = db.queues.delete(DEFAULT_QUEUE_ID).await;
    let reassigning = db.queues.delete_reassigning_actions(DEFAULT_QUEUE_ID).await;

    assert!(
        matches!(plain, Err(StorageError::DefaultQueueUndeletable)),
        "{plain:?}"
    );
    assert!(
        matches!(reassigning, Err(StorageError::DefaultQueueUndeletable)),
        "{reassigning:?}"
    );
    assert_eq!(
        (
            db.queue_exists(DEFAULT_QUEUE_ID).await,
            db.queue_of(parked).await
        ),
        (true, Some(DEFAULT_QUEUE_ID))
    );
}

#[tokio::test]
async fn a_delete_that_fails_partway_leaves_the_actions_in_their_queue() {
    let db = db().await;
    let doomed = db.with_queue("Alerts").await;
    let live = db.with_action("Live", doomed).await;
    sqlx::query(
        "CREATE TRIGGER refuse_queue_delete BEFORE DELETE ON queues \
         BEGIN SELECT RAISE(ABORT, 'refused'); END",
    )
    .execute(&db.pool)
    .await
    .unwrap();

    let failed = db.queues.delete_reassigning_actions(doomed).await;

    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        (db.queue_exists(doomed).await, db.queue_of(live).await),
        (true, Some(doomed))
    );
}

#[tokio::test]
async fn a_queue_named_default_is_an_ordinary_queue_once_the_default_was_renamed() {
    let db = db().await;
    db.queues
        .save(&Queue {
            name: "Main".to_owned(),
            ..db.queues.get(DEFAULT_QUEUE_ID).await.unwrap().unwrap()
        })
        .await
        .unwrap();
    let impostor = db.with_queue("Default").await;
    let moved_action = db.with_action("Moved", impostor).await;

    let moved = db
        .queues
        .delete_reassigning_actions(impostor)
        .await
        .unwrap();

    let names: Vec<_> = db
        .queues
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|q| (q.id, q.name))
        .collect();
    assert_eq!(
        (moved, db.queue_of(moved_action).await, names),
        (
            Some(1),
            Some(DEFAULT_QUEUE_ID),
            vec![(DEFAULT_QUEUE_ID, "Main".to_owned())]
        )
    );
}
