use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use forge_types::LogLevel;
use forge_voice::SynthesisDefaults;
use serde::{Deserialize, Serialize};

use crate::StorageError;

pub mod reserved_keys {
    pub const THEME: &str = "theme";
    pub const DENSITY: &str = "density";
    pub const FONT_BODY: &str = "font_body";
    pub const FONT_MONO: &str = "font_mono";
    pub const EVENT_LOG_RETENTION_DAYS: &str = "event_log_retention_days";
    pub const SERVER_ENABLED: &str = "server.enabled";
    pub const SERVER_BIND_ADDRESS: &str = "server.bind_address";
    pub const SERVER_PORT: &str = "server.port";
    pub const SERVER_LAN_BIND_ENABLED: &str = "server.lan_bind_enabled";
    pub const SERVER_AUTH_REQUIRED_FOR_READS: &str = "server.auth_required_for_reads";
    pub const SERVER_HTTP_OVERLAY_REQUIRE_TOKEN: &str = "server.http_overlay_require_token";
    pub const SERVER_OVERLAY_CORS_ANY_ORIGIN: &str = "server.overlay_cors_any_origin";
    pub const SERVER_OVERLAY_ROOT: &str = "server.overlay_root";
    pub const SERVER_ADDITIONAL_ORIGINS: &str = "server.additional_origins";
    pub const SCRIPT_HTTP_ALLOWED_DOMAINS: &str = "script.http_allowed_domains";
    pub const SCRIPT_HTTP_MAX_CALLS: &str = "script.http_max_calls_per_script";
    pub const SCRIPT_HTTP_TIMEOUT_MS: &str = "script.http_timeout_ms";
    pub const SCRIPT_HTTP_ALLOW_LOCAL: &str = "script.http_allow_local";
    pub const SCRIPT_HTTP_MAX_RESPONSE_BYTES: &str = "script.http_max_response_bytes";
    pub const CORE_HTTP_ALLOW_LOCAL: &str = "core.http_allow_local";
    pub const SCRIPT_OP_LIMIT: &str = "script.op_limit";
    pub const SCRIPT_TIMEOUT_MS: &str = "script.timeout_ms";
    pub const LANGUAGE: &str = "app.language";
    pub const KEYBOARD_SHORTCUTS: &str = "app.keyboard_shortcuts";
    pub const AUDIO_OUTPUT_DEVICE_ID: &str = "audio.output_device_id";
    pub const AUDIO_VOICE_GATE_ENABLED: &str = "audio.voice_gate_enabled";
    pub const AUDIO_VOICE_GATE_INPUT_DEVICE_ID: &str = "audio.voice_gate_input_device_id";
    pub const AUDIO_VOICE_GATE_THRESHOLD: &str = "audio.voice_gate_threshold";
    pub const AUDIO_VOICE_GATE_HOLD_MS: &str = "audio.voice_gate_hold_ms";
    pub const CHAT_HISTORY_STORE_LIMIT: &str = "chat_history.store_limit";
    pub const CHAT_HISTORY_DISPLAY_LIMIT: &str = "chat_history.display_limit";
    pub const PICKER_FAVORITES_SUB_ACTIONS: &str = "picker.favorites.sub_actions";
    pub const PICKER_FAVORITES_TRIGGERS: &str = "picker.favorites.triggers";
    pub const TTS_DISABLED_ENGINES: &str = "tts.disabled_engines";
    pub const TTS_SYNTHESIS_DEFAULTS: &str = "tts.synthesis_defaults";
    pub const TTS_MASTER_VOLUME: &str = "tts.master_volume";
    pub const TTS_ENGINE_PARAMS_PREFIX: &str = "tts.engine_params.";
    pub const SOUNDBOARD_ENABLED: &str = "soundboard.enabled";
    pub const SOUNDBOARD_OUTPUT_DEVICE: &str = "soundboard.output_device";
    pub const SOUNDBOARD_MASTER_VOLUME: &str = "soundboard.master_volume";
    pub const SOUNDBOARD_ALSO_HEADPHONES: &str = "soundboard.also_headphones";
    pub const DIAGNOSTICS_LOG_LEVEL: &str = "diagnostics.log_level";
}

/// How each settings key may appear in the publicly-attachable diagnostic bundle.
pub mod disclosure {
    use std::collections::{BTreeMap, HashMap};

