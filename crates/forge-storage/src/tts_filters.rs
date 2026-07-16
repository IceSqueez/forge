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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsPipelineSettings {
    pub url_mode: UrlMode,
    /// Maximum character count before truncation. `None` = unlimited.
    pub max_length: Option<u32>,
    pub blocklist_mode: BlocklistMode,
    /// Strip Twitch-style emote tokens before synthesis.
    pub strip_twitch_emotes: bool,
    /// Strip channel-point reward emote tokens before synthesis.
    pub strip_reward_emotes: bool,
}

impl Default for TtsPipelineSettings {
    fn default() -> Self {
        Self {
            url_mode: UrlMode::Speak,
            max_length: None,
            blocklist_mode: BlocklistMode::Censor,
            strip_twitch_emotes: true,
            strip_reward_emotes: true,
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
