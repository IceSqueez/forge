use sqlx::SqlitePool;

pub(crate) async fn vacuum_into(pool: &SqlitePool, target: &str) -> Result<(), sqlx::Error> {
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(pool)
        .await
        .map(|_| ())
}

pub(crate) async fn checkpoint_then_vacuum_into(
    pool: &SqlitePool,
    target: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_all(pool)
        .await?;
    vacuum_into(pool, target).await
}
