use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::sync::Notify;

const FLUSH_DELAY: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(crate) struct GlobalReadTally {
    pending: Mutex<HashMap<String, u64>>,
    counted: Notify,
    closing: Notify,
}

impl GlobalReadTally {
    pub(crate) fn count(&self, name: &str) {
        {
            let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
            match pending.get_mut(name) {
                Some(reads) => *reads = reads.saturating_add(1),
                None => {
                    pending.insert(name.to_owned(), 1);
                }
            }
        }
        self.counted.notify_one();
    }

    pub(crate) fn unflushed(&self, name: &str) -> u64 {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn forget(&self, name: &str) {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(name);
    }

    pub(crate) fn rename(&self, old_name: &str, new_name: &str) {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(reads) = pending.remove(old_name) {
            pending.insert(new_name.to_owned(), reads);
        }
    }

    pub(crate) fn close(&self) {
        self.closing.notify_one();
    }

    fn take(&self) -> HashMap<String, u64> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|p| p.into_inner()))
    }

    fn put_back(&self, unsaved: HashMap<String, u64>) {
        let mut pending = self.pending.lock().unwrap_or_else(|p| p.into_inner());
        for (name, reads) in unsaved {
            let total = pending.entry(name).or_insert(0);
            *total = total.saturating_add(reads);
        }
    }

    pub(crate) async fn flush(&self, writer: &SqlitePool) {
        let reads = self.take();
        if reads.is_empty() {
            return;
        }
        if let Err(error) = write_reads(writer, &reads).await {
            tracing::warn!(%error, "global read counts were not saved; retrying with the next reads");
            self.put_back(reads);
        }
    }
}

