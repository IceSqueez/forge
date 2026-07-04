use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

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
    pub const SERVER_ENABLED_KEY: &str = "server.enabled";
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
    pub const CORE_HTTP_ALLOW_LOCAL_KEY: &str = "core.http_allow_local";
    pub const SCRIPT_OP_LIMIT_KEY: &str = "script.op_limit";
    pub const SCRIPT_TIMEOUT_MS_KEY: &str = "script.timeout_ms";
    pub const LANGUAGE: &str = "app.language";
    pub const KEYBOARD_SHORTCUTS: &str = "app.keyboard_shortcuts";
    pub const AUDIO_OUTPUT_DEVICE_ID_KEY: &str = "audio.output_device_id";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    En,
    Uk,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Language::En => f.write_str("en"),
            Language::Uk => f.write_str("uk"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownLanguage(pub String);

impl fmt::Display for UnknownLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown language code: {}", self.0)
    }
}

impl FromStr for Language {
    type Err = UnknownLanguage;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "en" => Ok(Language::En),
            "uk" => Ok(Language::Uk),
            other => Err(UnknownLanguage(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Spacious,
}

impl fmt::Display for Density {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Density::Compact => f.write_str("compact"),
            Density::Cozy => f.write_str("cozy"),
            Density::Spacious => f.write_str("spacious"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnknownDensity(pub String);

impl fmt::Display for UnknownDensity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown density: {}", self.0)
    }
}

impl FromStr for Density {
    type Err = UnknownDensity;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "compact" => Ok(Density::Compact),
            "cozy" => Ok(Density::Cozy),
            "spacious" => Ok(Density::Spacious),
            other => Err(UnknownDensity(other.to_owned())),
        }
    }
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait SettingsRepo: Send + Sync {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError>;
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError>;

    /// Absent key returns `Language::En` (first-run default before migration seed reaches storage).
    async fn language(&self) -> Result<Language, StorageError> {
        match self.get_string(reserved_keys::LANGUAGE).await? {
            Some(s) => s
                .parse()
                .map_err(|e: UnknownLanguage| StorageError::Parse(e.to_string())),
            None => Ok(Language::default()),
        }
    }

    async fn set_language(&self, lang: Language) -> Result<(), StorageError> {
        self.set_string(reserved_keys::LANGUAGE, &lang.to_string())
            .await
    }

    /// Absent or corrupt key silently returns `Density::Cozy` (default).
    async fn density(&self) -> Result<Density, StorageError> {
        match self.get_string(reserved_keys::DENSITY).await? {
            Some(s) => Ok(s.parse().unwrap_or_default()),
            None => Ok(Density::default()),
        }
    }

    async fn set_density(&self, density: Density) -> Result<(), StorageError> {
        self.set_string(reserved_keys::DENSITY, &density.to_string())
            .await
    }

    /// Returns stored font family name for interface (body) text, or None if unset (bundled default applies).
    async fn font_body(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::FONT_BODY).await
    }

    /// Sets interface (body) font family name, or passes None to unset and use bundled default.
    async fn set_font_body(&self, name: Option<String>) -> Result<(), StorageError> {
        match name {
            Some(family) => self.set_string(reserved_keys::FONT_BODY, &family).await,
            None => {
                self.delete(reserved_keys::FONT_BODY).await?;
                Ok(())
            }
        }
    }

    /// Returns stored font family name for monospace (code) text, or None if unset (bundled default applies).
    async fn font_mono(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::FONT_MONO).await
    }

    /// Sets monospace (code) font family name, or passes None to unset and use bundled default.
    async fn set_font_mono(&self, name: Option<String>) -> Result<(), StorageError> {
        match name {
            Some(family) => self.set_string(reserved_keys::FONT_MONO, &family).await,
            None => {
                self.delete(reserved_keys::FONT_MONO).await?;
                Ok(())
            }
        }
    }

    /// Returns the persisted TTS output device id (opaque, backend-defined string), or
    /// None if unset (OS default device applies).
    async fn audio_output_device_id(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::AUDIO_OUTPUT_DEVICE_ID_KEY)
            .await
    }

    /// Sets the persisted TTS output device id, or pass None to clear the preference
    /// and fall back to the OS default device.
    async fn set_audio_output_device_id(
        &self,
        device_id: Option<String>,
    ) -> Result<(), StorageError> {
        match device_id {
            Some(id) => {
                self.set_string(reserved_keys::AUDIO_OUTPUT_DEVICE_ID_KEY, &id)
                    .await
            }
            None => {
                self.delete(reserved_keys::AUDIO_OUTPUT_DEVICE_ID_KEY)
                    .await?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Density, Language};

    fn _trait_is_dyn_safe(_: &dyn super::SettingsRepo) {}

    #[test]
    fn language_round_trips_through_display_and_from_str() {
        assert_eq!(Language::default(), Language::En);
        for lang in [Language::En, Language::Uk] {
            let s = lang.to_string();
            assert_eq!(s.parse::<Language>().unwrap(), lang);
        }
    }

    #[test]
    fn language_from_str_rejects_non_canonical_codes_preserving_input() {
        for bad in ["EN", "En", "fr", "", " en", "en "] {
            let err = bad.parse::<Language>().unwrap_err();
            assert_eq!(err.0, bad);
        }
    }

    #[test]
    fn density_round_trips_through_display_and_from_str() {
        // Cozy is the product default — display strings are the persisted format.
        assert_eq!(Density::default(), Density::Cozy);
        for density in [Density::Compact, Density::Cozy, Density::Spacious] {
            let s = density.to_string();
            assert_eq!(s.parse::<Density>().unwrap(), density);
        }
    }

    #[test]
    fn density_from_str_rejects_non_canonical_values_preserving_input() {
        for bad in ["Cozy", "COMPACT", "dense", "", " cozy"] {
            let err = bad.parse::<Density>().unwrap_err();
            assert_eq!(err.0, bad);
        }
    }
}
