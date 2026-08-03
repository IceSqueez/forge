use std::sync::{Arc, LazyLock};

use forge_storage::{FilterRule, FilterRuleKind, TtsPipelineSettings, UrlMode as StorageUrlMode};
use forge_tts_pipeline::{
    BlocklistMode, EmoteSources, EmoteTokenSet, OutputConfig, PipelineConfig, PipelineError,
    ReplacementRule, SkipRulesConfig,
};
use forge_types::Shared;

/// Only produced by the save posture; the boot posture drops invalid rules instead.
#[derive(Debug, thiserror::Error)]
pub enum FilterMappingError {
    #[error("rule {index} ({name:?}) has invalid regex pattern `{pattern}`: {source}")]
    InvalidRegex {
        index: usize,
        name: String,
        pattern: String,
        source: regex::Error,
    },
    #[error("skip rule custom regex #{index} has invalid pattern `{pattern}`: {source}")]
    InvalidSkipRegex {
        index: usize,
        pattern: String,
        source: regex::Error,
    },
}

impl From<FilterMappingError> for PipelineError {
    fn from(e: FilterMappingError) -> Self {
        match e {
            FilterMappingError::InvalidRegex {
                pattern, source, ..
            }
            | FilterMappingError::InvalidSkipRegex {
                pattern, source, ..
            } => PipelineError::InvalidRegex { pattern, source },
        }
    }
}

#[allow(clippy::expect_used)]
static MIGRATED_URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"https?://\S+").expect("static regex"));

fn migrate_url_mode(
    mode: StorageUrlMode,
    replacement_rules: &mut Vec<ReplacementRule>,
    skip_rules: &mut SkipRulesConfig,
) {
    match mode {
        StorageUrlMode::Speak => {}
        StorageUrlMode::Replace => {
            replacement_rules.insert(
                0,
                ReplacementRule::Regex {
                    compiled: MIGRATED_URL_REGEX.clone(),
                    replacement: "link".to_owned(),
                },
            );
        }
        StorageUrlMode::Suppress => {
            skip_rules.contains_url = true;
        }
    }
}

fn migrate_max_length(max_length: Option<u32>, skip_rules: &mut SkipRulesConfig) {
    if let Some(n) = max_length {
        skip_rules.longer_than = true;
        skip_rules.max_chars = n as usize;
    }
}

fn effective_skip_prefix(settings: &TtsPipelineSettings) -> Option<String> {
    match &settings.skip_prefix {
        Some(prefix) if !prefix.is_empty() => Some(prefix.clone()),
        _ if settings.skip_starts_with_bang => Some("!".to_owned()),
        _ => None,
    }
}

fn skip_rules_base(settings: &TtsPipelineSettings) -> SkipRulesConfig {
    SkipRulesConfig {
        contains_url: settings.skip_contains_url,
        skip_prefix: effective_skip_prefix(settings),
        from_bot_accounts: settings.skip_from_bot_accounts,
        bot_accounts: settings.bot_accounts.clone(),
        longer_than: settings.skip_longer_than,
        max_chars: settings.longer_than_max_chars as usize,
        repeat_of_recent: settings.skip_repeat_of_recent,
        window: settings.repeat_of_recent_window as usize,
        emote_only: settings.skip_emote_only,
        mostly_non_latin: settings.skip_mostly_non_latin,
        custom_regexes: Vec::new(),
    }
}

fn compile_skip_regexes_lenient(patterns: &[String]) -> Vec<regex::Regex> {
    patterns
        .iter()
        .enumerate()
        .filter_map(|(index, pattern)| match regex::Regex::new(pattern) {
            Ok(compiled) => Some(compiled),
            Err(source) => {
                tracing::warn!(
                    regex_index = index,
                    pattern = %pattern,
                    error = %source,
                    "invalid skip custom regex; dropping"
                );
                None
            }
        })
        .collect()
}

fn compile_skip_regexes_strict(
    patterns: &[String],
) -> Result<Vec<regex::Regex>, FilterMappingError> {
    patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            regex::Regex::new(pattern).map_err(|source| FilterMappingError::InvalidSkipRegex {
                index,
                pattern: pattern.clone(),
                source,
            })
        })
        .collect()
}

fn skip_rules_from_settings_lenient(settings: &TtsPipelineSettings) -> SkipRulesConfig {
    let mut skip_rules = skip_rules_base(settings);
    skip_rules.custom_regexes = compile_skip_regexes_lenient(&settings.skip_custom_regexes);
    skip_rules
}