    use forge_types::redaction::STAMP;

    use super::reserved_keys;

    /// A key with no entry in `class_of` is `Withheld`, so a key added tomorrow costs a diagnostic, never a leak.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SettingDisclosure {
        Verbatim,
        Kind,
        Presence,
        Withheld,
    }

    /// `Verbatim` is reserved for values that cannot name a person, a channel, a host or a path.
    pub fn class_of(key: &str) -> SettingDisclosure {
        use SettingDisclosure::{Kind, Presence, Verbatim, Withheld};

        if key.starts_with(reserved_keys::TTS_ENGINE_PARAMS_PREFIX) {
            return Verbatim;
        }

        match key {
            reserved_keys::THEME
            | reserved_keys::DENSITY
            | reserved_keys::LANGUAGE
            | reserved_keys::DIAGNOSTICS_LOG_LEVEL
            | reserved_keys::EVENT_LOG_RETENTION_DAYS
            | reserved_keys::SERVER_ENABLED
            | reserved_keys::SERVER_PORT
            | reserved_keys::SERVER_LAN_BIND_ENABLED
            | reserved_keys::SERVER_AUTH_REQUIRED_FOR_READS
            | reserved_keys::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN
            | reserved_keys::SERVER_OVERLAY_CORS_ANY_ORIGIN
            | reserved_keys::SCRIPT_HTTP_MAX_CALLS
            | reserved_keys::SCRIPT_HTTP_TIMEOUT_MS
            | reserved_keys::SCRIPT_HTTP_ALLOW_LOCAL
            | reserved_keys::SCRIPT_HTTP_MAX_RESPONSE_BYTES
            | reserved_keys::SCRIPT_OP_LIMIT
            | reserved_keys::SCRIPT_TIMEOUT_MS
            | reserved_keys::CORE_HTTP_ALLOW_LOCAL
            | reserved_keys::AUDIO_VOICE_GATE_ENABLED
            | reserved_keys::AUDIO_VOICE_GATE_THRESHOLD
            | reserved_keys::AUDIO_VOICE_GATE_HOLD_MS
            | reserved_keys::CHAT_HISTORY_STORE_LIMIT
            | reserved_keys::CHAT_HISTORY_DISPLAY_LIMIT
            | reserved_keys::TTS_DISABLED_ENGINES
            | reserved_keys::TTS_SYNTHESIS_DEFAULTS
            | reserved_keys::TTS_MASTER_VOLUME
            | reserved_keys::SOUNDBOARD_ENABLED
            | reserved_keys::SOUNDBOARD_MASTER_VOLUME
            | reserved_keys::SOUNDBOARD_ALSO_HEADPHONES => Verbatim,

            reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS
            | reserved_keys::SERVER_ADDITIONAL_ORIGINS
            | reserved_keys::KEYBOARD_SHORTCUTS
            | reserved_keys::PICKER_FAVORITES_SUB_ACTIONS
            | reserved_keys::PICKER_FAVORITES_TRIGGERS => Kind,

            reserved_keys::SERVER_BIND_ADDRESS
            | reserved_keys::SERVER_OVERLAY_ROOT
            | reserved_keys::FONT_BODY
            | reserved_keys::FONT_MONO
            | reserved_keys::AUDIO_OUTPUT_DEVICE_ID
            | reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID
            | reserved_keys::SOUNDBOARD_OUTPUT_DEVICE => Presence,

            _ => Withheld,
        }
    }

    /// `None` keeps the key out of the bundle entirely.
    pub fn render(key: &str, value: &str) -> Option<String> {
        match class_of(key) {
            SettingDisclosure::Verbatim => Some(value.to_owned()),
            SettingDisclosure::Kind => Some(shape_of(value)),
            SettingDisclosure::Presence => {
                Some(if value.is_empty() { "unset" } else { "set" }.to_owned())
            }
            SettingDisclosure::Withheld => None,
        }
    }

    /// Ordered by key so two bundles from the same install diff cleanly.
    pub fn render_all(stored: &HashMap<String, String>) -> BTreeMap<String, String> {
        stored
            .iter()
            .filter_map(|(key, value)| render(key, value).map(|shown| (key.clone(), shown)))
            .collect()
    }

    fn shape_of(value: &str) -> String {
        match serde_json::from_str::<serde_json::Value>(value) {
            Ok(serde_json::Value::Array(items)) => format!("list({})", items.len()),
            Ok(serde_json::Value::Object(fields)) => format!("map({})", fields.len()),
            _ => format!("{STAMP} len={}>", value.chars().count()),
        }
    }
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

    async fn get_theme(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::THEME).await
    }

    async fn set_theme(&self, theme_key: &str) -> Result<(), StorageError> {
        self.set_string(reserved_keys::THEME, theme_key).await
    }

    async fn font_body(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::FONT_BODY).await
    }

    /// `None` clears the override (not a no-op) and falls back to the bundled default.
    async fn set_font_body(&self, name: Option<String>) -> Result<(), StorageError> {
        match name {
            Some(family) => self.set_string(reserved_keys::FONT_BODY, &family).await,
            None => {
                self.delete(reserved_keys::FONT_BODY).await?;
                Ok(())
            }
        }
    }

    async fn font_mono(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::FONT_MONO).await
    }

    /// `None` clears the override (not a no-op) and falls back to the bundled default.
    async fn set_font_mono(&self, name: Option<String>) -> Result<(), StorageError> {
        match name {
            Some(family) => self.set_string(reserved_keys::FONT_MONO, &family).await,
            None => {
                self.delete(reserved_keys::FONT_MONO).await?;
                Ok(())
            }
        }
    }

    async fn audio_output_device_id(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::AUDIO_OUTPUT_DEVICE_ID).await
    }

    /// `None` clears the preference (not a no-op) and falls back to the OS default device.
    async fn set_audio_output_device_id(
        &self,
        device_id: Option<String>,
    ) -> Result<(), StorageError> {
        match device_id {
            Some(id) => {
                self.set_string(reserved_keys::AUDIO_OUTPUT_DEVICE_ID, &id)
                    .await
            }
            None => {
                self.delete(reserved_keys::AUDIO_OUTPUT_DEVICE_ID).await?;
                Ok(())
            }
        }
    }
}

