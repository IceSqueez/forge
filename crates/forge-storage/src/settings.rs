use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use forge_speech_types::SynthesisDefaults;
use forge_types::LogLevel;
use serde::{Deserialize, Serialize};

use crate::StorageError;
pub use typed::log_level_as_str;
use typed::{read_setting, write_setting};

pub const SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT: bool = true;

pub mod reserved_keys {
    pub const THEME: &str = "theme";
    pub const ACCENT: &str = "accent";
    pub const DENSITY: &str = "density";
    pub const FONT_BODY: &str = "font_body";
    pub const FONT_MONO: &str = "font_mono";
    pub const EVENT_LOG_RETENTION_DAYS: &str = "event_log_retention_days";
    pub const ACTION_HISTORY_RETENTION_DAYS: &str = "action_history_retention_days";
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
    pub const REGION: &str = "app.region";
    pub const DATE_FORMAT: &str = "app.date_format";
    pub const TIME_FORMAT: &str = "app.time_format";
    pub const FIRST_DAY_OF_WEEK: &str = "app.first_day_of_week";
    pub const KEYBOARD_SHORTCUTS: &str = "app.keyboard_shortcuts";
    pub const AUDIO_OUTPUT_DEVICE_ID: &str = "audio.output_device_id";
    pub const AUDIO_VOICE_GATE_ENABLED: &str = "audio.voice_gate_enabled";
    pub const AUDIO_VOICE_GATE_INPUT_DEVICE_ID: &str = "audio.voice_gate_input_device_id";
    pub const AUDIO_VOICE_GATE_THRESHOLD: &str = "audio.voice_gate_threshold";
    pub const AUDIO_VOICE_GATE_HOLD_MS: &str = "audio.voice_gate_hold_ms";
    pub const AUDIO_SPEECH_ROUTE: &str = "audio.speech_route";
    pub const AUDIO_CLIPS_ROUTE: &str = "audio.clips_route";
    pub const AUDIO_OVERLAY_ID: &str = "audio.overlay_id";
    pub const CHAT_HISTORY_PER_VIEWER_LIMIT: &str = "chat_history.per_viewer_limit";
    pub const CHAT_HISTORY_DISPLAY_LIMIT: &str = "chat_history.display_limit";
    pub const PICKER_FAVORITES_SUB_ACTIONS: &str = "picker.favorites.sub_actions";
    pub const PICKER_FAVORITES_TRIGGERS: &str = "picker.favorites.triggers";
    pub const PICKER_FAVORITES_ICONS: &str = "picker.favorites.icons";
    pub const TTS_DISABLED_ENGINES: &str = "tts.disabled_engines";
    pub const TTS_SYNTHESIS_DEFAULTS: &str = "tts.synthesis_defaults";
    pub const TTS_MASTER_VOLUME: &str = "tts.master_volume";
    pub const TTS_ENGINE_PARAMS_PREFIX: &str = "tts.engine_params.";
    pub const SOUNDBOARD_ENABLED: &str = "soundboard.enabled";
    pub const SOUNDBOARD_OUTPUT_DEVICE: &str = "soundboard.output_device";
    pub const SOUNDBOARD_MASTER_VOLUME: &str = "soundboard.master_volume";
    pub const SOUNDBOARD_ALSO_HEADPHONES: &str = "soundboard.also_headphones";
    pub const DIAGNOSTICS_LOG_LEVEL: &str = "diagnostics.log_level";
    pub const STAY_AWAKE: &str = "app.stay_awake";
    pub const WINDOW_PLACEMENT: &str = "app.window_placement";
    pub const UPDATES_ROLLED_BACK_VERSION: &str = "updates.rolled_back_version";
    pub const UPDATES_LEFTOVER_INSTALL_NOTICED: &str = "updates.leftover_install_noticed";
    pub const CREDENTIALS_KEY_LOSS: &str = "credentials.key_loss_stranded";
    pub const INTEGRATION_ENABLED_PREFIX: &str = "integration.enabled.";
    pub const RETIRED_MIDI_ENABLED: &str = "midi.enabled";
    pub const RETIRED_HOTKEY_ENABLED: &str = "hotkey.enabled";
    pub const OBS_AUTO_RECONNECT: &str = "obs.auto_reconnect";
    pub const OBS_CONNECT_ON_LAUNCH: &str = "obs.connect_on_launch";
    pub const VTUBE_AUTO_RECONNECT: &str = "vtube.auto_reconnect";
    pub const VTUBE_CONNECT_ON_LAUNCH: &str = "vtube.connect_on_launch";
    pub const MIDI_KNOWN_DEVICES: &str = "midi.known_devices";
    pub const HOTKEY_HOLD_CEILING_SECS: &str = "hotkey.hold_ceiling_secs";
    pub const DONATELLO_POLL_INTERVAL_SECS: &str = "donations.donatello.poll_interval_secs";
    pub const VOICE_ASSIGNMENT_STRATEGY: &str = "voice:strategy";
    pub const NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE: &str =
        "notifications.action_failure_os_notice";
    pub const NOTIFICATIONS_CONNECTION_LOSS_SOUND: &str = "notifications.connection_loss_sound";
    pub const NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE: &str =
        "notifications.scheduled_backup_failure_os_notice";
    pub const BACKUP_SCHEDULE_ENABLED: &str = "backup_schedule.enabled";
    pub const BACKUP_SCHEDULE_INTERVAL_MINUTES: &str = "backup_schedule.interval_minutes";
    pub const BACKUP_SCHEDULE_KEEP_NEWEST: &str = "backup_schedule.keep_newest";
    pub const BACKUP_SCHEDULE_KEEP_DAILY_DAYS: &str = "backup_schedule.keep_daily_days";
    pub const BACKUP_SCHEDULE_INCLUDE_MEDIA: &str = "backup_schedule.include_media";
    pub const BACKUP_SCHEDULE_INCLUDE_VOICES: &str = "backup_schedule.include_voices";
    pub const BACKUP_SCHEDULE_INCLUDE_OVERLAYS: &str = "backup_schedule.include_overlays";
    pub const BACKUP_SCHEDULE_INCLUDE_ASSETS: &str = "backup_schedule.include_assets";
    pub const BACKUP_SCHEDULE_DESTINATION: &str = "backup_schedule.destination";
    pub const BACKUP_SCHEDULE_KEY: &str = "backup_schedule.key";
    pub const BACKUP_SCHEDULE_LAST_SUCCESS_AT: &str = "backup_schedule.last_success_at";
    pub const BACKUP_SCHEDULE_CONTENT_FINGERPRINT: &str = "backup_schedule.content_fingerprint";
    pub const SAFETY_COPIES_MODE: &str = "safety_copies.mode";
    pub const SAFETY_COPIES_KEEP_NEWEST: &str = "safety_copies.keep_newest";
    pub const SAFETY_COPIES_DELETE_AFTER_DAYS: &str = "safety_copies.delete_after_days";
}

