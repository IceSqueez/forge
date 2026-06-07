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
    pub const SCRIPT_HTTP_ALLOWED_DOMAINS_KEY: &str = "script.http_allowed_domains";
    pub const SCRIPT_HTTP_MAX_CALLS_KEY: &str = "script.http_max_calls_per_script";
    pub const SCRIPT_HTTP_TIMEOUT_MS_KEY: &str = "script.http_timeout_ms";
    pub const SCRIPT_HTTP_ALLOW_LOCAL_KEY: &str = "script.http_allow_local";
    pub const SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY: &str = "script.http_max_response_bytes";
    pub const SCRIPT_OP_LIMIT_KEY: &str = "script.op_limit";
    pub const SCRIPT_TIMEOUT_MS_KEY: &str = "script.timeout_ms";
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait SettingsRepo: Send + Sync {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError>;
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError>;
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
        assert!(!SCRIPT_HTTP_ALLOWED_DOMAINS_KEY.is_empty());
        assert!(!SCRIPT_HTTP_MAX_CALLS_KEY.is_empty());
        assert!(!SCRIPT_HTTP_TIMEOUT_MS_KEY.is_empty());
        assert!(!SCRIPT_HTTP_ALLOW_LOCAL_KEY.is_empty());
        assert!(!SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY.is_empty());
        assert!(!SCRIPT_OP_LIMIT_KEY.is_empty());
        assert!(!SCRIPT_TIMEOUT_MS_KEY.is_empty());
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
            SCRIPT_HTTP_ALLOWED_DOMAINS_KEY,
            SCRIPT_HTTP_MAX_CALLS_KEY,
            SCRIPT_HTTP_TIMEOUT_MS_KEY,
            SCRIPT_HTTP_ALLOW_LOCAL_KEY,
            SCRIPT_HTTP_MAX_RESPONSE_BYTES_KEY,
            SCRIPT_OP_LIMIT_KEY,
            SCRIPT_TIMEOUT_MS_KEY,
        ];
        let unique: std::collections::HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len());
    }
}
