use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

mod language;

pub use language::{DetectionOutcome, LanguageCode, LanguageDetector};

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

#[derive(Debug, Clone)]
pub struct StageOutcome {
    pub stage: StageName,
    pub input: String,
    pub output: String,
    pub action: StageAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageName {
    SkipRules,
    WordBlocklist,
    TextReplacements,
    Output,
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

#[derive(Debug, Clone, Default)]
pub struct EmoteTokenSet {
    pub tokens: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum ReplacementRule {
    Text {
        pattern: String,
        replacement: String,
    },
    /// The `regex` crate guarantees linear-time matching with no backtracking - no ReDoS risk.
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

#[derive(Debug, Clone)]
pub struct SkipRulesConfig {
    pub contains_url: bool,
    pub skip_prefix: Option<String>,
    pub from_bot_accounts: bool,
    pub bot_accounts: Vec<String>,
    pub longer_than: bool,
    pub max_chars: usize,
    pub repeat_of_recent: bool,
    pub window: usize,
    pub emote_only: bool,
    pub mostly_non_latin: bool,
    pub custom_regexes: Vec<regex::Regex>,
}

impl Default for SkipRulesConfig {
    fn default() -> Self {
        Self {
            contains_url: false,
            skip_prefix: None,
            from_bot_accounts: false,
            bot_accounts: Vec::new(),
            longer_than: false,
            max_chars: 200,
            repeat_of_recent: false,
            window: 3,
            emote_only: false,
            mostly_non_latin: false,
            custom_regexes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutputConfig {
    pub read_display_name_first: bool,
    pub emote_to_word: bool,
    pub sanitize_punctuation: bool,
    pub max_duration_secs: Option<u32>,
    pub language_aware_voice: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineContext<'a> {
    pub viewer_name: &'a str,
    pub recent_messages: &'a [String],
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("invalid regex pattern `{pattern}`: {source}")]
    InvalidRegex {
        pattern: String,
        source: regex::Error,
    },
}

/// Holds pre-compiled `Regex` objects, so construction is fallible.
#[derive(Debug, Clone, Default)]
pub struct PipelineConfig {
    pub emote_sources: EmoteSources,
    pub emote_tokens: EmoteTokenSet,
    pub skip_rules: SkipRulesConfig,
    pub replacement_rules: Vec<ReplacementRule>,
    pub word_blocklist: Vec<String>,
    pub blocklist_mode: BlocklistMode,
    pub output: OutputConfig,
    pub strip_reward_emotes: bool,
}

impl PipelineConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        emote_sources: EmoteSources,
        emote_tokens: EmoteTokenSet,
        skip_rules: SkipRulesConfig,
        replacement_rules: Vec<ReplacementRule>,
        word_blocklist: Vec<String>,
        blocklist_mode: BlocklistMode,
        output: OutputConfig,
        strip_reward_emotes: bool,
    ) -> Self {
        Self {
            emote_sources,
            emote_tokens,
            skip_rules,
            replacement_rules,
            word_blocklist,
            blocklist_mode,
            output,
            strip_reward_emotes,
        }
    }
}

const BUILTIN_BOT_ACCOUNTS: &[&str] = &[
    "nightbot",
    "streamelements",
    "moobot",
    "fossabot",
    "streamlabs",
    "wizebot",
    "botisimo",
];

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

pub fn strip_emote_tokens(text: &str, tokens: &EmoteTokenSet) -> String {
    if tokens.tokens.is_empty() {
        return text.to_owned();
    }
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|w| !tokens.tokens.contains(*w))
        .collect();
    words.join(" ")
}

#[allow(clippy::expect_used)]
static URL_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?:https?|ftp)://\S+").expect("static regex"));

#[allow(clippy::expect_used)]
static COLON_EMOTE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r":([A-Za-z0-9_]+):").expect("static regex"));

fn colon_tokens_to_words(text: &str) -> String {
    COLON_EMOTE_RE.replace_all(text, "$1").into_owned()
}

fn is_bot_account(viewer_name: &str, extra: &[String]) -> bool {
    let lower = viewer_name.to_lowercase();
    BUILTIN_BOT_ACCOUNTS.iter().any(|b| *b == lower)
        || extra.iter().any(|b| b.to_lowercase() == lower)
}

fn is_repeat_of_recent(text: &str, recent: &[String]) -> bool {
    let trimmed = text.trim();
    recent.iter().any(|r| r.trim() == trimmed)
}

fn is_colon_emote_token(token: &str) -> bool {
    COLON_EMOTE_RE
        .find(token)
        .is_some_and(|m| m.start() == 0 && m.end() == token.len())
}