pub mod catalog;
pub mod core_settings;
pub mod disclosure;
pub mod favorites;
pub mod portability;
pub mod spec;
pub mod typed;

pub const DEFAULT_NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE: bool = true;
pub const DEFAULT_NOTIFICATIONS_CONNECTION_LOSS_SOUND: bool = false;
pub const DEFAULT_NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE: bool = true;
pub const DEFAULT_BACKUP_SCHEDULE_ENABLED: bool = false;
pub const DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES: u32 = 60;
pub const MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES: u32 = 30;
pub const MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES: u32 = 10_080;
pub const DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST: u32 = 20;
pub const MIN_BACKUP_SCHEDULE_KEEP_NEWEST: u32 = 1;
pub const MAX_BACKUP_SCHEDULE_KEEP_NEWEST: u32 = 1000;
pub const DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS: u32 = 30;
pub const MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS: u32 = 0;
pub const MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS: u32 = 3650;
pub const DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY: bool = true;
pub const DEFAULT_SAFETY_COPIES_KEEP_NEWEST: u32 = 5;
pub const MIN_SAFETY_COPIES_KEEP_NEWEST: u32 = 1;
pub const MAX_SAFETY_COPIES_KEEP_NEWEST: u32 = 1000;
pub const DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS: u32 = 30;
pub const MIN_SAFETY_COPIES_DELETE_AFTER_DAYS: u32 = 1;
pub const MAX_SAFETY_COPIES_DELETE_AFTER_DAYS: u32 = 3650;
pub const DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT: u32 = 500;
pub const DEFAULT_CHAT_HISTORY_PER_VIEWER_LIMIT: u32 = 50;
pub const UNLIMITED_CHAT_HISTORY_PER_VIEWER_LIMIT: u32 = 0;
pub const MIN_CHAT_HISTORY_PER_VIEWER_LIMIT: u32 = 1;
pub const MAX_CHAT_HISTORY_PER_VIEWER_LIMIT: u32 = 10_000;
pub const MIN_CHAT_HISTORY_DISPLAY_LIMIT: u32 = 1;
pub const MAX_CHAT_HISTORY_DISPLAY_LIMIT: u32 = 10_000;
pub const MIN_VOICE_GATE_HOLD_MS: u32 = 0;
pub const MAX_VOICE_GATE_HOLD_MS: u32 = 10_000;
pub const SILENT_LEVEL: f32 = 0.0;
pub const FULL_LEVEL: f32 = 1.0;

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

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait SettingsRepo: Send + Sync {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError>;
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError>;

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

    async fn get_theme(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::THEME).await
    }

    async fn set_theme(&self, theme_key: &str) -> Result<(), StorageError> {
        self.set_string(reserved_keys::THEME, theme_key).await
    }

    async fn get_accent(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::ACCENT).await
    }

    async fn set_accent(&self, accent_key: &str) -> Result<(), StorageError> {
        self.set_string(reserved_keys::ACCENT, accent_key).await
    }

    async fn font_body(&self) -> Result<Option<String>, StorageError> {
        self.get_string(reserved_keys::FONT_BODY).await
    }

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

