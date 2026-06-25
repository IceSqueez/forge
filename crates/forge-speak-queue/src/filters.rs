use std::sync::Arc;

use forge_storage::{FilterRule, FilterRuleKind, TtsPipelineSettings, UrlMode as StorageUrlMode};
use forge_tts_pipeline::{
    BlocklistMode, EmoteSources, EmoteTokenSet, PipelineConfig, PipelineError, ReplacementRule,
    UrlMode,
};

/// Errors produced when translating persisted rules into a validated pipeline config.
///
/// Only produced by the save posture. The boot posture drops invalid rules instead so a
/// hand-edited database can never block startup; the save path rejects so the user sees it.
#[derive(Debug, thiserror::Error)]
pub enum FilterMappingError {
    #[error("rule {index} ({name:?}) has invalid regex pattern `{pattern}`: {source}")]
    InvalidRegex {
        index: usize,
        name: String,
        pattern: String,
        source: regex::Error,
    },
}

impl From<FilterMappingError> for PipelineError {
    fn from(e: FilterMappingError) -> Self {
        match e {
            FilterMappingError::InvalidRegex {
                pattern, source, ..
            } => PipelineError::InvalidRegex { pattern, source },
        }
    }
}

fn storage_url_mode_to_pipeline(mode: StorageUrlMode) -> UrlMode {
    match mode {
        StorageUrlMode::Speak => UrlMode::Passthrough,
        StorageUrlMode::Replace => UrlMode::Replace {
            substitute: "link".to_owned(),
        },
        StorageUrlMode::Suppress => UrlMode::SkipMessage,
    }
}

fn storage_blocklist_mode_to_pipeline(mode: forge_storage::BlocklistMode) -> BlocklistMode {
    match mode {
        forge_storage::BlocklistMode::Censor => BlocklistMode::Censor,
        forge_storage::BlocklistMode::Suppress => BlocklistMode::SkipMessage,
    }
}

/// Translates a single replacement-kind `FilterRule` into its pipeline equivalent.
///
/// Disabled rules and empty literal patterns map to `None`. A regex that fails to
/// compile is returned as `Err`; the caller decides whether to skip or reject it.
fn map_rule_strict(
    rule: &FilterRule,
    index: usize,
) -> Result<Option<ReplacementRule>, FilterMappingError> {
    if !rule.enabled {
        return Ok(None);
    }
    match &rule.kind {
        FilterRuleKind::Literal {
            pattern,
            replacement,
        } => {
            if pattern.is_empty() {
                return Ok(None);
            }
            Ok(Some(ReplacementRule::Text {
                pattern: pattern.clone(),
                replacement: replacement.clone(),
            }))
        }
        FilterRuleKind::Regex {
            pattern,
            replacement,
        } => {
            let compiled =
                regex::Regex::new(pattern).map_err(|source| FilterMappingError::InvalidRegex {
                    index,
                    name: rule.name.clone(),
                    pattern: pattern.clone(),
                    source,
                })?;
            Ok(Some(ReplacementRule::Regex {
                compiled,
                replacement: replacement.clone(),
            }))
        }
        FilterRuleKind::Blocklist { .. } => {
            // Blocklist rules map to the word_blocklist field, not replacement_rules.
            // Collected separately by the caller.
            Ok(None)
        }
    }
}

struct MappedRules {
    replacement_rules: Vec<ReplacementRule>,
    word_blocklist: Vec<String>,
    blocklist_mode: BlocklistMode,
}

/// Translates persisted `FilterRule` rows into pipeline-ready structures.
///
/// `strict = true` (save posture): returns `Err` on the first invalid regex so the UI
/// can reject the save before the live config is touched.
///
/// `strict = false` (boot posture): drops invalid rules with a logged WARN and proceeds;
/// startup never fails on a hand-edited DB entry.
fn map_rules(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
    strict: bool,
) -> Result<MappedRules, FilterMappingError> {
    let mut replacement_rules = Vec::new();
    let mut word_blocklist = Vec::new();
    // Last blocklist rule's mode wins; if no blocklist rule exists, fall back to settings.
    let mut blocklist_mode = storage_blocklist_mode_to_pipeline(settings.blocklist_mode);

    for (index, rule) in rules.iter().enumerate() {
        if !rule.enabled {
            continue;
        }
        match &rule.kind {
            FilterRuleKind::Blocklist { words, mode } => {
                word_blocklist.extend(words.iter().cloned());
                blocklist_mode = storage_blocklist_mode_to_pipeline(*mode);
            }
            _ => match map_rule_strict(rule, index) {
                Ok(Some(r)) => replacement_rules.push(r),
                Ok(None) => {}
                Err(e) if strict => return Err(e),
                Err(e) => {
                    tracing::warn!(
                        rule_index = index,
                        rule_name = %e.name_for_log(),
                        pattern = %e.pattern_for_log(),
                        "invalid regex in filter rule; skipping"
                    );
                }
            },
        }
    }

    Ok(MappedRules {
        replacement_rules,
        word_blocklist,
        blocklist_mode,
    })
}

