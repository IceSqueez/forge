use std::collections::HashMap;

use async_trait::async_trait;

use crate::StorageError;

pub mod reserved_keys {
    pub const ONBOARDING_COMPLETED: &str = "onboarding_completed";
    pub const LAST_ONBOARDING_STEP: &str = "last_onboarding_step";
    pub const THEME: &str = "theme";
    pub const ACCENT_COLOR: &str = "accent_color";
    pub const DENSITY: &str = "density";
    pub const FONT_BODY: &str = "font_body";
    pub const FONT_MONO: &str = "font_mono";
    pub const EVENT_LOG_RETENTION_DAYS_KEY: &str = "event_log_retention_days";
    pub const SERVER_BIND_ADDRESS_KEY: &str = "server.bind_address";
    pub const SERVER_PORT_KEY: &str = "server.port";
    pub const SERVER_LAN_BIND_ENABLED_KEY: &str = "server.lan_bind_enabled";
    pub const SERVER_AUTH_REQUIRED_FOR_READS_KEY: &str = "server.auth_required_for_reads";
    pub const SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY: &str = "server.http_overlay_require_token";
    pub const SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY: &str = "server.overlay_cors_any_origin";
    pub const SERVER_OVERLAY_ROOT_KEY: &str = "server.overlay_root";

    pub const TTS_ENGINE_DEFAULT: &str = "tts.engine_default";
    pub const TTS_MASTER_VOLUME: &str = "tts.master_volume";
    pub const TTS_MASTER_PITCH: &str = "tts.master_pitch";
    pub const TTS_MASTER_SPEED: &str = "tts.master_speed";
    pub const TTS_MAX_QUEUE_LEN: &str = "tts.max_queue_len";
    pub const TTS_MAX_PER_USER_PENDING: &str = "tts.max_per_user_pending";
    pub const TTS_LENGTH_CAP: &str = "tts.length_cap";
    pub const TTS_PIPELINE_CONFIG_JSON: &str = "tts.pipeline_config_json";
}

const VALID_BIND_ADDRESSES: &[&str] = &["127.0.0.1", "0.0.0.0", "::1", "::"];

#[async_trait]
pub trait SettingsRepo: Send + Sync {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError>;
    /// Returns `true` if the key was present and has been removed.
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError>;

