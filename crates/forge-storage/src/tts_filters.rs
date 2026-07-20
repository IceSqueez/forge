use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Mode controlling what happens when a blocked word is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlocklistMode {
    /// Replace the matched word with `***`.
    #[default]
    Censor,
    /// Drop the entire message from the speak queue.
    Suppress,
}

/// How URLs embedded in chat messages are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UrlMode {
    /// Read the URL as-is.
    #[default]
    Speak,
    /// Replace the URL with a configurable spoken label (default: "link").
    Replace,
    /// Drop the entire message when a URL is present.
    Suppress,
}

/// Kind-specific parameters for a single filter rule.
///
/// The `Regex` variant stores the SOURCE pattern only - compiled form is never
/// persisted; the TTS domain re-compiles at load time and rejects invalid patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterRuleKind {
    /// Case-insensitive literal substring → replacement.
    Literal {
        pattern: String,
        replacement: String,
    },
    /// `regex`-crate pattern string → replacement.
    Regex {
        pattern: String,
        replacement: String,
    },
    /// Blocked-word set with a per-rule censor-or-suppress mode.
    Blocklist {
        words: Vec<String>,
        mode: BlocklistMode,
    },
}

/// A single user-authored filter rule.
///
/// `position` is dense 0..n; gaps are a load-time repair in the TTS domain - the
/// storage layer stores whatever value the caller provides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Zero-based ordering index; rules are applied in ascending position order.
    pub position: u32,
    pub kind: FilterRuleKind,
}

/// Pipeline-level settings that are not user-authored rules but are persisted
/// alongside them as part of the TTS filter configuration.
///
/// `url_mode` and `max_length` are retired by the skip-rules/output model below
/// but stay readable so the TTS domain can one-time-convert existing rows
/// (`UrlMode::Replace` into a synthetic replacement rule, `UrlMode::Suppress`
/// into `skip_contains_url`, `max_length` into `longer_than_max_chars`) without
/// breaking deserialization of rows written before this model existed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsPipelineSettings {
    pub url_mode: UrlMode,
    pub max_length: Option<u32>,
    pub blocklist_mode: BlocklistMode,
    /// Strip Twitch-style emote tokens before synthesis.
    pub strip_twitch_emotes: bool,
    /// Strip channel-point reward emote tokens before synthesis.
    pub strip_reward_emotes: bool,
    #[serde(default)]
    pub skip_contains_url: bool,
    /// Retired by `skip_prefix` but stays readable so existing rows one-time-convert
    /// (`true` becomes `skip_prefix: Some("!")`) without breaking deserialization.
    #[serde(default)]
    pub skip_starts_with_bang: bool,
    #[serde(default)]
    pub skip_prefix: Option<String>,
    #[serde(default)]
    pub skip_from_bot_accounts: bool,
    /// User-added bot accounts merged with the built-in list at evaluation time.
    #[serde(default)]
    pub bot_accounts: Vec<String>,
    #[serde(default)]
    pub skip_longer_than: bool,
    #[serde(default = "default_longer_than_max_chars")]
    pub longer_than_max_chars: u32,
    #[serde(default)]
    pub skip_repeat_of_recent: bool,
    #[serde(default = "default_repeat_of_recent_window")]
    pub repeat_of_recent_window: u32,
    #[serde(default)]
    pub output_read_display_name_first: bool,
    #[serde(default)]
    pub output_emote_to_word: bool,
    #[serde(default)]
    pub skip_emote_only: bool,
    #[serde(default)]
    pub skip_mostly_non_latin: bool,
    /// Source patterns only - compiled form is never persisted, matching `FilterRuleKind::Regex`.
    #[serde(default)]
    pub skip_custom_regexes: Vec<String>,
    #[serde(default)]
    pub output_sanitize_punctuation: bool,
}

fn default_longer_than_max_chars() -> u32 {
    200
}

fn default_repeat_of_recent_window() -> u32 {
    3
}

impl Default for TtsPipelineSettings {
    fn default() -> Self {
        Self {
            url_mode: UrlMode::Speak,
            max_length: None,
            blocklist_mode: BlocklistMode::Censor,
            strip_twitch_emotes: true,
            strip_reward_emotes: true,
            skip_contains_url: false,
            skip_starts_with_bang: false,
            skip_prefix: None,
            skip_from_bot_accounts: false,
            bot_accounts: Vec::new(),
            skip_longer_than: false,
            longer_than_max_chars: default_longer_than_max_chars(),
            skip_repeat_of_recent: false,
            repeat_of_recent_window: default_repeat_of_recent_window(),
            output_read_display_name_first: false,
            output_emote_to_word: false,
            skip_emote_only: false,
            skip_mostly_non_latin: false,
            skip_custom_regexes: Vec::new(),
            output_sanitize_punctuation: false,
        }
    }
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait TtsFiltersRepo: Send + Sync {
    /// Returns all rules ordered by `position` ascending.
    async fn list_rules(&self) -> Result<Vec<FilterRule>, StorageError>;

    /// Replaces the entire ordered rule set atomically.
    async fn replace_rules(&self, rules: &[FilterRule]) -> Result<(), StorageError>;

    async fn get_pipeline_settings(&self) -> Result<TtsPipelineSettings, StorageError>;

    async fn set_pipeline_settings(
        &self,
        settings: &TtsPipelineSettings,
    ) -> Result<(), StorageError>;
}
