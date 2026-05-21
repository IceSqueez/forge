use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Final outcome of running the full pipeline on one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineResult {
    Speak(String),
    Skip { reason: SkipReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    MatchedSkipRule(&'static str),
    BlockedByWordFilter,
    EmptyAfterProcessing,
}

/// Per-stage transformation record for the live-preview API.
#[derive(Debug, Clone)]
pub struct StageOutcome {
    pub stage: StageName,
    pub input: String,
    pub output: String,
    pub action: StageAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageName {
    EmoteStripper,
    UrlSanitizer,
    TextReplacements,
    WordBlocklist,
    LengthCapper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageAction {
    PassedThrough,
    Transformed,
    Skipped { reason: SkipReason },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmoteSources {
    pub twitch: bool,
    pub bttv: bool,
    pub ffz: bool,
    pub seven_tv: bool,
    pub emoji: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrlMode {
    Replace { substitute: String },
    SkipMessage,
    Passthrough,
}

impl Default for UrlMode {
    fn default() -> Self {
        Self::Replace {
            substitute: "link".into(),
        }
    }
}

/// A set of emote tokens supplied by the speak-queue actor at construction time.
#[derive(Debug, Clone, Default)]
pub struct EmoteTokenSet {
    pub tokens: HashSet<String>,
}

/// A text or regex replacement rule.
#[derive(Debug, Clone)]
pub enum ReplacementRule {
    Text {
        pattern: String,
        replacement: String,
    },
    Regex {
        compiled: regex::Regex,
        replacement: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlocklistMode {
    #[default]
    Censor,
    SkipMessage,
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("invalid regex pattern `{pattern}`: {source}")]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },
}

/// Full, validated configuration for one pipeline run.
///
/// Constructed once per settings-save; reused across messages.
/// Holds pre-compiled `Regex` objects — construction is fallible.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub emote_sources: EmoteSources,
    pub emote_tokens: EmoteTokenSet,
    pub url_mode: UrlMode,
    pub replacement_rules: Vec<ReplacementRule>,
    pub word_blocklist: Vec<String>,
    pub blocklist_mode: BlocklistMode,
    pub max_chars: usize,
}

impl PipelineConfig {
    pub fn new(
        emote_sources: EmoteSources,
        emote_tokens: EmoteTokenSet,
        url_mode: UrlMode,
        replacement_rules: Vec<ReplacementRule>,
        word_blocklist: Vec<String>,
        blocklist_mode: BlocklistMode,
        max_chars: usize,
    ) -> Self {
        Self {
            emote_sources,
            emote_tokens,
            url_mode,
            replacement_rules,
            word_blocklist,
            blocklist_mode,
            max_chars,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            emote_sources: EmoteSources::default(),
            emote_tokens: EmoteTokenSet::default(),
            url_mode: UrlMode::default(),
            replacement_rules: vec![],
            word_blocklist: vec![],
            blocklist_mode: BlocklistMode::default(),
            max_chars: 500,
        }
    }
}

/// Run the full pipeline on `text`.
///
/// Pure function — no I/O, no allocation beyond string manipulation.
/// Never panics. Config must be pre-validated via `PipelineConfig::new`.
pub fn process(_text: &str, _config: &PipelineConfig) -> PipelineResult {
    PipelineResult::Speak(_text.to_owned())
}

/// Run the pipeline and return per-stage outcomes for UI preview.
///
/// Returns all five `StageOutcome` entries regardless of early `Skip`.
/// On `Skip`, subsequent stages receive the last non-empty intermediate text
/// but record `StageAction::Skipped`.
pub fn preview(text: &str, _config: &PipelineConfig) -> (PipelineResult, Vec<StageOutcome>) {
    let stages = [
        StageName::EmoteStripper,
        StageName::UrlSanitizer,
        StageName::TextReplacements,
        StageName::WordBlocklist,
        StageName::LengthCapper,
    ];
    let outcomes = stages
        .iter()
        .map(|&stage| StageOutcome {
            stage,
            input: text.to_owned(),
            output: text.to_owned(),
            action: StageAction::PassedThrough,
        })
        .collect();
    (PipelineResult::Speak(text.to_owned()), outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_passthrough_stub() {
        let config = PipelineConfig::default();
        let result = process("hello world", &config);
        assert_eq!(result, PipelineResult::Speak("hello world".into()));
    }

    #[test]
    fn preview_returns_five_stages() {
        let config = PipelineConfig::default();
        let (_result, outcomes) = preview("test", &config);
        assert_eq!(outcomes.len(), 5);
        assert_eq!(outcomes[0].stage, StageName::EmoteStripper);
        assert_eq!(outcomes[4].stage, StageName::LengthCapper);
    }

    #[test]
    fn pipeline_result_variants() {
        let speak = PipelineResult::Speak("hi".into());
        let skip = PipelineResult::Skip {
            reason: SkipReason::EmptyAfterProcessing,
        };
        assert!(matches!(speak, PipelineResult::Speak(_)));
        assert!(matches!(skip, PipelineResult::Skip { .. }));
    }
}
