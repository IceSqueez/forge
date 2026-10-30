use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock};

use forge_storage::{FilterRule, FilterRuleKind, TtsPipelineSettings, UrlMode as StorageUrlMode};
use forge_tts_pipeline::{
    BlocklistMode, EmoteTokenSet, EmoteTreatment, OutputConfig, PipelineConfig, ReplacementRule,
    SkipRulesConfig,
};
use forge_types::Shared;

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

#[expect(clippy::expect_used, reason = "literal pattern is a valid regex")]
static MIGRATED_URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"https?://\S+").expect("static regex"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementSource {
    FilterRule(usize),
    UrlMode,
}

const BANG_COMMAND_PREFIX: &str = "!";

pub struct BuiltConfig {
    config: PipelineConfig,
    replacement_sources: Vec<ReplacementSource>,
    rejected: Vec<FilterMappingError>,
}

impl BuiltConfig {
    pub fn require_valid(self) -> Result<PipelineConfig, FilterMappingError> {
        match self.rejected.into_iter().next() {
            Some(error) => Err(error),
            None => Ok(self.config),
        }
    }

    pub fn dropping_invalid(self) -> (PipelineConfig, Vec<ReplacementSource>) {
        for error in &self.rejected {
            tracing::warn!(
                rule_index = error.index(),
                rule_name = %error.name_for_log(),
                pattern = %error.pattern_for_log(),
                "invalid regex in TTS filter settings; skipping"
            );
        }
        (self.config, self.replacement_sources)
    }
}

pub fn build_config(rules: &[FilterRule], settings: &TtsPipelineSettings) -> BuiltConfig {
    let (mut mapped, mut rejected) = map_rules(rules, settings);
    let (custom_regexes, skip_regex_errors) = compile_skip_regexes(&settings.skip_custom_regexes);
    rejected.extend(skip_regex_errors);
    let mut skip_rules = SkipRulesConfig {
        contains_url: settings.skip_contains_url,
        skip_prefix: effective_skip_prefix(settings),
        from_bot_accounts: settings.skip_from_bot_accounts,
        bot_accounts: settings.bot_accounts.clone(),
        max_chars: max_chars(settings),
        repeat_of_recent: settings.skip_repeat_of_recent,
        emote_only: settings.skip_emote_only,
        mostly_non_latin: settings.skip_mostly_non_latin,
        custom_regexes,
    };
    apply_url_mode(settings.url_mode, &mut mapped, &mut skip_rules);
    let config = PipelineConfig {
        emote_tokens: EmoteTokenSet::default(),
        skip_rules,
        replacement_rules: mapped.replacement_rules,
        word_blocklist: mapped.word_blocklist,
        blocklist_mode: mapped.blocklist_mode,
        output: output_from_settings(settings),
        strip_reward_emotes: settings.strip_reward_emotes,
    };
    BuiltConfig {
        config,
        replacement_sources: mapped.replacement_sources,
        rejected,
    }
}

pub fn emote_treatment(settings: &TtsPipelineSettings) -> EmoteTreatment {
    if settings.output_emote_to_word {
        EmoteTreatment::ToWord
    } else if settings.strip_twitch_emotes {
        EmoteTreatment::StripTwitch
    } else {
        EmoteTreatment::Keep
    }
}

pub fn store_emote_treatment(settings: &mut TtsPipelineSettings, treatment: EmoteTreatment) {
    settings.output_emote_to_word = treatment == EmoteTreatment::ToWord;
    settings.strip_twitch_emotes = treatment == EmoteTreatment::StripTwitch;
}

pub fn effective_skip_prefix(settings: &TtsPipelineSettings) -> Option<String> {
    match &settings.skip_prefix {
        Some(prefix) if !prefix.is_empty() => Some(prefix.clone()),
        _ if settings.skip_starts_with_bang => Some(BANG_COMMAND_PREFIX.to_owned()),
        _ => None,
    }
}

pub fn repeat_window(settings: &TtsPipelineSettings) -> NonZeroUsize {
    NonZeroUsize::new(settings.repeat_of_recent_window as usize).unwrap_or(NonZeroUsize::MIN)
}

fn max_chars(settings: &TtsPipelineSettings) -> Option<usize> {
    settings
        .max_length
        .or(settings
            .skip_longer_than
            .then_some(settings.longer_than_max_chars))
        .map(|chars| chars as usize)
}

fn apply_url_mode(
    mode: StorageUrlMode,
    mapped: &mut MappedRules,
    skip_rules: &mut SkipRulesConfig,
) {
    match mode {
        StorageUrlMode::Speak => {}
        StorageUrlMode::Replace => {
            mapped.replacement_rules.insert(
                0,
                ReplacementRule::LinkWord {
                    compiled: MIGRATED_URL_REGEX.clone(),
                },
            );
            mapped
                .replacement_sources
                .insert(0, ReplacementSource::UrlMode);
        }
        StorageUrlMode::Suppress => {
            skip_rules.contains_url = true;
        }
    }
}

fn compile_skip_regexes(patterns: &[String]) -> (Vec<regex::Regex>, Vec<FilterMappingError>) {
    let mut compiled = Vec::with_capacity(patterns.len());
    let mut rejected = Vec::new();
    for (index, pattern) in patterns.iter().enumerate() {
        match regex::Regex::new(pattern) {
            Ok(regex) => compiled.push(regex),
            Err(source) => rejected.push(FilterMappingError::InvalidSkipRegex {
                index,
                pattern: pattern.clone(),
                source,
            }),
        }
    }
    (compiled, rejected)
}

fn output_from_settings(settings: &TtsPipelineSettings) -> OutputConfig {
    OutputConfig {
        read_display_name_first: settings.output_read_display_name_first,
        emote_treatment: emote_treatment(settings),
        strip_emoji: settings.strip_emoji,
        sanitize_punctuation: settings.output_sanitize_punctuation,
        max_duration_secs: settings.output_max_duration_secs,
        language_aware_voice: settings.output_language_aware_voice.unwrap_or(false),
    }
}

fn storage_blocklist_mode_to_pipeline(mode: forge_storage::BlocklistMode) -> BlocklistMode {
    match mode {
        forge_storage::BlocklistMode::Censor => BlocklistMode::Censor,
        forge_storage::BlocklistMode::Suppress => BlocklistMode::SkipMessage,
    }
}

fn map_replacement_rule(
    rule: &FilterRule,
    index: usize,
) -> Result<Option<ReplacementRule>, FilterMappingError> {
    match &rule.kind {
        FilterRuleKind::Literal {
            pattern,
            replacement,
            match_inside_words,
        } => {
            if pattern.is_empty() {
                return Ok(None);
            }
            let pattern = pattern.clone();
            let replacement = replacement.clone();
            Ok(Some(if *match_inside_words {
                ReplacementRule::Text {
                    pattern,
                    replacement,
                }
            } else {
                ReplacementRule::WholeWord {
                    pattern,
                    replacement,
                }
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
        FilterRuleKind::Blocklist { .. } => Ok(None),
    }
}

struct MappedRules {
    replacement_rules: Vec<ReplacementRule>,
    replacement_sources: Vec<ReplacementSource>,
    word_blocklist: Vec<String>,
    blocklist_mode: BlocklistMode,
}

fn map_rules(
    rules: &[FilterRule],
    settings: &TtsPipelineSettings,
) -> (MappedRules, Vec<FilterMappingError>) {
    let mut replacement_rules = Vec::new();
    let mut replacement_sources = Vec::new();
    let mut word_blocklist = Vec::new();
    let mut blocklist_mode = storage_blocklist_mode_to_pipeline(settings.blocklist_mode);
    let mut invalid = Vec::new();

    for (index, rule) in rules.iter().enumerate() {
        if !rule.enabled {
            continue;
        }
        match &rule.kind {
            FilterRuleKind::Blocklist { words, mode } => {
                word_blocklist.extend(words.iter().cloned());
                blocklist_mode = storage_blocklist_mode_to_pipeline(*mode);
            }
            _ => match map_replacement_rule(rule, index) {
                Ok(Some(r)) => {
                    replacement_rules.push(r);
                    replacement_sources.push(ReplacementSource::FilterRule(index));
                }
                Ok(None) => {}
                Err(e) => invalid.push(e),
            },
        }
    }

    let mapped = MappedRules {
        replacement_rules,
        replacement_sources,
        word_blocklist,
        blocklist_mode,
    };
    (mapped, invalid)
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
    fn index(&self) -> usize {
        match self {
            FilterMappingError::InvalidRegex { index, .. }
            | FilterMappingError::InvalidSkipRegex { index, .. } => *index,
        }
    }

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