fn skip_rules_from_settings_strict(
    settings: &TtsPipelineSettings,
) -> Result<SkipRulesConfig, FilterMappingError> {
    let mut skip_rules = skip_rules_base(settings);
    skip_rules.custom_regexes = compile_skip_regexes_strict(&settings.skip_custom_regexes)?;
    Ok(skip_rules)
}

fn output_from_settings(settings: &TtsPipelineSettings) -> OutputConfig {
    OutputConfig {
        read_display_name_first: settings.output_read_display_name_first,
        emote_to_word: settings.output_emote_to_word,
        sanitize_punctuation: settings.output_sanitize_punctuation,
        max_duration_secs: settings.output_max_duration_secs,
    }
}

fn storage_blocklist_mode_to_pipeline(mode: forge_storage::BlocklistMode) -> BlocklistMode {
    match mode {
        forge_storage::BlocklistMode::Censor => BlocklistMode::Censor,
        forge_storage::BlocklistMode::Suppress => BlocklistMode::SkipMessage,
    }
}

/// Disabled rules and empty literal patterns map to `None`; a bad regex maps to `Err`.
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
            // Collected into word_blocklist separately by the caller, not returned here.
            Ok(None)
        }
    }
}

struct MappedRules {
    replacement_rules: Vec<ReplacementRule>,
    word_blocklist: Vec<String>,
    blocklist_mode: BlocklistMode,
}

/// `strict = true` rejects on the first invalid regex; `strict = false` drops it and logs.
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

/// Rejects on the first invalid regex; the live config is never replaced on `Err`.
pub fn build_config_strict(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
) -> Result<PipelineConfig, FilterMappingError> {
    let mapped = map_rules(rules, settings, true)?;
    let mut replacement_rules = mapped.replacement_rules;
    let mut skip_rules = skip_rules_from_settings_strict(settings)?;
    migrate_url_mode(settings.url_mode, &mut replacement_rules, &mut skip_rules);
    migrate_max_length(settings.max_length, &mut skip_rules);
    Ok(PipelineConfig::new(
        emote_sources_from_settings(settings),
        EmoteTokenSet::default(),
        skip_rules,
        replacement_rules,
        mapped.word_blocklist,
        mapped.blocklist_mode,
        output_from_settings(settings),
        settings.strip_reward_emotes,
    ))
}

/// Drops invalid regex rules and logs, so startup survives a hand-edited DB.
pub fn build_config_lenient(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
) -> PipelineConfig {
    let mapped = map_rules(rules, settings, false).unwrap_or_else(|_| {
        // Unreachable under strict=false unless the strict guard is bypassed by a bug.
        MappedRules {
            replacement_rules: vec![],
            word_blocklist: vec![],
            blocklist_mode: storage_blocklist_mode_to_pipeline(settings.blocklist_mode),
        }
    });
    let mut replacement_rules = mapped.replacement_rules;
    let mut skip_rules = skip_rules_from_settings_lenient(settings);
    migrate_url_mode(settings.url_mode, &mut replacement_rules, &mut skip_rules);
    migrate_max_length(settings.max_length, &mut skip_rules);
    PipelineConfig::new(
        emote_sources_from_settings(settings),
        EmoteTokenSet::default(),
        skip_rules,
        replacement_rules,
        mapped.word_blocklist,
        mapped.blocklist_mode,
        output_from_settings(settings),
        settings.strip_reward_emotes,
    )
}

#[derive(Clone)]
pub struct PipelineConfigHandle(Shared<PipelineConfig>);

impl PipelineConfigHandle {
    pub fn new(initial: PipelineConfig) -> Self {
        Self(Shared::new(initial))
    }

    pub fn load(&self) -> Arc<PipelineConfig> {
        self.0.load()
    }

    pub fn swap(&self, next: PipelineConfig) {
        self.0.store(next);
    }
}

impl FilterMappingError {
    fn name_for_log(&self) -> &str {
        match self {
            FilterMappingError::InvalidRegex { name, .. } => name.as_str(),
            FilterMappingError::InvalidSkipRegex { .. } => "skip rule",
        }
    }

    fn pattern_for_log(&self) -> &str {
        match self {
            FilterMappingError::InvalidRegex { pattern, .. }
            | FilterMappingError::InvalidSkipRegex { pattern, .. } => pattern.as_str(),
        }
    }
}
