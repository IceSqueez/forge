use async_trait::async_trait;
use forge_storage::{
    OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo, StorageError,
};
use rand::rand_core::Rng;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

#[derive(sqlx::FromRow)]
struct OverlayRow {
    id: String,
    display_name: String,
    kind_id: String,
    enabled: i64,
    position: i64,
    config: String,
    config_schema_version: i64,
    generator_version: i64,
    source_overrides: String,
    credential: String,
    created_at: i64,
    updated_at: i64,
}

fn decode_row(row: OverlayRow) -> Result<OverlayDefinition, SqliteStorageError> {
    let config: OverlayConfig = serde_json::from_str(&row.config)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid overlay config json: {e}")))?;
    let source_overrides: Vec<String> = serde_json::from_str(&row.source_overrides)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid source_overrides json: {e}")))?;

    Ok(OverlayDefinition {
        id: OverlayId::new(row.id),
        display_name: row.display_name,
        kind_id: row.kind_id,
        enabled: row.enabled != 0,
        position: row.position,
        config,
        config_schema_version: row.config_schema_version.max(0) as u32,
        generator_version: row.generator_version.max(0) as u32,
        source_overrides,
        credential: OverlayCredential::new(row.credential),
        created_at: from_epoch_ms(row.created_at)?,
        updated_at: from_epoch_ms(row.updated_at)?,
    })
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

fn epoch_ms_now() -> i64 {
    to_epoch_ms(OffsetDateTime::now_utc())
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut prev_hyphen = true;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen {
            slug.push('-');
            prev_hyphen = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "overlay".to_string()
    } else {
        slug
    }
}

fn generate_credential() -> OverlayCredential {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    OverlayCredential::new(hex)
}

pub struct SqliteOverlayRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteOverlayRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    async fn mint_unique_id(&self, base: &str) -> Result<OverlayId, StorageError> {
        let base_slug = slugify(base);
        let mut candidate = base_slug.clone();
        let mut suffix: u32 = 1;

        loop {
            let existing: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM overlays WHERE id = ?")
                .bind(&candidate)
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

            if existing.is_none() {
                return Ok(OverlayId::new(candidate));
            }

            suffix += 1;
            candidate = format!("{base_slug}-{suffix}");
        }
    }

    async fn next_position(&self) -> Result<i64, StorageError> {
        let (max,): (Option<i64>,) = sqlx::query_as("SELECT MAX(position) FROM overlays")
            .fetch_one(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(max.map(|m| m + 1).unwrap_or(0))
    }
}

#[async_trait]
impl OverlayRepo for SqliteOverlayRepo {
    async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
        let rows: Vec<OverlayRow> = sqlx::query_as(
            "SELECT id, display_name, kind_id, enabled, position, config,
                    config_schema_version, generator_version, source_overrides,
                    credential, created_at, updated_at
             FROM overlays
             ORDER BY position ASC, display_name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
        let row: Option<OverlayRow> = sqlx::query_as(
            "SELECT id, display_name, kind_id, enabled, position, config,
                    config_schema_version, generator_version, source_overrides,
                    credential, created_at, updated_at
             FROM overlays WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_credential(
        &self,
        credential: &OverlayCredential,
    ) -> Result<Option<OverlayDefinition>, StorageError> {
        let row: Option<OverlayRow> = sqlx::query_as(
            "SELECT id, display_name, kind_id, enabled, position, config,
                    config_schema_version, generator_version, source_overrides,
                    credential, created_at, updated_at
             FROM overlays WHERE credential = ?",
        )
        .bind(credential.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn create(
        &self,
        display_name: &str,
        kind_id: &str,
        config_schema_version: u32,
    ) -> Result<OverlayDefinition, StorageError> {
        let id = self.mint_unique_id(display_name).await?;
        let position = self.next_position().await?;
        let credential = generate_credential();
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO overlays
                 (id, display_name, kind_id, enabled, position, config,
                  config_schema_version, generator_version, source_overrides,
                  credential, created_at, updated_at)
             VALUES (?, ?, ?, 1, ?, '{}', ?, 0, '[]', ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(display_name)
        .bind(kind_id)
        .bind(position)
        .bind(config_schema_version as i64)
        .bind(credential.as_str())
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(OverlayDefinition {
            id,
            display_name: display_name.to_string(),
            kind_id: kind_id.to_string(),
            enabled: true,
            position,
            config: OverlayConfig::new(),
            config_schema_version,
            generator_version: 0,
            source_overrides: Vec::new(),
            credential,
            created_at: from_epoch_ms(now_ms)?,
            updated_at: from_epoch_ms(now_ms)?,
        })
    }

    async fn save(&self, definition: &OverlayDefinition) -> Result<(), StorageError> {
        let config_json =
            serde_json::to_string(&definition.config).map_err(StorageError::Serialization)?;
        let source_overrides_json = serde_json::to_string(&definition.source_overrides)
            .map_err(StorageError::Serialization)?;
        let enabled: i64 = if definition.enabled { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        let result = sqlx::query(
            "UPDATE overlays SET
                 display_name          = ?,
                 kind_id               = ?,
                 enabled               = ?,
                 position              = ?,
                 config                = ?,
                 config_schema_version = ?,
                 generator_version     = ?,
                 source_overrides      = ?,
                 credential            = ?,
                 updated_at            = ?
             WHERE id = ?",
        )
        .bind(&definition.display_name)
        .bind(&definition.kind_id)
        .bind(enabled)
        .bind(definition.position)
        .bind(&config_json)
        .bind(definition.config_schema_version as i64)
        .bind(definition.generator_version as i64)
        .bind(&source_overrides_json)
        .bind(definition.credential.as_str())
        .bind(now_ms)
        .bind(definition.id.as_str())
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound {
                key: definition.id.as_str().to_owned(),
            });
        }
        Ok(())
    }

    async fn set_enabled(&self, id: &OverlayId, enabled: bool) -> Result<bool, StorageError> {
        let enabled_val: i64 = if enabled { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        let result = sqlx::query("UPDATE overlays SET enabled = ?, updated_at = ? WHERE id = ?")
            .bind(enabled_val)
            .bind(now_ms)
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete(&self, id: &OverlayId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM overlays WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_retained_content(
        &self,
        id: &OverlayId,
    ) -> Result<Option<OverlayConfig>, StorageError> {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT retained_content FROM overlays WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        let Some(json) = row.and_then(|(content,)| content) else {
            return Ok(None);
        };

        let content: OverlayConfig = serde_json::from_str(&json).map_err(|e| {
            SqliteStorageError::Decode(format!("invalid retained_content json: {e}"))
        })?;
        Ok(Some(content))
    }

    async fn set_retained_content(
        &self,
        id: &OverlayId,
        content: &OverlayConfig,
    ) -> Result<(), StorageError> {
        let json = serde_json::to_string(content).map_err(StorageError::Serialization)?;

        let result = sqlx::query("UPDATE overlays SET retained_content = ? WHERE id = ?")
            .bind(&json)
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            });
        }
        Ok(())
    }
}
