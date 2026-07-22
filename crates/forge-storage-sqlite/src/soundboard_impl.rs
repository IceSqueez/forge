use std::path::PathBuf;

use async_trait::async_trait;
use forge_storage::{SoundboardClipsRepo, StorageError, StoredClip};
use forge_types::{ClipId, OutputDevice};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn parse_clip_id(s: &str) -> Result<ClipId, StorageError> {
    serde_json::from_str(&format!("\"{s}\"")).map_err(|e| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "invalid clip id `{s}`: {e}"
        )))
    })
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

pub struct SqliteSoundboardClipsRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteSoundboardClipsRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct ClipRow {
    id: String,
    name: String,
    file_path: String,
    volume: f64,
    output_device: String,
    hotkey: Option<String>,
    created_at: i64,
    category: String,
    loop_playback: bool,
    duration_secs: Option<f64>,
    builtin_id: Option<String>,
}

#[async_trait]
impl SoundboardClipsRepo for SqliteSoundboardClipsRepo {
    async fn list(&self) -> Result<Vec<StoredClip>, StorageError> {
        let rows: Vec<ClipRow> = sqlx::query_as(
            "SELECT id, name, file_path, volume, output_device, hotkey, created_at,
                    category, loop_playback, duration_secs, builtin_id
             FROM soundboard_clips ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(row_to_clip).collect()
    }

    async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, StorageError> {
        let id_str = id.to_string();
        let row: Option<ClipRow> = sqlx::query_as(
            "SELECT id, name, file_path, volume, output_device, hotkey, created_at,
                    category, loop_playback, duration_secs, builtin_id
             FROM soundboard_clips WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(row_to_clip).transpose()
    }

    async fn save(&self, clip: &StoredClip) -> Result<(), StorageError> {
        let id_str = clip.id.to_string();
        let path_str = clip
            .file_path
            .to_str()
            .ok_or_else(|| {
                StorageError::from(SqliteStorageError::Decode("non-utf8 clip path".to_string()))
            })?
            .to_string();
        let device_json =
            serde_json::to_string(&clip.output_device).map_err(StorageError::Serialization)?;
        let created_ms = to_epoch_ms(clip.created_at);
        let duration_secs = clip.duration_secs.map(f64::from);

        sqlx::query(
            "INSERT INTO soundboard_clips
                (id, name, file_path, volume, output_device, hotkey, created_at,
                 category, loop_playback, duration_secs, builtin_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                file_path = excluded.file_path,
                volume = excluded.volume,
                output_device = excluded.output_device,
                hotkey = excluded.hotkey,
                category = excluded.category,
                loop_playback = excluded.loop_playback,
                duration_secs = excluded.duration_secs,
                builtin_id = excluded.builtin_id",
        )
        .bind(&id_str)
        .bind(&clip.name)
        .bind(&path_str)
        .bind(f64::from(clip.volume))
        .bind(&device_json)
        .bind(clip.hotkey.as_deref())
        .bind(created_ms)
        .bind(&clip.category)
        .bind(clip.loop_playback)
        .bind(duration_secs)
        .bind(clip.builtin_id.as_deref())
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: ClipId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM soundboard_clips WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_clip(row: ClipRow) -> Result<StoredClip, StorageError> {
    let id = parse_clip_id(&row.id)?;
    let output_device: OutputDevice = serde_json::from_str(&row.output_device).map_err(|e| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "invalid output_device json: {e}"
        )))
    })?;
    let created_at = OffsetDateTime::from_unix_timestamp_nanos(
        i128::from(row.created_at) * 1_000_000,
    )
    .map_err(|e| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "invalid created_at {}: {e}",
            row.created_at
        )))
    })?;
    Ok(StoredClip {
        id,
        name: row.name,
        file_path: PathBuf::from(row.file_path),
        volume: row.volume as f32,
        output_device,
        hotkey: row.hotkey,
        created_at,
        category: row.category,
        loop_playback: row.loop_playback,
        duration_secs: row.duration_secs.map(|d| d as f32),
        builtin_id: row.builtin_id,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::SqliteBackend;
    use forge_types::OutputDevice;

    async fn open() -> SqliteBackend {
        SqliteBackend::open_with_key(":memory:", [0xab; 32])
            .await
            .expect("open")
    }

    fn sample_clip() -> StoredClip {
        StoredClip {
            id: ClipId::new(),
            name: "Air horn".to_string(),
            file_path: PathBuf::from("/tmp/airhorn.wav"),
            volume: 0.8,
            output_device: OutputDevice::ByName {
                name: "Speakers".to_string(),
            },
            hotkey: Some("Ctrl+1".to_string()),
            created_at: OffsetDateTime::now_utc(),
            category: "memes".to_string(),
            loop_playback: false,
            duration_secs: None,
            builtin_id: None,
        }
    }

    #[tokio::test]
    async fn save_and_get_roundtrip() {
        let backend = open().await;
        let clip = sample_clip();
        let repo = backend.soundboard_clips_repo_impl();
        repo.save(&clip).await.unwrap();
        let back = repo.get(clip.id).await.unwrap().unwrap();
        assert_eq!(back.name, clip.name);
        assert_eq!(back.file_path, clip.file_path);
        assert!((back.volume - 0.8).abs() < 1e-6);
        assert_eq!(back.hotkey.as_deref(), Some("Ctrl+1"));
    }

    #[tokio::test]
    async fn list_sorted_by_name() {
        let backend = open().await;
        let repo = backend.soundboard_clips_repo_impl();
        for n in ["Zed", "Air", "Mid"] {
            let mut c = sample_clip();
            c.id = ClipId::new();
            c.name = n.to_string();
            repo.save(&c).await.unwrap();
        }
        let listed = repo.list().await.unwrap();
        let names: Vec<&str> = listed.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Air", "Mid", "Zed"]);
    }

    #[tokio::test]
    async fn delete_returns_true_for_existing_and_false_for_missing() {
        let backend = open().await;
        let repo = backend.soundboard_clips_repo_impl();
        let clip = sample_clip();
        repo.save(&clip).await.unwrap();
        assert!(repo.delete(clip.id).await.unwrap());
        assert!(!repo.delete(clip.id).await.unwrap());
    }

    #[tokio::test]
    async fn update_replaces_existing_row() {
        let backend = open().await;
        let repo = backend.soundboard_clips_repo_impl();
        let mut clip = sample_clip();
        repo.save(&clip).await.unwrap();
        clip.name = "Renamed".to_string();
        clip.volume = 0.25;
        repo.save(&clip).await.unwrap();
        let back = repo.get(clip.id).await.unwrap().unwrap();
        assert_eq!(back.name, "Renamed");
        assert!((back.volume - 0.25).abs() < 1e-6);
    }
}