fn decode_bool_setting(s: &str) -> Option<bool> {
    if s == "1" || s.eq_ignore_ascii_case("true") {
        Some(true)
    } else if s == "0" || s.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

pub async fn get_bool_setting(repo: &dyn SettingsRepo, key: &str, default: bool) -> bool {
    match repo.get_string(key).await {
        Ok(Some(s)) => decode_bool_setting(&s).unwrap_or(default),
        _ => default,
    }
}

pub async fn set_bool_setting(
    repo: &dyn SettingsRepo,
    key: &str,
    value: bool,
) -> Result<(), StorageError> {
    repo.set_string(key, if value { "true" } else { "false" })
        .await
}

/// Absent key, read error, or malformed JSON all yield `None`; callers apply their default.
pub async fn get_json_setting<T: serde::de::DeserializeOwned>(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Option<T> {
    match repo.get_string(key).await {
        Ok(Some(s)) => serde_json::from_str(&s).ok(),
        _ => None,
    }
}

pub async fn set_json_setting<T: Serialize>(
    repo: &dyn SettingsRepo,
    key: &str,
    value: &T,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(value).map_err(|e| StorageError::Parse(e.to_string()))?;
    repo.set_string(key, &json).await
}

pub async fn chat_history_store_limit(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::CHAT_HISTORY_STORE_LIMIT)
        .await?;
    Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(5000))
}

pub async fn set_chat_history_store_limit(
    repo: &dyn SettingsRepo,
    limit: u32,
) -> Result<(), StorageError> {
    repo.set_string(reserved_keys::CHAT_HISTORY_STORE_LIMIT, &limit.to_string())
        .await
}

pub async fn disabled_tts_engines(repo: &dyn SettingsRepo) -> Result<Vec<String>, StorageError> {
    let raw = repo.get_string(reserved_keys::TTS_DISABLED_ENGINES).await?;
    Ok(raw
        .as_deref()
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default())
}

pub async fn set_disabled_tts_engines(
    repo: &dyn SettingsRepo,
    engine_ids: &[String],
) -> Result<(), StorageError> {
    let json = serde_json::to_string(engine_ids).map_err(|e| StorageError::Parse(e.to_string()))?;
    repo.set_string(reserved_keys::TTS_DISABLED_ENGINES, &json)
        .await
}

