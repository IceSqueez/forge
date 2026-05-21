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
    /// The `regex` crate guarantees linear-time matching with no backtracking — no ReDoS risk.
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

fn is_emoji_char(c: char) -> bool {
    let cp = c as u32;
    matches!(
        cp,
        0x2600..=0x27BF
        | 0x1F000..=0x1FAFF
        | 0xFE00..=0xFE0F
        | 0x200D
        | 0x20E3
    )
}

fn stage_emote_stripper(text: &str, config: &PipelineConfig) -> String {
    let stripped = if config.emote_tokens.tokens.is_empty() {
        text.to_owned()
    } else {
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| !config.emote_tokens.tokens.contains(*w))
            .collect();
        words.join(" ")
    };

    if config.emote_sources.emoji {
        stripped.chars().filter(|c| !is_emoji_char(*c)).collect()
    } else {
        stripped
    }
}

#[allow(clippy::expect_used)]
static URL_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"(?:https?|ftp)://\S+").expect("valid url regex")
});

fn stage_url_sanitizer(text: &str, mode: &UrlMode) -> Result<String, SkipReason> {
    match mode {
        UrlMode::Passthrough => Ok(text.to_owned()),
        UrlMode::SkipMessage => {
            if URL_RE.is_match(text) {
                Err(SkipReason::MatchedSkipRule("message contains url"))
            } else {
                Ok(text.to_owned())
            }
        }
        UrlMode::Replace { substitute } => {
            Ok(URL_RE.replace_all(text, substitute.as_str()).into_owned())
        }
    }
}

fn stage_text_replacements(text: &str, rules: &[ReplacementRule]) -> String {
    let mut current = text.to_owned();
    for rule in rules {
        match rule {
            ReplacementRule::Text {
                pattern,
                replacement,
            } => {
                current = case_insensitive_replace(&current, pattern, replacement);
            }
            ReplacementRule::Regex {
                compiled,
                replacement,
            } => {
                current = compiled
                    .replace_all(&current, replacement.as_str())
                    .into_owned();
            }
        }
    }
    current
}

fn case_insensitive_replace(text: &str, pattern: &str, replacement: &str) -> String {
    if pattern.is_empty() {
        return text.to_owned();
    }
    let lower_text = text.to_lowercase();
    let lower_pattern = pattern.to_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut start = 0usize;
    while let Some(pos) = lower_text[start..].find(&lower_pattern) {
        let abs = start + pos;
        result.push_str(&text[start..abs]);
        result.push_str(replacement);
        start = abs + lower_pattern.len();
    }
    result.push_str(&text[start..]);
    result
}

fn stage_word_blocklist(
    text: &str,
    blocklist: &[String],
    mode: &BlocklistMode,
) -> Result<String, SkipReason> {
    if blocklist.is_empty() {
        return Ok(text.to_owned());
    }
    let lower_list: Vec<String> = blocklist.iter().map(|w| w.to_lowercase()).collect();
    let mut result = String::with_capacity(text.len());
    let mut first = true;
    for word in text.split_whitespace() {
        let lower_word = word.to_lowercase();
        if lower_list.iter().any(|b| b == &lower_word) {
            match mode {
                BlocklistMode::SkipMessage => return Err(SkipReason::BlockedByWordFilter),
                BlocklistMode::Censor => {
                    if !first {
                        result.push(' ');
                    }
                    result.push_str("[beep]");
                    first = false;
                    continue;
                }
            }
        }
        if !first {
            result.push(' ');
        }
        result.push_str(word);
        first = false;
    }
    Ok(result)
}

fn stage_length_capper(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}\u{2026}")
    }
}

enum StageOut {
    Ok(String),
    Skip(SkipReason),
}

