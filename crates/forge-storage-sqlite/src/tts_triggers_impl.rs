use async_trait::async_trait;
use forge_storage::{StorageError, TtsTriggerSettings, TtsTriggerSettingsRepo};

use crate::error::SqliteStorageError;

pub struct SqliteTtsTriggerSettingsRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteTtsTriggerSettingsRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

type SettingsRow = (i64, i64, i64, i64, i64, i64, i64);

fn decode_settings_row(row: SettingsRow) -> TtsTriggerSettings {
    let (
        command_enabled,
        channel_points_enabled,
        bits_enabled,
        sub_messages_enabled,
        read_username,
        speak_emotes,
        bits_skip_line,
    ) = row;
    TtsTriggerSettings {
        command_enabled: command_enabled != 0,
        channel_points_enabled: channel_points_enabled != 0,
        bits_enabled: bits_enabled != 0,
        sub_messages_enabled: sub_messages_enabled != 0,
        read_username: read_username != 0,
        speak_emotes: speak_emotes != 0,
        bits_skip_line: bits_skip_line != 0,
    }
}

#[async_trait]
impl TtsTriggerSettingsRepo for SqliteTtsTriggerSettingsRepo {
    async fn get_trigger_settings(&self) -> Result<TtsTriggerSettings, StorageError> {
        let row: Option<SettingsRow> = sqlx::query_as(
            "SELECT command_enabled, channel_points_enabled, bits_enabled,
                    sub_messages_enabled, read_username, speak_emotes, bits_skip_line
             FROM tts_trigger_settings WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(decode_settings_row).unwrap_or_default())
    }

    async fn set_trigger_settings(
        &self,
        settings: &TtsTriggerSettings,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO tts_trigger_settings
                (id, command_enabled, channel_points_enabled, bits_enabled,
                 sub_messages_enabled, read_username, speak_emotes, bits_skip_line)
             VALUES (1, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                command_enabled        = excluded.command_enabled,
                channel_points_enabled = excluded.channel_points_enabled,
                bits_enabled           = excluded.bits_enabled,
                sub_messages_enabled   = excluded.sub_messages_enabled,
                read_username          = excluded.read_username,
                speak_emotes           = excluded.speak_emotes,
                bits_skip_line         = excluded.bits_skip_line",
        )
        .bind(settings.command_enabled as i64)
        .bind(settings.channel_points_enabled as i64)
        .bind(settings.bits_enabled as i64)
        .bind(settings.sub_messages_enabled as i64)
        .bind(settings.read_username as i64)
        .bind(settings.speak_emotes as i64)
        .bind(settings.bits_skip_line as i64)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }
}