pub async fn synthesis_defaults(
    repo: &dyn SettingsRepo,
) -> Result<SynthesisDefaults, StorageError> {
    let raw = repo
        .get_string(reserved_keys::TTS_SYNTHESIS_DEFAULTS)
        .await?;
    Ok(raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EngineParams {
    pub pitch_semitones: f32,
    pub rate_multiplier: f32,
    pub gain: f32,
}

impl Default for EngineParams {
    fn default() -> Self {
        Self {
            pitch_semitones: 0.0,
            rate_multiplier: 1.0,
            gain: 1.0,
        }
    }
}

pub async fn engine_params(
    repo: &dyn SettingsRepo,
    engine_id: &str,
) -> Result<EngineParams, StorageError> {
    let key = format!("{}{engine_id}", reserved_keys::TTS_ENGINE_PARAMS_PREFIX);
    let raw = repo.get_string(&key).await?;
    Ok(raw
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default())
}

pub async fn set_engine_params(
    repo: &dyn SettingsRepo,
    engine_id: &str,
    params: EngineParams,
) -> Result<(), StorageError> {
    let key = format!("{}{engine_id}", reserved_keys::TTS_ENGINE_PARAMS_PREFIX);
    let json = serde_json::to_string(&params).map_err(|e| StorageError::Parse(e.to_string()))?;
    repo.set_string(&key, &json).await
}

pub async fn master_volume(repo: &dyn SettingsRepo) -> Result<f32, StorageError> {
    let raw = repo.get_string(reserved_keys::TTS_MASTER_VOLUME).await?;
    Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(1.0))
}

pub async fn set_master_volume(repo: &dyn SettingsRepo, volume: f32) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::TTS_MASTER_VOLUME,
        &volume.clamp(0.0, 1.0).to_string(),
    )
    .await
}

pub const VOICE_GATE_DEFAULT_THRESHOLD: f32 = 0.15;
pub const VOICE_GATE_DEFAULT_HOLD_MS: u32 = 800;

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceGateSettings {
    pub enabled: bool,
    pub input_device_id: Option<String>,
    /// Linear peak amplitude in 0.0..=1.0, not decibels.
    pub threshold: f32,
    pub hold_ms: u32,
}

impl Default for VoiceGateSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            input_device_id: None,
            threshold: VOICE_GATE_DEFAULT_THRESHOLD,
            hold_ms: VOICE_GATE_DEFAULT_HOLD_MS,
        }
    }
}

pub async fn voice_gate_settings(
    repo: &dyn SettingsRepo,
) -> Result<VoiceGateSettings, StorageError> {
    let defaults = VoiceGateSettings::default();
    let enabled = get_bool_setting(
        repo,
        reserved_keys::AUDIO_VOICE_GATE_ENABLED,
        defaults.enabled,
    )
    .await;
    let input_device_id = repo
        .get_string(reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID)
        .await?;
    let threshold = repo
        .get_string(reserved_keys::AUDIO_VOICE_GATE_THRESHOLD)
        .await?
        .as_deref()
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(defaults.threshold);
    let hold_ms = repo
        .get_string(reserved_keys::AUDIO_VOICE_GATE_HOLD_MS)
        .await?
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(defaults.hold_ms);
    Ok(VoiceGateSettings {
        enabled,
        input_device_id,
        threshold,
        hold_ms,
    })
}

pub async fn set_voice_gate_enabled(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    set_bool_setting(repo, reserved_keys::AUDIO_VOICE_GATE_ENABLED, enabled).await
}

/// `None` clears the preference (not a no-op) and falls back to the OS default device.
pub async fn set_voice_gate_input_device_id(
    repo: &dyn SettingsRepo,
    device_id: Option<String>,
) -> Result<(), StorageError> {
    match device_id {
        Some(id) => {
            repo.set_string(reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID, &id)
                .await
        }
        None => {
            repo.delete(reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID)
                .await?;
            Ok(())
        }
    }
}

pub async fn set_voice_gate_threshold(
    repo: &dyn SettingsRepo,
    threshold: f32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::AUDIO_VOICE_GATE_THRESHOLD,
        &threshold.clamp(0.0, 1.0).to_string(),
    )
    .await
}