fn emote_sources_from_settings(settings: &TtsPipelineSettings) -> EmoteSources {
    EmoteSources {
        twitch: settings.strip_twitch_emotes,
        bttv: false,
        ffz: false,
        seven_tv: false,
        emoji: false,
    }
}

/// Translates persisted filter rules and pipeline settings into a validated
/// `PipelineConfig`, failing on the first invalid regex pattern.
///
/// Use this on the settings-save path so invalid regexes are rejected before the
/// live config is touched. The live config is never replaced on `Err`.
pub fn build_config_strict(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
) -> Result<PipelineConfig, FilterMappingError> {
    let mapped = map_rules(rules, settings, true)?;
    let max_chars = settings.max_length.unwrap_or(500) as usize;
    Ok(PipelineConfig::new(
        emote_sources_from_settings(settings),
        EmoteTokenSet::default(),
        storage_url_mode_to_pipeline(settings.url_mode),
        mapped.replacement_rules,
        mapped.word_blocklist,
        mapped.blocklist_mode,
        max_chars,
    ))
}

/// Translates persisted filter rules and pipeline settings into a validated
/// `PipelineConfig`, skipping invalid regex rules rather than failing.
///
/// Boot posture drops invalid rules so startup survives a hand-edited DB; the save
/// path rejects instead so the user sees the offending pattern.
pub fn build_config_lenient(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
) -> PipelineConfig {
    let mapped = map_rules(rules, settings, false).unwrap_or_else(|_| {
        // map_rules with strict=false only returns Err if a bug bypasses the strict guard;
        // fall back to defaults rather than panic.
        MappedRules {
            replacement_rules: vec![],
            word_blocklist: vec![],
            blocklist_mode: storage_blocklist_mode_to_pipeline(settings.blocklist_mode),
        }
    });
    let max_chars = settings.max_length.unwrap_or(500) as usize;
    PipelineConfig::new(
        emote_sources_from_settings(settings),
        EmoteTokenSet::default(),
        storage_url_mode_to_pipeline(settings.url_mode),
        mapped.replacement_rules,
        mapped.word_blocklist,
        mapped.blocklist_mode,
        max_chars,
    )
}

/// Shared, swappable pipeline config.
///
/// Readers clone the inner `Arc<PipelineConfig>` (atomic increment only) under the
/// read guard, then drop the guard before any `.await`. The write guard is held only
/// for the pointer swap, never across an await point.
#[derive(Clone)]
pub struct PipelineConfigHandle(Arc<std::sync::RwLock<Arc<PipelineConfig>>>);

impl PipelineConfigHandle {
    pub fn new(initial: PipelineConfig) -> Self {
        Self(Arc::new(std::sync::RwLock::new(Arc::new(initial))))
    }

    /// Returns an owned `Arc` to the current config without holding the lock.
    pub fn load(&self) -> Arc<PipelineConfig> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Replaces the active config with `next`.
    ///
    /// The write guard is held only for this pointer swap; no async work happens
    /// inside, so the no-lock-across-await invariant is trivially satisfied.
    pub fn swap(&self, next: PipelineConfig) {
        let mut guard = self.0.write().unwrap_or_else(|e| e.into_inner());
        *guard = Arc::new(next);
    }
}

// --- helpers used only inside this module for logging ---

impl FilterMappingError {
    fn name_for_log(&self) -> &str {
        match self {
            FilterMappingError::InvalidRegex { name, .. } => name.as_str(),
        }
    }

    fn pattern_for_log(&self) -> &str {
        match self {
            FilterMappingError::InvalidRegex { pattern, .. } => pattern.as_str(),
        }
    }
}