fn run_stage(stage: StageName, text: &str, config: &PipelineConfig) -> StageOut {
    match stage {
        StageName::EmoteStripper => StageOut::Ok(stage_emote_stripper(text, config)),
        StageName::UrlSanitizer => match stage_url_sanitizer(text, &config.url_mode) {
            Ok(s) => StageOut::Ok(s),
            Err(r) => StageOut::Skip(r),
        },
        StageName::TextReplacements => {
            StageOut::Ok(stage_text_replacements(text, &config.replacement_rules))
        }
        StageName::WordBlocklist => {
            match stage_word_blocklist(text, &config.word_blocklist, &config.blocklist_mode) {
                Ok(s) => StageOut::Ok(s),
                Err(r) => StageOut::Skip(r),
            }
        }
        StageName::LengthCapper => {
            let out = stage_length_capper(text, config.max_chars);
            if out.trim().is_empty() {
                StageOut::Skip(SkipReason::EmptyAfterProcessing)
            } else {
                StageOut::Ok(out)
            }
        }
    }
}

/// Run the full pipeline on `text`.
///
/// Pure function — no I/O, no allocation beyond string manipulation.
/// Never panics. Config must be pre-validated via `PipelineConfig::new`.
pub fn process(text: &str, config: &PipelineConfig) -> PipelineResult {
    let stages = [
        StageName::EmoteStripper,
        StageName::UrlSanitizer,
        StageName::TextReplacements,
        StageName::WordBlocklist,
        StageName::LengthCapper,
    ];
    let mut current = text.to_owned();
    for stage in stages {
        match run_stage(stage, &current, config) {
            StageOut::Ok(s) => current = s,
            StageOut::Skip(r) => return PipelineResult::Skip { reason: r },
        }
    }
    PipelineResult::Speak(current)
}