pub async fn set_voice_gate_hold_ms(
    repo: &dyn SettingsRepo,
    hold_ms: u32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::AUDIO_VOICE_GATE_HOLD_MS,
        &hold_ms.to_string(),
    )
    .await
}

pub async fn chat_history_display_limit(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::CHAT_HISTORY_DISPLAY_LIMIT)
        .await?;
    Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(500))
}

pub async fn set_chat_history_display_limit(
    repo: &dyn SettingsRepo,
    limit: u32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::CHAT_HISTORY_DISPLAY_LIMIT,
        &limit.to_string(),
    )
    .await
}

pub async fn soundboard_enabled(repo: &dyn SettingsRepo) -> Result<bool, StorageError> {
    Ok(get_bool_setting(repo, reserved_keys::SOUNDBOARD_ENABLED, true).await)
}

pub async fn set_soundboard_enabled(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    set_bool_setting(repo, reserved_keys::SOUNDBOARD_ENABLED, enabled).await
}

pub async fn soundboard_output_device(
    repo: &dyn SettingsRepo,
) -> Result<Option<String>, StorageError> {
    repo.get_string(reserved_keys::SOUNDBOARD_OUTPUT_DEVICE)
        .await
}

pub async fn set_soundboard_output_device(
    repo: &dyn SettingsRepo,
    device_id: Option<String>,
) -> Result<(), StorageError> {
    match device_id {
        Some(id) => {
            repo.set_string(reserved_keys::SOUNDBOARD_OUTPUT_DEVICE, &id)
                .await
        }
        None => {
            repo.delete(reserved_keys::SOUNDBOARD_OUTPUT_DEVICE).await?;
            Ok(())
        }
    }
}

pub async fn soundboard_master_volume(repo: &dyn SettingsRepo) -> Result<f32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::SOUNDBOARD_MASTER_VOLUME)
        .await?;
    Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(1.0))
}

pub async fn set_soundboard_master_volume(
    repo: &dyn SettingsRepo,
    volume: f32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::SOUNDBOARD_MASTER_VOLUME,
        &volume.clamp(0.0, 1.0).to_string(),
    )
    .await
}

pub async fn soundboard_also_headphones(repo: &dyn SettingsRepo) -> Result<bool, StorageError> {
    Ok(get_bool_setting(repo, reserved_keys::SOUNDBOARD_ALSO_HEADPHONES, false).await)
}

pub async fn set_soundboard_also_headphones(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    set_bool_setting(repo, reserved_keys::SOUNDBOARD_ALSO_HEADPHONES, enabled).await
}

pub const DEFAULT_DIAGNOSTIC_LOG_LEVEL: LogLevel = LogLevel::Info;

const DIAGNOSTIC_LOG_LEVELS: [LogLevel; 5] = [
    LogLevel::Trace,
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
];

