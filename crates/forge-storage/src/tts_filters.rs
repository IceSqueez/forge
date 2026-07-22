use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlocklistMode {
    /// Replaces the matched word with `***`.
    #[default]
    Censor,
    /// Drops the entire message, not just the matched word.
    Suppress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UrlMode {
    #[default]
    Speak,
    /// Spoken label defaults to "link".
    Replace,
    /// Drops the entire message, not just the URL.
    Suppress,
}

/// The `Regex` variant stores the SOURCE pattern only, never the compiled form; the
/// TTS domain re-compiles at load time and rejects invalid patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterRuleKind {
    /// Case-insensitive literal substring -> replacement.
    Literal {
        pattern: String,
        replacement: String,
    },
    /// `regex`-crate pattern syntax -> replacement.
    Regex {
        pattern: String,
        replacement: String,
    },
    Blocklist {
        words: Vec<String>,
        mode: BlocklistMode,
    },
}

/// `position` is dense 0..n; gaps are a load-time repair in the TTS domain, not here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Zero-based ordering index; rules are applied in ascending position order.
    pub position: u32,
    pub kind: FilterRuleKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsPipelineSettings {
    pub url_mode: UrlMode,
    pub max_length: Option<u32>,
    pub blocklist_mode: BlocklistMode,
    pub strip_twitch_emotes: bool,
    pub strip_reward_emotes: bool,
    #[serde(default)]
    pub skip_contains_url: bool,
    #[serde(default)]
    pub skip_starts_with_bang: bool,
    #[serde(default)]
    pub skip_prefix: Option<String>,
    #[serde(default)]
    pub skip_from_bot_accounts: bool,
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