    async fn event_log_retention_days(&self) -> Result<u32, StorageError> {
        let raw = self
            .get_string(reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY)
            .await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(7))
    }

    async fn set_event_log_retention_days(&self, days: u32) -> Result<(), StorageError> {
        self.set_string(
            reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY,
            &days.to_string(),
        )
        .await
    }

    async fn server_bind_address(&self) -> Result<String, StorageError> {
        let raw = self
            .get_string(reserved_keys::SERVER_BIND_ADDRESS_KEY)
            .await?;
        Ok(raw.unwrap_or_else(|| "127.0.0.1".to_owned()))
    }

    async fn set_server_bind_address(&self, addr: &str) -> Result<(), StorageError> {
        if !VALID_BIND_ADDRESSES.contains(&addr) {
            return Err(StorageError::ValidationFailed {
                field: "server.bind_address".to_owned(),
                reason: format!("must be one of: {}", VALID_BIND_ADDRESSES.join(", ")),
            });
        }
        self.set_string(reserved_keys::SERVER_BIND_ADDRESS_KEY, addr)
            .await
    }

    async fn server_port(&self) -> Result<u16, StorageError> {
        let raw = self.get_string(reserved_keys::SERVER_PORT_KEY).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(8081))
    }

    async fn set_server_port(&self, port: u16) -> Result<(), StorageError> {
        self.set_string(reserved_keys::SERVER_PORT_KEY, &port.to_string())
            .await
    }

    async fn server_lan_bind_enabled(&self) -> Result<bool, StorageError> {
        let raw = self
            .get_string(reserved_keys::SERVER_LAN_BIND_ENABLED_KEY)
            .await?;
        Ok(raw.as_deref().map(|s| s == "true").unwrap_or(false))
    }

    async fn set_server_lan_bind_enabled(&self, enabled: bool) -> Result<(), StorageError> {
        self.set_string(
            reserved_keys::SERVER_LAN_BIND_ENABLED_KEY,
            if enabled { "true" } else { "false" },
        )
        .await
    }

    async fn server_auth_required_for_reads(&self) -> Result<bool, StorageError> {
        let raw = self
            .get_string(reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS_KEY)
            .await?;
        Ok(raw.as_deref().map(|s| s == "true").unwrap_or(false))
    }

    async fn set_server_auth_required_for_reads(&self, required: bool) -> Result<(), StorageError> {
        self.set_string(
            reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS_KEY,
            if required { "true" } else { "false" },
        )
        .await
    }

    async fn server_http_overlay_require_token(&self) -> Result<bool, StorageError> {
        let raw = self
            .get_string(reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY)
            .await?;
        Ok(raw.as_deref().map(|s| s == "true").unwrap_or(false))
    }

    async fn server_overlay_cors_any_origin(&self) -> Result<bool, StorageError> {
        let raw = self
            .get_string(reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY)
            .await?;
        Ok(raw.as_deref().map(|s| s == "true").unwrap_or(true))
    }

    async fn server_overlay_root(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::SERVER_OVERLAY_ROOT_KEY)
            .await
    }

    async fn tts_engine_default(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::TTS_ENGINE_DEFAULT).await
    }

    async fn set_tts_engine_default(&self, engine_id: &str) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_ENGINE_DEFAULT, engine_id)
            .await
    }

    async fn tts_master_volume(&self) -> Result<f32, StorageError> {
        let raw = self.get_string(reserved_keys::TTS_MASTER_VOLUME).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(1.0))
    }

    async fn set_tts_master_volume(&self, volume: f32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_MASTER_VOLUME, &volume.to_string())
            .await
    }

    async fn tts_master_pitch(&self) -> Result<f32, StorageError> {
        let raw = self.get_string(reserved_keys::TTS_MASTER_PITCH).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0.0))
    }

    async fn set_tts_master_pitch(&self, semitones: f32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_MASTER_PITCH, &semitones.to_string())
            .await
    }

    async fn tts_master_speed(&self) -> Result<f32, StorageError> {
        let raw = self.get_string(reserved_keys::TTS_MASTER_SPEED).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(1.0))
    }

    async fn set_tts_master_speed(&self, multiplier: f32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_MASTER_SPEED, &multiplier.to_string())
            .await
    }

    async fn tts_max_queue_len(&self) -> Result<u32, StorageError> {
        let raw = self.get_string(reserved_keys::TTS_MAX_QUEUE_LEN).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(100))
    }

    async fn set_tts_max_queue_len(&self, len: u32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_MAX_QUEUE_LEN, &len.to_string())
            .await
    }

    async fn tts_max_per_user_pending(&self) -> Result<u32, StorageError> {
        let raw = self
            .get_string(reserved_keys::TTS_MAX_PER_USER_PENDING)
            .await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(5))
    }

    async fn set_tts_max_per_user_pending(&self, limit: u32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_MAX_PER_USER_PENDING, &limit.to_string())
            .await
    }

    async fn tts_length_cap(&self) -> Result<u32, StorageError> {
        let raw = self.get_string(reserved_keys::TTS_LENGTH_CAP).await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(500))
    }

    async fn set_tts_length_cap(&self, cap: u32) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_LENGTH_CAP, &cap.to_string())
            .await
    }

    async fn tts_pipeline_config_json(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::TTS_PIPELINE_CONFIG_JSON)
            .await
    }

    async fn set_tts_pipeline_config_json(&self, json: &str) -> Result<(), StorageError> {
        self.set_string(reserved_keys::TTS_PIPELINE_CONFIG_JSON, json)
            .await
    }

    async fn sheet_width(&self, key: &str) -> Result<Option<f32>, StorageError> {
        let storage_key = format!("sheet_width:{key}");
        let raw = self.get_string(&storage_key).await?;
        match raw {
            None => Ok(None),
            Some(s) => match s.parse::<f32>() {
                Ok(v) => Ok(Some(v)),
                Err(_) => {
                    tracing::warn!(key, raw = %s, "sheet_width: stored value is not a valid f32; falling back to None");
                    Ok(None)
                }
            },
        }
    }

    async fn set_sheet_width(&self, key: &str, width: f32) -> Result<(), StorageError> {
        let storage_key = format!("sheet_width:{key}");
        self.set_string(&storage_key, &width.to_string()).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::reserved_keys::*;

    fn _trait_is_dyn_safe(_: &dyn super::SettingsRepo) {}

    #[test]
    fn reserved_keys_are_non_empty() {
        assert!(!ONBOARDING_COMPLETED.is_empty());
        assert!(!LAST_ONBOARDING_STEP.is_empty());
        assert!(!THEME.is_empty());
        assert!(!ACCENT_COLOR.is_empty());
        assert!(!DENSITY.is_empty());
        assert!(!FONT_BODY.is_empty());
        assert!(!FONT_MONO.is_empty());
        assert!(!EVENT_LOG_RETENTION_DAYS_KEY.is_empty());
        assert!(!SERVER_BIND_ADDRESS_KEY.is_empty());
        assert!(!SERVER_PORT_KEY.is_empty());
        assert!(!SERVER_LAN_BIND_ENABLED_KEY.is_empty());
        assert!(!SERVER_AUTH_REQUIRED_FOR_READS_KEY.is_empty());
        assert!(!SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY.is_empty());
        assert!(!SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY.is_empty());
        assert!(!SERVER_OVERLAY_ROOT_KEY.is_empty());
        assert!(!TTS_ENGINE_DEFAULT.is_empty());
        assert!(!TTS_MASTER_VOLUME.is_empty());
        assert!(!TTS_MASTER_PITCH.is_empty());
        assert!(!TTS_MASTER_SPEED.is_empty());
        assert!(!TTS_MAX_QUEUE_LEN.is_empty());
        assert!(!TTS_MAX_PER_USER_PENDING.is_empty());
        assert!(!TTS_LENGTH_CAP.is_empty());
        assert!(!TTS_PIPELINE_CONFIG_JSON.is_empty());
    }

    #[test]
    fn reserved_keys_are_distinct() {
        let keys = [
            ONBOARDING_COMPLETED,
            LAST_ONBOARDING_STEP,
            THEME,
            ACCENT_COLOR,
            DENSITY,
            FONT_BODY,
            FONT_MONO,
            EVENT_LOG_RETENTION_DAYS_KEY,
            SERVER_BIND_ADDRESS_KEY,
            SERVER_PORT_KEY,
            SERVER_LAN_BIND_ENABLED_KEY,
            SERVER_AUTH_REQUIRED_FOR_READS_KEY,
            SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_KEY,
            SERVER_OVERLAY_CORS_ANY_ORIGIN_KEY,
            SERVER_OVERLAY_ROOT_KEY,
            TTS_ENGINE_DEFAULT,
            TTS_MASTER_VOLUME,
            TTS_MASTER_PITCH,
            TTS_MASTER_SPEED,
            TTS_MAX_QUEUE_LEN,
            TTS_MAX_PER_USER_PENDING,
            TTS_LENGTH_CAP,
            TTS_PIPELINE_CONFIG_JSON,
        ];
        let unique: std::collections::HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len());
    }
}