/// Stable on-disk spelling: changing one orphans every value already persisted under it.
pub fn log_level_as_str(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn decode_log_level(raw: &str) -> Option<LogLevel> {
    let raw = raw.trim();
    DIAGNOSTIC_LOG_LEVELS
        .into_iter()
        .find(|level| raw.eq_ignore_ascii_case(log_level_as_str(level)))
}

/// An unreadable stored spelling yields the default; a diagnostics setting never blocks boot.
pub async fn diagnostic_log_level(repo: &dyn SettingsRepo) -> Result<LogLevel, StorageError> {
    let raw = repo
        .get_string(reserved_keys::DIAGNOSTICS_LOG_LEVEL)
        .await?;
    Ok(raw
        .as_deref()
        .and_then(decode_log_level)
        .unwrap_or(DEFAULT_DIAGNOSTIC_LOG_LEVEL))
}

pub async fn set_diagnostic_log_level(
    repo: &dyn SettingsRepo,
    level: &LogLevel,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::DIAGNOSTICS_LOG_LEVEL,
        log_level_as_str(level),
    )
    .await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_types::LogLevel;
    use serde::{Deserialize, Serialize};

    use super::disclosure::{self, SettingDisclosure};
    use super::{
        DEFAULT_DIAGNOSTIC_LOG_LEVEL, Density, Language, SettingsRepo, StorageError,
        diagnostic_log_level, get_json_setting, reserved_keys, set_diagnostic_log_level,
        set_json_setting,
    };

    #[derive(Default)]
    struct MapRepo {
        map: Mutex<HashMap<String, String>>,
    }

    #[async_trait]
    impl SettingsRepo for MapRepo {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.map
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.map.lock().unwrap().remove(key).is_some())
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.map.lock().unwrap().clone())
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    #[tokio::test]
    async fn get_json_setting_yields_none_for_absent_malformed_and_wrong_shape() {
        let repo = MapRepo::default();
        repo.set_string("malformed", "{not json").await.unwrap();
        repo.set_string("wrong_shape", "42").await.unwrap();

        for key in ["absent", "malformed", "wrong_shape"] {
            let decoded: Option<Vec<String>> = get_json_setting(&repo, key).await;
            assert!(decoded.is_none(), "expected None for {key:?}");
        }
    }

    #[tokio::test]
    async fn json_setting_round_trips_a_vec_and_a_struct() {
        let repo = MapRepo::default();

        let list = vec!["a.com".to_owned(), "b.com".to_owned()];
        set_json_setting(&repo, "list", &list).await.unwrap();
        assert_eq!(
            get_json_setting::<Vec<String>>(&repo, "list").await,
            Some(list)
        );

        let sample = Sample {
            name: "nova".into(),
            count: 3,
        };
        set_json_setting(&repo, "sample", &sample).await.unwrap();
        assert_eq!(
            get_json_setting::<Sample>(&repo, "sample").await,
            Some(sample)
        );
    }

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

    // Why: the stored spelling is a persistence contract, not a formatting detail - respelling
    // any of the five orphans every value already written under the old spelling, so each one
    // is pinned here on purpose.
    #[tokio::test]
    async fn diagnostic_log_level_persists_each_level_under_its_canonical_lowercase_spelling() {
        let repo = MapRepo::default();
        for (level, spelling) in [
            (LogLevel::Trace, "trace"),
            (LogLevel::Debug, "debug"),
            (LogLevel::Info, "info"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Error, "error"),
        ] {
            set_diagnostic_log_level(&repo, &level).await.unwrap();
            let stored = repo
                .get_string(reserved_keys::DIAGNOSTICS_LOG_LEVEL)
                .await
                .unwrap();
            assert_eq!(stored.as_deref(), Some(spelling));
            assert_eq!(diagnostic_log_level(&repo).await.unwrap(), level);
        }
    }

    #[tokio::test]
    async fn diagnostic_log_level_yields_the_default_when_absent_or_undecodable() {
        let repo = MapRepo::default();
        assert_eq!(
            diagnostic_log_level(&repo).await.unwrap(),
            DEFAULT_DIAGNOSTIC_LOG_LEVEL,
        );

        for stored in ["", "   ", "verbose", "warning", "traces", "5", "info,debug"] {
            repo.set_string(reserved_keys::DIAGNOSTICS_LOG_LEVEL, stored)
                .await
                .unwrap();
            assert_eq!(
                diagnostic_log_level(&repo).await.unwrap(),
                DEFAULT_DIAGNOSTIC_LOG_LEVEL,
                "expected the default for {stored:?}",
            );
        }
    }

    #[tokio::test]
    async fn diagnostic_log_level_decodes_padded_and_mixed_case_spellings() {
        let repo = MapRepo::default();
        for (stored, expected) in [
            (" trace ", LogLevel::Trace),
            ("DEBUG", LogLevel::Debug),
            ("\twarn\n", LogLevel::Warn),
            ("eRrOr", LogLevel::Error),
        ] {
            repo.set_string(reserved_keys::DIAGNOSTICS_LOG_LEVEL, stored)
                .await
                .unwrap();
            assert_eq!(
                diagnostic_log_level(&repo).await.unwrap(),
                expected,
                "for {stored:?}",
            );
        }
    }

    /// A value no disclosure rendering may ever pass through.
    const SECRET: &str = "nova_the_broadcaster";

    /// Why: default-deny is the whole safety property of the vocabulary - a key added tomorrow,
    /// or a key whose classification is deleted, must cost a missing diagnostic and not a leak.
    #[test]
    fn an_unclassified_key_is_withheld_and_renders_nothing() {
        for key in [
            "",
            "twitch.account_login",
            "some.key.invented.tomorrow",
            "theme.extra",
            // One character short of the engine-params prefix, so the `starts_with` arm misses it.
            "tts.engine_params",
        ] {
            assert_eq!(
                disclosure::class_of(key),
                SettingDisclosure::Withheld,
                "key {key:?}",
            );
            assert_eq!(disclosure::render(key, SECRET), None, "key {key:?}");
        }
    }

    /// Why: R4 - no account, channel, host or path value appears anywhere in the bundle. Moving
    /// any key on this list into the verbatim arm publishes it, so the class is pinned per key.
    #[test]
    fn keys_that_can_name_a_host_a_path_or_a_device_are_never_verbatim() {
        for (key, expected) in [
            (
                reserved_keys::SERVER_OVERLAY_ROOT,
                SettingDisclosure::Presence,
            ),
            (
                reserved_keys::SERVER_BIND_ADDRESS,
                SettingDisclosure::Presence,
            ),
            (
                reserved_keys::AUDIO_OUTPUT_DEVICE_ID,
                SettingDisclosure::Presence,
            ),
            (
                reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID,
                SettingDisclosure::Presence,
            ),
            (
                reserved_keys::SOUNDBOARD_OUTPUT_DEVICE,
                SettingDisclosure::Presence,
            ),
            (reserved_keys::FONT_BODY, SettingDisclosure::Presence),
            (reserved_keys::FONT_MONO, SettingDisclosure::Presence),
            (
                reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS,
                SettingDisclosure::Kind,
            ),
            (
                reserved_keys::SERVER_ADDITIONAL_ORIGINS,
                SettingDisclosure::Kind,
            ),
        ] {
            assert_eq!(disclosure::class_of(key), expected, "key {key}");
            assert!(
                !disclosure::render(key, SECRET).is_some_and(|shown| shown.contains(SECRET)),
                "key {key} disclosed its value",
            );
        }
    }

    #[test]
    fn an_operational_setting_is_disclosed_verbatim() {
        for (key, value) in [
            (reserved_keys::DIAGNOSTICS_LOG_LEVEL, "trace"),
            (reserved_keys::SERVER_PORT, "8080"),
            (reserved_keys::SERVER_ENABLED, "true"),
            ("tts.engine_params.piper.speed", "1.25"),
        ] {
            assert_eq!(disclosure::class_of(key), SettingDisclosure::Verbatim);
            assert_eq!(
                disclosure::render(key, value).as_deref(),
                Some(value),
                "key {key}",
            );
        }
    }

    #[test]
    fn a_kind_key_discloses_only_the_container_shape() {
        for (value, expected) in [
            (r#"["a.com","b.com"]"#, "list(2)"),
            ("[]", "list(0)"),
            (r#"{"a":1,"b":2}"#, "map(2)"),
            ("{}", "map(0)"),
            // Not a container: falls back to a stamped character count, not the text.
            (SECRET, "<redacted len=20>"),
            ("Привіт", "<redacted len=6>"),
            ("", "<redacted len=0>"),
        ] {
            assert_eq!(
                disclosure::render(reserved_keys::SCRIPT_HTTP_ALLOWED_DOMAINS, value).as_deref(),
                Some(expected),
                "value {value:?}",
            );
        }
    }

    #[test]
    fn a_presence_key_discloses_only_whether_a_value_exists() {
        for (value, expected) in [("", "unset"), (" ", "set"), (SECRET, "set")] {
            assert_eq!(
                disclosure::render(reserved_keys::SERVER_OVERLAY_ROOT, value).as_deref(),
                Some(expected),
                "value {value:?}",
            );
        }
    }

    #[test]
    fn render_all_drops_withheld_keys_and_orders_the_rest_by_key() {
        let stored: HashMap<String, String> = [
            (reserved_keys::SERVER_PORT, "8080"),
            (reserved_keys::DIAGNOSTICS_LOG_LEVEL, "debug"),
            (reserved_keys::SERVER_OVERLAY_ROOT, "/home/nova/overlays"),
            ("twitch.account_login", SECRET),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

        let shown = disclosure::render_all(&stored);

        assert_eq!(
            shown.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![
                "diagnostics.log_level",
                "server.overlay_root",
                "server.port"
            ],
        );
        assert!(
            !shown.values().any(|v| v.contains("nova")),
            "a withheld key or a path value survived: {shown:?}",
        );
    }
}