pub(crate) fn decode_bool_setting(s: &str) -> Option<bool> {
    let s = s.trim();
    if s == "1" || s.eq_ignore_ascii_case("true") {
        Some(true)
    } else if s == "0" || s.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn undecodable(key: &str) -> StorageError {
    StorageError::UndecodableSetting {
        key: key.to_owned(),
    }
}

pub async fn stored_bool_setting(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Result<Option<bool>, StorageError> {
    repo.get_string(key)
        .await?
        .map(|raw| decode_bool_setting(&raw).ok_or_else(|| undecodable(key)))
        .transpose()
}

pub async fn get_bool_setting(repo: &dyn SettingsRepo, key: &str, default: bool) -> bool {
    stored_bool_setting(repo, key)
        .await
        .ok()
        .flatten()
        .unwrap_or(default)
}

pub async fn set_bool_setting(
    repo: &dyn SettingsRepo,
    key: &str,
    value: bool,
) -> Result<(), StorageError> {
    repo.set_string(key, if value { "true" } else { "false" })
        .await
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredDisplayFormats {
    pub region: Option<String>,
    pub date_format: Option<String>,
    pub time_format: Option<String>,
    pub first_day_of_week: Option<String>,
}

pub async fn stored_display_formats(
    repo: &dyn SettingsRepo,
) -> Result<StoredDisplayFormats, StorageError> {
    Ok(StoredDisplayFormats {
        region: repo.get_string(reserved_keys::REGION).await?,
        date_format: repo.get_string(reserved_keys::DATE_FORMAT).await?,
        time_format: repo.get_string(reserved_keys::TIME_FORMAT).await?,
        first_day_of_week: repo.get_string(reserved_keys::FIRST_DAY_OF_WEEK).await?,
    })
}

pub async fn stored_json_setting<T: serde::de::DeserializeOwned>(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Result<Option<T>, StorageError> {
    repo.get_string(key)
        .await?
        .map(|raw| serde_json::from_str(&raw).map_err(|_| undecodable(key)))
        .transpose()
}

pub async fn get_json_setting<T: serde::de::DeserializeOwned>(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Option<T> {
    stored_json_setting(repo, key).await.ok().flatten()
}

pub async fn set_json_setting<T: Serialize>(
    repo: &dyn SettingsRepo,
    key: &str,
    value: &T,
) -> Result<(), StorageError> {
    let json = serde_json::to_string(value).map_err(|e| StorageError::Parse(e.to_string()))?;
    repo.set_string(key, &json).await
}

pub fn clamp_chat_history_per_viewer_limit(limit: u32) -> u32 {
    core_settings::CHAT_HISTORY_PER_VIEWER_LIMIT
        .codec()
        .clamp(limit)
}

pub async fn chat_history_per_viewer_limit(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    read_setting(repo, &core_settings::CHAT_HISTORY_PER_VIEWER_LIMIT).await
}

pub async fn set_chat_history_per_viewer_limit(
    repo: &dyn SettingsRepo,
    limit: u32,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::CHAT_HISTORY_PER_VIEWER_LIMIT, &limit).await
}

pub async fn disabled_tts_engines(repo: &dyn SettingsRepo) -> Result<Vec<String>, StorageError> {
    Ok(
        stored_json_setting(repo, reserved_keys::TTS_DISABLED_ENGINES)
            .await?
            .unwrap_or_default(),
    )
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
    Ok(stored_json_setting(repo, &key).await?.unwrap_or_default())
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
    read_setting(repo, &core_settings::TTS_MASTER_VOLUME).await
}

pub async fn set_master_volume(repo: &dyn SettingsRepo, volume: f32) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::TTS_MASTER_VOLUME, &volume).await
}

pub const VOICE_GATE_DEFAULT_THRESHOLD: f32 = 0.15;
pub const VOICE_GATE_DEFAULT_HOLD_MS: u32 = 800;

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceGateSettings {
    pub enabled: bool,
    pub input_device_id: Option<String>,
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
    let enabled = read_setting(repo, &core_settings::AUDIO_VOICE_GATE_ENABLED).await?;
    let input_device_id = repo
        .get_string(reserved_keys::AUDIO_VOICE_GATE_INPUT_DEVICE_ID)
        .await?;
    let threshold = read_setting(repo, &core_settings::AUDIO_VOICE_GATE_THRESHOLD).await?;
    let hold_ms = read_setting(repo, &core_settings::AUDIO_VOICE_GATE_HOLD_MS).await?;
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
    write_setting(repo, &core_settings::AUDIO_VOICE_GATE_ENABLED, &enabled).await
}

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
    write_setting(repo, &core_settings::AUDIO_VOICE_GATE_THRESHOLD, &threshold).await
}

pub async fn set_voice_gate_hold_ms(
    repo: &dyn SettingsRepo,
    hold_ms: u32,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::AUDIO_VOICE_GATE_HOLD_MS, &hold_ms).await
}