async fn write_reads(writer: &SqlitePool, reads: &HashMap<String, u64>) -> Result<(), sqlx::Error> {
    let mut tx = writer.begin().await?;
    for (name, count) in reads {
        sqlx::query("UPDATE globals SET reads = reads + ? WHERE name = ?")
            .bind(i64::try_from(*count).unwrap_or(i64::MAX))
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

pub(crate) fn spawn_read_flush_task(tally: Arc<GlobalReadTally>, writer: SqlitePool) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = tally.closing.notified() => return,
                () = tally.counted.notified() => {}
            }
            tokio::select! {
                () = tally.closing.notified() => return,
                () = tokio::time::sleep(FLUSH_DELAY) => {}
            }
            tally.flush(&writer).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use forge_storage::{GlobalsRepo, StorageError};
    use forge_types::Variant;
    use tempfile::TempDir;

    use crate::{SqliteGlobalsRepo, SqlitePools, apply_migrations, connect_pools};

    struct Rig {
        repo: SqliteGlobalsRepo,
        pools: SqlitePools,
        _dir: TempDir,
    }

    impl Rig {
        async fn open(names: &[&str]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let url = format!("sqlite://{}", dir.path().join("forge.db").display());
            let pools = connect_pools(&url).await.unwrap();
            apply_migrations(pools.writer()).await.unwrap();
            let repo = SqliteGlobalsRepo::new(pools.clone());
            for name in names {
                repo.set(name, Variant::Int(1), true).await.unwrap();
            }
            Self {
                repo,
                pools,
                _dir: dir,
            }
        }

        async fn read(&self, name: &str, times: usize) {
            for _ in 0..times {
                assert!(self.repo.get(name).await.unwrap().is_some());
            }
        }

        async fn flush(&self) {
            self.repo.read_tally().flush(self.pools.writer()).await;
        }

        async fn stored_reads(&self, name: &str) -> i64 {
            sqlx::query_scalar("SELECT reads FROM globals WHERE name = ?")
                .bind(name)
                .fetch_one(self.pools.reader())
                .await
                .unwrap()
        }

        async fn listed_reads(&self, name: &str) -> Option<u64> {
            self.repo
                .list()
                .await
                .unwrap()
                .into_iter()
                .find(|entry| entry.name == name)
                .map(|entry| entry.reads)
        }

        async fn refuse_read_updates_of(&self, name: &str) {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TRIGGER refuse_reads BEFORE UPDATE OF reads ON globals \
                 WHEN NEW.name = '{name}' BEGIN SELECT RAISE(ABORT, 'refused'); END"
            )))
            .execute(self.pools.writer())
            .await
            .unwrap();
        }

        async fn allow_read_updates(&self) {
            sqlx::query("DROP TRIGGER refuse_reads")
                .execute(self.pools.writer())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn get_answers_and_counts_with_the_writer_pool_closed() {
        let rig = Rig::open(&["x"]).await;
        rig.pools.writer().close().await;

        let value = rig.repo.get("x").await.unwrap();

        assert_eq!(value, Some(Variant::Int(1)));
        assert_eq!(rig.repo.read_tally().unflushed("x"), 1);
    }

    #[tokio::test]
    async fn get_counts_nothing_for_a_missing_or_archived_global() {
        let rig = Rig::open(&["shelved"]).await;
        rig.repo.archive("shelved").await.unwrap();

        let answers = (
            rig.repo.get("absent").await.unwrap(),
            rig.repo.get("shelved").await.unwrap(),
        );

        assert_eq!(answers, (None, None));
        assert_eq!(rig.repo.read_tally().unflushed("absent"), 0);
        assert_eq!(rig.repo.read_tally().unflushed("shelved"), 0);
    }

    #[tokio::test]
    async fn listing_adds_unflushed_reads_to_the_stored_count() {
        let rig = Rig::open(&["x"]).await;
        rig.read("x", 1).await;
        rig.flush().await;
        rig.read("x", 2).await;

        assert_eq!(rig.stored_reads("x").await, 1);
        assert_eq!(rig.listed_reads("x").await, Some(3));
    }

    #[tokio::test]
    async fn listing_archived_globals_shows_their_unflushed_reads() {
        let rig = Rig::open(&["x"]).await;
        rig.read("x", 2).await;
        rig.repo.archive("x").await.unwrap();

        let archived = rig.repo.list_archived().await.unwrap();

        assert_eq!(archived[0].reads, 2);
    }

    #[tokio::test]
    async fn flushing_twice_stores_each_read_once() {
        let rig = Rig::open(&["x"]).await;
        rig.read("x", 2).await;

        rig.flush().await;
        rig.flush().await;

        assert_eq!(rig.stored_reads("x").await, 2);
        assert_eq!(rig.listed_reads("x").await, Some(2));
    }

    #[tokio::test]
    async fn renaming_carries_unflushed_reads_to_the_new_name() {
        let rig = Rig::open(&["old"]).await;
        rig.read("old", 2).await;

        rig.repo.rename("old", "new").await.unwrap();
        rig.flush().await;

        assert_eq!(rig.stored_reads("new").await, 2);
    }

    #[tokio::test]
    async fn a_refused_rename_leaves_unflushed_reads_on_both_names() {
        let rig = Rig::open(&["first", "second"]).await;
        rig.read("first", 1).await;
        rig.read("second", 2).await;

        let renamed = rig.repo.rename("first", "second").await;

        assert!(matches!(renamed, Err(StorageError::NameCollision { .. })));
        assert_eq!(rig.listed_reads("first").await, Some(1));
        assert_eq!(rig.listed_reads("second").await, Some(2));
    }

    #[tokio::test]
    async fn a_global_recreated_after_delete_does_not_inherit_unflushed_reads() {
        let rig = Rig::open(&["x"]).await;
        rig.read("x", 2).await;

        rig.repo.delete("x").await.unwrap();
        rig.repo.set("x", Variant::Int(2), true).await.unwrap();
        rig.flush().await;

        assert_eq!(rig.stored_reads("x").await, 0);
    }

    #[tokio::test]
    async fn a_failed_flush_stores_nothing_and_the_retry_stores_every_read_once() {
        let rig = Rig::open(&["good", "refused"]).await;
        rig.refuse_read_updates_of("refused").await;
        rig.read("good", 2).await;
        rig.read("refused", 1).await;

        rig.flush().await;
        let after_failure = (
            rig.stored_reads("good").await,
            rig.stored_reads("refused").await,
            rig.listed_reads("good").await,
        );
        rig.allow_read_updates().await;
        rig.read("good", 1).await;
        rig.flush().await;

        assert_eq!(after_failure, (0, 0, Some(2)));
        assert_eq!(rig.stored_reads("good").await, 3);
        assert_eq!(rig.stored_reads("refused").await, 1);
    }
}
