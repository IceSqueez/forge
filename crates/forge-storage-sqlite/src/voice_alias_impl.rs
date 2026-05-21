use async_trait::async_trait;
use forge_storage::{
    AliasId, AssignmentStrategy, IgnoreProfile, StorageError, VoiceAlias, VoiceAliasRepo,
};
use forge_tts_core::{EngineId, VoiceId};
use forge_voice::AliasState;

use crate::error::SqliteStorageError;

pub struct SqliteVoiceAliasRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteVoiceAliasRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

type AliasRow = (
    String,
    String,
    String,
    String,
    String,
    Option<f64>,
    Option<f64>,
    String,
);

fn decode_alias_row(row: AliasRow) -> Result<VoiceAlias, StorageError> {
    let (id_str, viewer_id, viewer_name, engine_id, voice_id, pitch, rate, state_str) = row;
    let state = match state_str.as_str() {
        "Blocked" => AliasState::Blocked,
        _ => AliasState::Active,
    };
    Ok(VoiceAlias {
        id: AliasId(id_str),
        viewer_id,
        viewer_name,
        engine_id: EngineId(engine_id),
        voice_id: VoiceId(voice_id),
        pitch_semitones: pitch.map(|v| v as f32),
        rate_multiplier: rate.map(|v| v as f32),
        state,
    })
}