pub async fn chat_history_display_limit(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    read_setting(repo, &core_settings::CHAT_HISTORY_DISPLAY_LIMIT).await
}

pub async fn set_chat_history_display_limit(
    repo: &dyn SettingsRepo,
    limit: u32,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::CHAT_HISTORY_DISPLAY_LIMIT, &limit).await
}

pub async fn soundboard_enabled(repo: &dyn SettingsRepo) -> Result<bool, StorageError> {
    read_setting(repo, &core_settings::SOUNDBOARD_ENABLED).await
}

pub async fn set_soundboard_enabled(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::SOUNDBOARD_ENABLED, &enabled).await
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
    read_setting(repo, &core_settings::SOUNDBOARD_MASTER_VOLUME).await
}

pub async fn set_soundboard_master_volume(
    repo: &dyn SettingsRepo,
    volume: f32,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::SOUNDBOARD_MASTER_VOLUME, &volume).await
}

pub async fn soundboard_also_headphones(repo: &dyn SettingsRepo) -> Result<bool, StorageError> {
    read_setting(repo, &core_settings::SOUNDBOARD_ALSO_HEADPHONES).await
}

pub async fn set_soundboard_also_headphones(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::SOUNDBOARD_ALSO_HEADPHONES, &enabled).await
}

pub const DEFAULT_DIAGNOSTIC_LOG_LEVEL: LogLevel = LogLevel::Info;

