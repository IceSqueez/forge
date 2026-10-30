use forge_storage::settings::core_settings;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection, SqlitePool};

const SNAPSHOT_EXCLUDED_KEYS: [&str; 2] = [
    core_settings::BACKUP_SCHEDULE_LAST_SUCCESS_AT.name(),
    core_settings::BACKUP_SCHEDULE_CONTENT_FINGERPRINT.name(),
];

pub(crate) async fn vacuum_into(pool: &SqlitePool, target: &str) -> Result<(), sqlx::Error> {
    sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(pool)
        .await?;
    drop_service_progress(target).await
}

async fn drop_service_progress(target: &str) -> Result<(), sqlx::Error> {
    let mut snapshot = SqliteConnectOptions::new()
        .filename(target)
        .connect()
        .await?;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ''), (?, '')")
        .bind(SNAPSHOT_EXCLUDED_KEYS[0])
        .bind(SNAPSHOT_EXCLUDED_KEYS[1])
        .execute(&mut snapshot)
        .await?;
    sqlx::query("DELETE FROM settings WHERE key IN (?, ?)")
        .bind(SNAPSHOT_EXCLUDED_KEYS[0])
        .bind(SNAPSHOT_EXCLUDED_KEYS[1])
        .execute(&mut snapshot)
        .await?;
    sqlx::query("VACUUM").execute(&mut snapshot).await?;
    snapshot.close().await
}