#[async_trait]
impl VoiceAliasRepo for SqliteVoiceAliasRepo {
    async fn list(&self) -> Result<Vec<VoiceAlias>, StorageError> {
        let rows: Vec<AliasRow> = sqlx::query_as(
            "SELECT id, viewer_id, viewer_name, engine_id, voice_id,
                    pitch_semitones, rate_multiplier, state
             FROM voice_aliases ORDER BY viewer_name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(decode_alias_row).collect()
    }

    async fn upsert(&self, alias: &VoiceAlias) -> Result<(), StorageError> {
        let state_str = match alias.state {
            AliasState::Active => "Active",
            AliasState::Blocked => "Blocked",
        };
        sqlx::query(
            "INSERT INTO voice_aliases
                (id, viewer_id, viewer_name, engine_id, voice_id,
                 pitch_semitones, rate_multiplier, state, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                viewer_name     = excluded.viewer_name,
                engine_id       = excluded.engine_id,
                voice_id        = excluded.voice_id,
                pitch_semitones = excluded.pitch_semitones,
                rate_multiplier = excluded.rate_multiplier,
                state           = excluded.state,
                updated_at      = excluded.updated_at",
        )
        .bind(&alias.id.0)
        .bind(&alias.viewer_id)
        .bind(&alias.viewer_name)
        .bind(&alias.engine_id.0)
        .bind(&alias.voice_id.0)
        .bind(alias.pitch_semitones.map(f64::from))
        .bind(alias.rate_multiplier.map(f64::from))
        .bind(state_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn delete(&self, id: &AliasId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM voice_aliases WHERE id = ?")
            .bind(&id.0)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn find_by_viewer(&self, viewer_id: &str) -> Result<Option<VoiceAlias>, StorageError> {
        let row: Option<AliasRow> = sqlx::query_as(
            "SELECT id, viewer_id, viewer_name, engine_id, voice_id,
                    pitch_semitones, rate_multiplier, state
             FROM voice_aliases WHERE viewer_id = ? LIMIT 1",
        )
        .bind(viewer_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(decode_alias_row).transpose()
    }

    async fn get_strategy(&self) -> Result<AssignmentStrategy, StorageError> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT value FROM settings WHERE key = 'voice:strategy'")
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        match row.and_then(|(v,)| v) {
            None => Ok(AssignmentStrategy::default()),
            Some(json) => serde_json::from_str(&json).map_err(StorageError::Serialization),
        }
    }

    async fn set_strategy(&self, strategy: &AssignmentStrategy) -> Result<(), StorageError> {
        let json = serde_json::to_string(strategy).map_err(StorageError::Serialization)?;
        sqlx::query(
            "INSERT INTO settings(key, value) VALUES('voice:strategy', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(&json)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn get_ignore_profile(&self) -> Result<IgnoreProfile, StorageError> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT excluded_voice_ids, excluded_locales FROM ignore_profile WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(IgnoreProfile::default()),
            Some((voice_ids_json, locales_json)) => {
                let excluded_voice_ids: Vec<VoiceId> =
                    serde_json::from_str(&voice_ids_json).map_err(StorageError::Serialization)?;
                let excluded_locales: Vec<String> =
                    serde_json::from_str(&locales_json).map_err(StorageError::Serialization)?;
                Ok(IgnoreProfile {
                    excluded_voice_ids,
                    excluded_locales,
                })
            }
        }
    }

    async fn set_ignore_profile(&self, profile: &IgnoreProfile) -> Result<(), StorageError> {
        let voice_ids_json = serde_json::to_string(&profile.excluded_voice_ids)
            .map_err(StorageError::Serialization)?;
        let locales_json = serde_json::to_string(&profile.excluded_locales)
            .map_err(StorageError::Serialization)?;
        sqlx::query(
            "INSERT INTO ignore_profile(id, excluded_voice_ids, excluded_locales, updated_at)
             VALUES(1, ?, ?, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                excluded_voice_ids = excluded.excluded_voice_ids,
                excluded_locales   = excluded.excluded_locales,
                updated_at         = excluded.updated_at",
        )
        .bind(&voice_ids_json)
        .bind(&locales_json)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::SqliteBackend;
    use forge_storage::DataProvider;
    use forge_voice::AliasState;

    async fn open() -> SqliteBackend {
        SqliteBackend::open_with_key(":memory:", [0xab; 32])
            .await
            .expect("open")
    }

    fn sample_alias() -> VoiceAlias {
        VoiceAlias {
            id: AliasId(ulid::Ulid::new().to_string()),
            viewer_id: "123456".into(),
            viewer_name: "testviewer".into(),
            engine_id: EngineId("piper".into()),
            voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
            pitch_semitones: None,
            rate_multiplier: None,
            state: AliasState::Active,
        }
    }

    #[tokio::test]
    async fn strategy_default_is_deterministic() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let strategy = repo.get_strategy().await.expect("get_strategy");
        assert_eq!(strategy, AssignmentStrategy::DeterministicByName);
    }

    #[tokio::test]
    async fn strategy_roundtrip() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let strategy = AssignmentStrategy::Single {
            voice_id: VoiceId("uk_UA-ukrainian-medium".into()),
            engine_id: EngineId("piper".into()),
        };
        repo.set_strategy(&strategy).await.expect("set_strategy");
        let back = repo.get_strategy().await.expect("get_strategy");
        assert_eq!(back, strategy);
    }

    #[tokio::test]
    async fn ignore_profile_default_is_empty() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let profile = repo.get_ignore_profile().await.expect("get_ignore_profile");
        assert!(profile.excluded_voice_ids.is_empty());
        assert!(profile.excluded_locales.is_empty());
    }

    #[tokio::test]
    async fn ignore_profile_roundtrip() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let profile = IgnoreProfile {
            excluded_voice_ids: vec![VoiceId("boring".into())],
            excluded_locales: vec!["de-DE".into()],
        };
        repo.set_ignore_profile(&profile).await.expect("set");
        let back = repo.get_ignore_profile().await.expect("get");
        assert_eq!(back.excluded_locales, vec!["de-DE".to_string()]);
    }

    #[tokio::test]
    async fn find_by_viewer_returns_none_for_unknown() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let result = repo.find_by_viewer("nobody").await.expect("find");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn upsert_and_find_by_viewer() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let alias = sample_alias();
        repo.upsert(&alias).await.expect("upsert");
        let found = repo
            .find_by_viewer(&alias.viewer_id)
            .await
            .expect("find")
            .expect("present");
        assert_eq!(found.viewer_id, alias.viewer_id);
        assert_eq!(found.engine_id.0, "piper");
    }

    #[tokio::test]
    async fn upsert_and_list() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let alias = sample_alias();
        repo.upsert(&alias).await.expect("upsert");
        let all = repo.list().await.expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].viewer_id, alias.viewer_id);
    }

    #[tokio::test]
    async fn delete_removes_alias() {
        let backend = open().await;
        let repo = backend.voice_alias_repo();
        let alias = sample_alias();
        repo.upsert(&alias).await.expect("upsert");
        repo.delete(&alias.id).await.expect("delete");
        let found = repo.find_by_viewer(&alias.viewer_id).await.expect("find");
        assert!(found.is_none());
    }
}