pub async fn diagnostic_log_level(repo: &dyn SettingsRepo) -> Result<LogLevel, StorageError> {
    read_setting(repo, &core_settings::DIAGNOSTICS_LOG_LEVEL).await
}

pub async fn set_diagnostic_log_level(
    repo: &dyn SettingsRepo,
    level: &LogLevel,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::DIAGNOSTICS_LOG_LEVEL, level).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialsKeyLoss {
    pub stranded: u64,
}

pub async fn record_credentials_key_loss(
    repo: &dyn SettingsRepo,
    loss: CredentialsKeyLoss,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::CREDENTIALS_KEY_LOSS,
        &loss.stranded.to_string(),
    )
    .await
}

pub async fn take_credentials_key_loss(
    repo: &dyn SettingsRepo,
) -> Result<Option<CredentialsKeyLoss>, StorageError> {
    let Some(raw) = repo.get_string(reserved_keys::CREDENTIALS_KEY_LOSS).await? else {
        return Ok(None);
    };
    repo.delete(reserved_keys::CREDENTIALS_KEY_LOSS).await?;
    Ok(raw
        .parse::<u64>()
        .ok()
        .map(|stranded| CredentialsKeyLoss { stranded }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_types::LogLevel;
    use serde::{Deserialize, Serialize};

    use super::{
        CredentialsKeyLoss, DEFAULT_DIAGNOSTIC_LOG_LEVEL, EngineParams, Language, SettingsRepo,
        StorageError, StoredDisplayFormats, chat_history_per_viewer_limit, diagnostic_log_level,
        disabled_tts_engines, engine_params, get_bool_setting, get_json_setting,
        record_credentials_key_loss, reserved_keys, set_chat_history_per_viewer_limit,
        set_diagnostic_log_level, set_json_setting, stored_bool_setting, stored_display_formats,
        stored_json_setting, take_credentials_key_loss,
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

    struct UnreachableRepo;

    const UNREACHABLE: &str = "settings store unreachable";

    #[async_trait]
    impl SettingsRepo for UnreachableRepo {
        async fn get_string(&self, _: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::Connection {
                reason: UNREACHABLE.to_owned(),
            })
        }
        async fn set_string(&self, _: &str, _: &str) -> Result<(), StorageError> {
            unreachable!("the strict getters never write")
        }
        async fn delete(&self, _: &str) -> Result<bool, StorageError> {
            unreachable!("the strict getters never delete")
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            unreachable!("the strict getters read one key")
        }
    }

    async fn seeded(key: &str, raw: Option<&str>) -> MapRepo {
        let repo = MapRepo::default();
        if let Some(raw) = raw {
            repo.set_string(key, raw).await.unwrap();
        }
        repo
    }

    fn names_only_the_key(error: &StorageError, key: &str, raw: &str) -> bool {
        matches!(error, StorageError::UndecodableSetting { key: named } if named == key)
            && (raw.is_empty() || !error.to_string().contains(raw))
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

    #[tokio::test]
    async fn stored_json_setting_separates_absent_valid_and_undecodable_values() {
        let key = "sample";
        for (raw, expected) in [
            (None, Ok(None)),
            (
                Some(r#"{"name":"nova","count":3}"#),
                Ok(Some(Sample {
                    name: "nova".into(),
                    count: 3,
                })),
            ),
            (Some(r#"{"name":"secret-raw-value""#), Err(true)),
            (Some(r#"["secret-raw-value"]"#), Err(true)),
            (Some(r#"{"name":"secret-raw-value","count":-1}"#), Err(true)),
            (Some(""), Err(true)),
        ] {
            let repo = seeded(key, raw).await;

            let decoded = stored_json_setting::<Sample>(&repo, key)
                .await
                .map_err(|error| names_only_the_key(&error, key, "secret-raw-value"));

            assert_eq!(decoded, expected, "{raw:?}");
        }
    }

    #[tokio::test]
    async fn stored_bool_setting_trims_and_rejects_blank_or_unknown_spellings() {
        let key = "flag";
        for (raw, expected) in [
            (None, Ok(None)),
            (Some("true"), Ok(Some(true))),
            (Some("TRUE"), Ok(Some(true))),
            (Some("1"), Ok(Some(true))),
            (Some(" true\n"), Ok(Some(true))),
            (Some("false"), Ok(Some(false))),
            (Some("0"), Ok(Some(false))),
            (Some("\tFalse "), Ok(Some(false))),
            (Some(""), Err(true)),
            (Some("   "), Err(true)),
            (Some("maybe"), Err(true)),
            (Some("2"), Err(true)),
            (Some("yes"), Err(true)),
        ] {
            let repo = seeded(key, raw).await;

            let decoded = stored_bool_setting(&repo, key)
                .await
                .map_err(|error| names_only_the_key(&error, key, raw.unwrap_or_default().trim()));

            assert_eq!(decoded, expected, "{raw:?}");
        }
    }

    #[tokio::test]
    async fn strict_getters_pass_a_store_failure_through_instead_of_reporting_it_undecodable() {
        let failures = [
            stored_bool_setting(&UnreachableRepo, "flag").await.err(),
            stored_json_setting::<Sample>(&UnreachableRepo, "sample")
                .await
                .err(),
            disabled_tts_engines(&UnreachableRepo).await.err(),
            engine_params(&UnreachableRepo, "piper").await.err(),
        ];

        for failure in failures {
            assert!(
                matches!(&failure, Some(StorageError::Connection { reason }) if reason == UNREACHABLE),
                "{failure:?}"
            );
        }
    }

    #[tokio::test]
    async fn get_bool_setting_falls_back_to_the_default_only_for_absent_or_undecodable_values() {
        for (raw, default, expected) in [
            (None, true, true),
            (None, false, false),
            (Some("maybe"), true, true),
            (Some("maybe"), false, false),
            (Some(" false "), true, false),
            (Some(" 1 "), false, true),
        ] {
            let repo = seeded("flag", raw).await;

            assert_eq!(
                get_bool_setting(&repo, "flag", default).await,
                expected,
                "{raw:?} with default {default}"
            );
        }
    }

    #[tokio::test]
    async fn disabled_tts_engines_reports_an_undecodable_list_instead_of_enabling_every_engine() {
        let key = reserved_keys::TTS_DISABLED_ENGINES;
        let absent = disabled_tts_engines(&MapRepo::default()).await.unwrap();
        let stored = disabled_tts_engines(&seeded(key, Some(r#"["piper","espeak"]"#)).await)
            .await
            .unwrap();
        let undecodable = disabled_tts_engines(&seeded(key, Some(r#"{"piper":true}"#)).await)
            .await
            .unwrap_err();

        assert_eq!(
            (
                absent,
                stored,
                names_only_the_key(&undecodable, key, "piper")
            ),
            (
                Vec::<String>::new(),
                vec!["piper".to_owned(), "espeak".to_owned()],
                true
            )
        );
    }

    #[tokio::test]
    async fn engine_params_reports_undecodable_params_under_the_engine_key_instead_of_defaults() {
        let key = format!("{}piper", reserved_keys::TTS_ENGINE_PARAMS_PREFIX);
        let custom = EngineParams {
            pitch_semitones: 3.0,
            rate_multiplier: 1.5,
            gain: 0.25,
        };
        let absent = engine_params(&MapRepo::default(), "piper").await.unwrap();
        let stored = engine_params(
            &seeded(&key, Some(&serde_json::to_string(&custom).unwrap())).await,
            "piper",
        )
        .await
        .unwrap();
        let undecodable = engine_params(&seeded(&key, Some(r#"{"gain":"loud"}"#)).await, "piper")
            .await
            .unwrap_err();

        assert_eq!(
            (
                absent,
                stored,
                names_only_the_key(&undecodable, &key, "loud")
            ),
            (EngineParams::default(), custom, true)
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

    #[tokio::test]
    async fn credentials_key_loss_is_reported_once_then_reads_as_absent() {
        let repo = MapRepo::default();
        assert_eq!(take_credentials_key_loss(&repo).await.unwrap(), None);

        record_credentials_key_loss(&repo, CredentialsKeyLoss { stranded: 3 })
            .await
            .unwrap();

        assert_eq!(
            take_credentials_key_loss(&repo).await.unwrap(),
            Some(CredentialsKeyLoss { stranded: 3 }),
        );
        assert_eq!(
            take_credentials_key_loss(&repo).await.unwrap(),
            None,
            "a second read must not repeat the same loss",
        );
    }

    #[tokio::test]
    async fn credentials_key_loss_reads_a_non_numeric_stored_value_as_absent_and_clears_it() {
        let repo = MapRepo::default();
        repo.set_string(reserved_keys::CREDENTIALS_KEY_LOSS, "not-a-count")
            .await
            .unwrap();

        assert_eq!(take_credentials_key_loss(&repo).await.unwrap(), None);
        assert_eq!(
            repo.get_string(reserved_keys::CREDENTIALS_KEY_LOSS)
                .await
                .unwrap(),
            None,
            "the unreadable record must still be cleared",
        );
    }

    #[tokio::test]
    async fn chat_history_per_viewer_limit_falls_back_to_fifty_when_absent_or_unparseable() {
        for stored in [None, Some(""), Some("many"), Some("-5"), Some("12.5")] {
            let repo = MapRepo::default();
            if let Some(raw) = stored {
                repo.set_string(reserved_keys::CHAT_HISTORY_PER_VIEWER_LIMIT, raw)
                    .await
                    .unwrap();
            }

            let limit = chat_history_per_viewer_limit(&repo).await.unwrap();

            assert_eq!(limit, 50, "stored={stored:?}");
        }
    }

    #[tokio::test]
    async fn chat_history_per_viewer_limit_clamps_a_stored_value_into_range_on_read() {
        for (stored, expected) in [
            ("0", 0),
            ("1", 1),
            ("2", 2),
            ("9999", 9999),
            ("10000", 10000),
            ("10001", 10000),
        ] {
            let repo = MapRepo::default();
            repo.set_string(reserved_keys::CHAT_HISTORY_PER_VIEWER_LIMIT, stored)
                .await
                .unwrap();

            let limit = chat_history_per_viewer_limit(&repo).await.unwrap();

            assert_eq!(limit, expected, "stored={stored}");
        }
    }

    #[tokio::test]
    async fn set_chat_history_per_viewer_limit_persists_the_clamped_value() {
        for (requested, persisted) in [
            (0, "0"),
            (1, "1"),
            (10000, "10000"),
            (10001, "10000"),
            (u32::MAX, "10000"),
        ] {
            let repo = MapRepo::default();

            set_chat_history_per_viewer_limit(&repo, requested)
                .await
                .unwrap();

            assert_eq!(
                repo.get_string(reserved_keys::CHAT_HISTORY_PER_VIEWER_LIMIT)
                    .await
                    .unwrap()
                    .as_deref(),
                Some(persisted),
                "requested={requested}"
            );
        }
    }

    #[tokio::test]
    async fn stored_display_formats_reads_each_key_and_leaves_absent_ones_empty() {
        let repo = MapRepo::default();
        for (key, value) in [
            (reserved_keys::REGION, "GB"),
            (reserved_keys::TIME_FORMAT, "12h"),
            (reserved_keys::FIRST_DAY_OF_WEEK, "sat"),
        ] {
            repo.set_string(key, value).await.unwrap();
        }

        assert_eq!(
            stored_display_formats(&repo).await.unwrap(),
            StoredDisplayFormats {
                region: Some("GB".to_owned()),
                date_format: None,
                time_format: Some("12h".to_owned()),
                first_day_of_week: Some("sat".to_owned()),
            }
        );
    }
}