/// Run the pipeline and return per-stage outcomes for UI preview.
///
/// Returns all five `StageOutcome` entries regardless of early `Skip`.
/// On `Skip`, subsequent stages receive the last non-empty intermediate text
/// but record `StageAction::Skipped`.
pub fn preview(text: &str, config: &PipelineConfig) -> (PipelineResult, Vec<StageOutcome>) {
    let stage_names = [
        StageName::EmoteStripper,
        StageName::UrlSanitizer,
        StageName::TextReplacements,
        StageName::WordBlocklist,
        StageName::LengthCapper,
    ];
    let mut outcomes = Vec::with_capacity(5);
    let mut current = text.to_owned();
    let mut early_skip: Option<SkipReason> = None;

    for name in stage_names {
        let input = current.clone();
        let (output, action) = if let Some(ref reason) = early_skip {
            (
                input.clone(),
                StageAction::Skipped {
                    reason: reason.clone(),
                },
            )
        } else {
            match run_stage(name, &input, config) {
                StageOut::Ok(out) => {
                    let action = if out == input {
                        StageAction::PassedThrough
                    } else {
                        StageAction::Transformed
                    };
                    (out, action)
                }
                StageOut::Skip(reason) => {
                    early_skip = Some(reason.clone());
                    (input.clone(), StageAction::Skipped { reason })
                }
            }
        };
        outcomes.push(StageOutcome {
            stage: name,
            input,
            output: output.clone(),
            action,
        });
        current = output;
    }

    let final_result = if let Some(reason) = early_skip {
        PipelineResult::Skip { reason }
    } else if current.trim().is_empty() {
        PipelineResult::Skip {
            reason: SkipReason::EmptyAfterProcessing,
        }
    } else {
        PipelineResult::Speak(current)
    };

    (final_result, outcomes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn process_passthrough_no_config() {
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
    fn emote_stripper_removes_known_tokens() {
        let mut config = PipelineConfig::default();
        config.emote_tokens.tokens.insert("LUL".into());
        config.emote_tokens.tokens.insert("Pog".into());
        let result = process("hello LUL world Pog nice", &config);
        assert_eq!(result, PipelineResult::Speak("hello world nice".into()));
    }

    #[test]
    fn emote_stripper_strips_emoji() {
        let mut config = PipelineConfig::default();
        config.emote_sources.emoji = true;
        let result = process("hello 🎉 world", &config);
        assert_eq!(result, PipelineResult::Speak("hello  world".into()));
    }

    #[test]
    fn url_sanitizer_replace_mode() {
        let config = PipelineConfig {
            url_mode: UrlMode::Replace {
                substitute: "link".into(),
            },
            ..PipelineConfig::default()
        };
        let result = process("check out https://example.com today", &config);
        assert_eq!(result, PipelineResult::Speak("check out link today".into()));
    }

    #[test]
    fn url_sanitizer_skip_message() {
        let config = PipelineConfig {
            url_mode: UrlMode::SkipMessage,
            ..PipelineConfig::default()
        };
        let result = process("visit http://spam.com", &config);
        assert!(matches!(result, PipelineResult::Skip { .. }));
    }

    #[test]
    fn url_sanitizer_passthrough_leaves_url() {
        let config = PipelineConfig {
            url_mode: UrlMode::Passthrough,
            ..PipelineConfig::default()
        };
        let result = process("visit https://forge.rs", &config);
        assert_eq!(
            result,
            PipelineResult::Speak("visit https://forge.rs".into())
        );
    }

    #[test]
    fn text_replacement_case_insensitive() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::Text {
                pattern: "lol".into(),
                replacement: "(laugh)".into(),
            }],
            ..PipelineConfig::default()
        };
        let result = process("LOL that was funny LoL", &config);
        assert_eq!(
            result,
            PipelineResult::Speak("(laugh) that was funny (laugh)".into())
        );
    }

    #[test]
    fn text_replacement_regex() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::Regex {
                compiled: regex::Regex::new(r"\d+").unwrap(),
                replacement: "#".into(),
            }],
            ..PipelineConfig::default()
        };
        let result = process("I have 42 cats and 7 dogs", &config);
        assert_eq!(
            result,
            PipelineResult::Speak("I have # cats and # dogs".into())
        );
    }

    #[test]
    fn word_blocklist_censor_mode() {
        let config = PipelineConfig {
            word_blocklist: vec!["badword".into()],
            blocklist_mode: BlocklistMode::Censor,
            ..PipelineConfig::default()
        };
        let result = process("this is badword here", &config);
        assert_eq!(result, PipelineResult::Speak("this is [beep] here".into()));
    }

    #[test]
    fn word_blocklist_skip_message_mode() {
        let config = PipelineConfig {
            word_blocklist: vec!["badword".into()],
            blocklist_mode: BlocklistMode::SkipMessage,
            ..PipelineConfig::default()
        };
        let result = process("contains badword here", &config);
        assert!(matches!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::BlockedByWordFilter
            }
        ));
    }

    #[test]
    fn word_blocklist_case_insensitive() {
        let config = PipelineConfig {
            word_blocklist: vec!["badword".into()],
            blocklist_mode: BlocklistMode::Censor,
            ..PipelineConfig::default()
        };
        let result = process("BADWORD in caps", &config);
        assert_eq!(result, PipelineResult::Speak("[beep] in caps".into()));
    }

    #[test]
    fn length_capper_truncates_with_ellipsis() {
        let config = PipelineConfig {
            max_chars: 5,
            ..PipelineConfig::default()
        };
        let result = process("hello world", &config);
        assert_eq!(result, PipelineResult::Speak("hello\u{2026}".into()));
    }

    #[test]
    fn length_capper_no_truncation_if_fits() {
        let config = PipelineConfig {
            max_chars: 100,
            ..PipelineConfig::default()
        };
        let result = process("short", &config);
        assert_eq!(result, PipelineResult::Speak("short".into()));
    }

    #[test]
    fn preview_all_stages_recorded_on_skip() {
        let config = PipelineConfig {
            url_mode: UrlMode::SkipMessage,
            ..PipelineConfig::default()
        };
        let (result, outcomes) = preview("visit https://example.com", &config);
        assert_eq!(outcomes.len(), 5);
        assert!(matches!(result, PipelineResult::Skip { .. }));
        assert_eq!(outcomes[1].stage, StageName::UrlSanitizer);
        assert!(matches!(outcomes[1].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[2].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[3].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[4].action, StageAction::Skipped { .. }));
    }

    #[test]
    fn preview_stage_input_output_chain() {
        let mut config = PipelineConfig::default();
        config.emote_tokens.tokens.insert("LUL".into());
        let (result, outcomes) = preview("hello LUL world", &config);
        assert_eq!(result, PipelineResult::Speak("hello world".into()));
        assert_eq!(outcomes[0].output, "hello world");
        assert_eq!(outcomes[1].input, "hello world");
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