fn is_emote_only(text: &str, tokens: &EmoteTokenSet) -> bool {
    let mut saw_token = false;
    for word in text.split_whitespace() {
        saw_token = true;
        if !tokens.tokens.contains(word) && !is_colon_emote_token(word) {
            return false;
        }
    }
    saw_token
}

fn is_latin_alpha(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c as u32, 0x00C0..=0x024F | 0x1E00..=0x1EFF)
}

fn is_mostly_non_latin(text: &str) -> bool {
    let mut latin = 0usize;
    let mut non_latin = 0usize;
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        if is_latin_alpha(c) {
            latin += 1;
        } else {
            non_latin += 1;
        }
    }
    non_latin > latin
}

fn stage_skip_rules(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
) -> Option<SkipReason> {
    let skip_rules = &config.skip_rules;
    if skip_rules.contains_url && URL_RE.is_match(text) {
        return Some(SkipReason::MatchedSkipRule("message contains a url"));
    }
    if let Some(prefix) = skip_rules.skip_prefix.as_deref()
        && !prefix.is_empty()
        && text.starts_with(prefix)
    {
        return Some(SkipReason::MatchedSkipRule("message starts with a prefix"));
    }
    if skip_rules.from_bot_accounts && is_bot_account(context.viewer_name, &skip_rules.bot_accounts)
    {
        return Some(SkipReason::MatchedSkipRule("message is from a bot account"));
    }
    if skip_rules.longer_than && text.chars().count() > skip_rules.max_chars {
        return Some(SkipReason::MatchedSkipRule("message exceeds max length"));
    }
    if skip_rules.repeat_of_recent && is_repeat_of_recent(text, context.recent_messages) {
        return Some(SkipReason::MatchedSkipRule(
            "message repeats a recent message",
        ));
    }
    if skip_rules.emote_only && is_emote_only(text, &config.emote_tokens) {
        return Some(SkipReason::MatchedSkipRule("message is emote-only"));
    }
    if skip_rules.mostly_non_latin && is_mostly_non_latin(text) {
        return Some(SkipReason::MatchedSkipRule(
            "message is mostly non-latin script",
        ));
    }
    if skip_rules.custom_regexes.iter().any(|re| re.is_match(text)) {
        return Some(SkipReason::MatchedSkipRule(
            "message matches a custom skip regex",
        ));
    }
    None
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

fn transform_emotes(text: &str, config: &PipelineConfig) -> String {
    if config.output.emote_to_word {
        colon_tokens_to_words(text)
    } else if config.emote_sources.twitch {
        strip_emote_tokens(text, &config.emote_tokens)
    } else {
        text.to_owned()
    }
}

fn sanitize_punctuation(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last: Option<char> = None;
    for c in text.chars() {
        if c.is_ascii_punctuation() && last == Some(c) {
            continue;
        }
        result.push(c);
        last = Some(c);
    }
    result
}

fn stage_output(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
) -> String {
    let emote_pass = transform_emotes(text, config);
    let emoji_pass: String = if config.emote_sources.emoji {
        emote_pass.chars().filter(|c| !is_emoji_char(*c)).collect()
    } else {
        emote_pass
    };
    let named = if prepend_display_name {
        format!("{} says: {}", context.viewer_name, emoji_pass)
    } else {
        emoji_pass
    };
    if config.output.sanitize_punctuation {
        sanitize_punctuation(&named)
    } else {
        named
    }
}

enum StageOut {
    Ok(String),
    Skip(SkipReason),
}

fn run_stage(
    stage: StageName,
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
) -> StageOut {
    match stage {
        StageName::SkipRules => match stage_skip_rules(text, config, context) {
            Some(reason) => StageOut::Skip(reason),
            None => StageOut::Ok(text.to_owned()),
        },
        StageName::WordBlocklist => {
            match stage_word_blocklist(text, &config.word_blocklist, &config.blocklist_mode) {
                Ok(s) => StageOut::Ok(s),
                Err(r) => StageOut::Skip(r),
            }
        }
        StageName::TextReplacements => {
            StageOut::Ok(stage_text_replacements(text, &config.replacement_rules))
        }
        StageName::Output => {
            StageOut::Ok(stage_output(text, config, context, prepend_display_name))
        }
    }
}

const STAGES: [StageName; 4] = [
    StageName::SkipRules,
    StageName::WordBlocklist,
    StageName::TextReplacements,
    StageName::Output,
];

/// Never panics; `config` must be pre-validated via `PipelineConfig::new`.
pub fn process(text: &str, config: &PipelineConfig, context: &PipelineContext) -> PipelineResult {
    run_stages(text, config, context, config.output.read_display_name_first)
}

/// The spoken text with the display-name prefix suppressed; `None` when the message is
/// skipped. A viewer name in front of a 5-word message dominates a language sample.
pub fn process_for_language(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
) -> Option<String> {
    match run_stages(text, config, context, false) {
        PipelineResult::Speak(spoken) => Some(spoken),
        PipelineResult::Skip { .. } => None,
    }
}

fn run_stages(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
) -> PipelineResult {
    let mut current = text.to_owned();
    for stage in STAGES {
        match run_stage(stage, &current, config, context, prepend_display_name) {
            StageOut::Ok(s) => current = s,
            StageOut::Skip(r) => return PipelineResult::Skip { reason: r },
        }
    }
    if current.trim().is_empty() {
        return PipelineResult::Skip {
            reason: SkipReason::EmptyAfterProcessing,
        };
    }
    PipelineResult::Speak(current)
}

pub fn preview(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
) -> (PipelineResult, Vec<StageOutcome>) {
    let mut outcomes = Vec::with_capacity(STAGES.len());
    let mut current = text.to_owned();
    let mut early_skip: Option<SkipReason> = None;

    for name in STAGES {
        let input = current.clone();
        let (output, action) = if let Some(ref reason) = early_skip {
            (
                input.clone(),
                StageAction::Skipped {
                    reason: reason.clone(),
                },
            )
        } else {
            match run_stage(
                name,
                &input,
                config,
                context,
                config.output.read_display_name_first,
            ) {
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

    fn ctx() -> PipelineContext<'static> {
        PipelineContext {
            viewer_name: "viewer",
            recent_messages: &[],
        }
    }

    #[test]
    fn process_passthrough_no_config() {
        let config = PipelineConfig::default();
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello world".into()));
    }

    #[test]
    fn preview_returns_four_stages() {
        let config = PipelineConfig::default();
        let (_result, outcomes) = preview("test", &config, &ctx());
        assert_eq!(outcomes.len(), 4);
        assert_eq!(outcomes[0].stage, StageName::SkipRules);
        assert_eq!(outcomes[3].stage, StageName::Output);
    }

    #[test]
    fn skip_rules_contains_url() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                contains_url: true,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let result = process("visit https://spam.com", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule("message contains a url")
            }
        );
    }

    #[test]
    fn skip_rules_starts_with_bang() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                skip_prefix: Some("!".into()),
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let result = process("!command arg", &config, &ctx());
        assert!(matches!(result, PipelineResult::Skip { .. }));
        let passthrough = process("command arg", &config, &ctx());
        assert_eq!(passthrough, PipelineResult::Speak("command arg".into()));
    }

    #[test]
    fn skip_rules_from_bot_accounts_matches_builtin_and_user_list() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                from_bot_accounts: true,
                bot_accounts: vec!["custombot".into()],
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let builtin_ctx = PipelineContext {
            viewer_name: "NightBot",
            recent_messages: &[],
        };
        assert!(matches!(
            process("hi chat", &config, &builtin_ctx),
            PipelineResult::Skip { .. }
        ));
        let custom_ctx = PipelineContext {
            viewer_name: "CustomBot",
            recent_messages: &[],
        };
        assert!(matches!(
            process("hi chat", &config, &custom_ctx),
            PipelineResult::Skip { .. }
        ));
        let human_ctx = PipelineContext {
            viewer_name: "a_real_viewer",
            recent_messages: &[],
        };
        assert_eq!(
            process("hi chat", &config, &human_ctx),
            PipelineResult::Speak("hi chat".into())
        );
    }

    #[test]
    fn skip_rules_longer_than_skips_instead_of_truncating() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                longer_than: true,
                max_chars: 5,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let result = process("hello world", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule("message exceeds max length")
            }
        );
        let fits = process("hello", &config, &ctx());
        assert_eq!(fits, PipelineResult::Speak("hello".into()));
    }

    #[test]
    fn skip_rules_repeat_of_recent_is_trimmed_case_sensitive() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                repeat_of_recent: true,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let recent = vec!["hello chat".to_owned()];
        let repeat_ctx = PipelineContext {
            viewer_name: "viewer",
            recent_messages: &recent,
        };
        assert!(matches!(
            process("  hello chat  ", &config, &repeat_ctx),
            PipelineResult::Skip { .. }
        ));
        assert_eq!(
            process("Hello chat", &config, &repeat_ctx),
            PipelineResult::Speak("Hello chat".into()),
            "case-sensitive - different casing must not match"
        );
    }

    #[test]
    fn emote_stripper_removes_known_tokens_in_output_stage() {
        let mut config = PipelineConfig::default();
        config.emote_sources.twitch = true;
        config.emote_tokens.tokens.insert("LUL".into());
        config.emote_tokens.tokens.insert("Pog".into());
        let result = process("hello LUL world Pog nice", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello world nice".into()));
    }

    #[test]
    fn emote_stripper_strips_emoji() {
        let mut config = PipelineConfig::default();
        config.emote_sources.emoji = true;
        let result = process("hello 🎉 world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello  world".into()));
    }

    #[test]
    fn output_emote_to_word_converts_colon_tokens_and_keeps_known_tokens() {
        let mut config = PipelineConfig {
            output: OutputConfig {
                emote_to_word: true,
                ..OutputConfig::default()
            },
            ..PipelineConfig::default()
        };
        config.emote_sources.twitch = true;
        config.emote_tokens.tokens.insert("LUL".into());
        let result = process("hello :pog: LUL world", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("hello pog LUL world".into()),
            "colon tokens become bare words; known-list tokens are left as spoken words"
        );
    }

    #[test]
    fn output_read_display_name_first_prefixes_viewer_name() {
        let config = PipelineConfig {
            output: OutputConfig {
                read_display_name_first: true,
                ..OutputConfig::default()
            },
            ..PipelineConfig::default()
        };
        let context = PipelineContext {
            viewer_name: "koval_dev",
            recent_messages: &[],
        };
        let result = process("hi chat", &config, &context);
        assert_eq!(
            result,
            PipelineResult::Speak("koval_dev says: hi chat".into())
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
        let result = process("LOL that was funny LoL", &config, &ctx());
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
        let result = process("I have 42 cats and 7 dogs", &config, &ctx());
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
        let result = process("this is badword here", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("this is [beep] here".into()));
    }

    #[test]
    fn word_blocklist_skip_message_mode() {
        let config = PipelineConfig {
            word_blocklist: vec!["badword".into()],
            blocklist_mode: BlocklistMode::SkipMessage,
            ..PipelineConfig::default()
        };
        let result = process("contains badword here", &config, &ctx());
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
        let result = process("BADWORD in caps", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("[beep] in caps".into()));
    }

    #[test]
    fn preview_all_stages_recorded_on_skip() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                contains_url: true,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        let (result, outcomes) = preview("visit https://example.com", &config, &ctx());
        assert_eq!(outcomes.len(), 4);
        assert!(matches!(result, PipelineResult::Skip { .. }));
        assert_eq!(outcomes[0].stage, StageName::SkipRules);
        assert!(matches!(outcomes[0].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[1].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[2].action, StageAction::Skipped { .. }));
        assert!(matches!(outcomes[3].action, StageAction::Skipped { .. }));
    }

    #[test]
    fn preview_stage_input_output_chain() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::Text {
                pattern: "world".into(),
                replacement: "forge".into(),
            }],
            ..PipelineConfig::default()
        };
        let (result, outcomes) = preview("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello forge".into()));
        assert_eq!(outcomes[1].output, "hello world");
        assert_eq!(outcomes[2].input, "hello world");
        assert_eq!(outcomes[2].output, "hello forge");
    }

    #[test]
    fn strip_emote_tokens_removes_whole_word_matches_only() {
        let mut set = EmoteTokenSet::default();
        set.tokens.insert("LUL".into());
        set.tokens.insert("PogChamp".into());
        for (input, expected) in [
            ("hello LUL world PogChamp", "hello world"),
            ("LUL", ""),
            ("no emotes here", "no emotes here"),
            ("LULzy aPogChamp", "LULzy aPogChamp"),
            ("PogChamp LUL PogChamp", ""),
        ] {
            assert_eq!(strip_emote_tokens(input, &set), expected, "input {input:?}",);
        }
    }

    #[test]
    fn strip_emote_tokens_with_empty_set_preserves_original_spacing() {
        let set = EmoteTokenSet::default();
        assert_eq!(
            strip_emote_tokens("keep  all   spaces", &set),
            "keep  all   spaces"
        );
    }

    #[test]
    fn process_for_language_drops_the_display_name_prefix_that_process_prepends() {
        let config = PipelineConfig {
            output: OutputConfig {
                read_display_name_first: true,
                ..OutputConfig::default()
            },
            ..PipelineConfig::default()
        };
        let context = PipelineContext {
            viewer_name: "koval_dev",
            recent_messages: &[],
        };
        assert_eq!(
            process("hi chat", &config, &context),
            PipelineResult::Speak("koval_dev says: hi chat".into())
        );
        assert_eq!(
            process_for_language("hi chat", &config, &context),
            Some("hi chat".to_owned())
        );
    }

    #[test]
    fn process_for_language_returns_none_when_the_message_is_skipped() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                contains_url: true,
                ..SkipRulesConfig::default()
            },
            ..PipelineConfig::default()
        };
        assert!(process_for_language("see https://example.com", &config, &ctx()).is_none());
    }
}
