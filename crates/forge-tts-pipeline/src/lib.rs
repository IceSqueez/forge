use std::collections::HashSet;
use std::ops::Range;
use std::sync::LazyLock;

use forge_types::is_bot_account;
use serde::{Deserialize, Serialize};

mod language;
mod trace;

pub use language::{DetectionOutcome, LanguageCode, LanguageDetector};
pub use trace::{AppliedReplacement, ReplacementOrigin};

use trace::{EditLog, Unlogged};

const CENSOR_TOKEN: &str = "[beep]";

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
    pub replacements: Vec<AppliedReplacement>,
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

impl EmoteTokenSet {
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn merged_with(&self, other: &EmoteTokenSet) -> EmoteTokenSet {
        EmoteTokenSet {
            tokens: self.tokens.union(&other.tokens).cloned().collect(),
        }
    }
}

impl FromIterator<String> for EmoteTokenSet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            tokens: iter.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ReplacementRule {
    Text {
        pattern: String,
        replacement: String,
    },
    WholeWord {
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

    pub fn with_message_emotes(&self, message_emotes: &EmoteTokenSet) -> PipelineConfig {
        PipelineConfig {
            emote_tokens: self.emote_tokens.merged_with(message_emotes),
            ..self.clone()
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

pub fn strip_emote_tokens(text: &str, tokens: &EmoteTokenSet) -> String {
    strip_emote_tokens_logged(text, tokens, &mut Unlogged)
}

fn strip_emote_tokens_logged(text: &str, tokens: &EmoteTokenSet, log: &mut impl EditLog) -> String {
    if tokens.tokens.is_empty() {
        return text.to_owned();
    }
    let mut result = String::with_capacity(text.len());
    let mut gap_start = 0usize;
    for word in word_ranges(text)
        .into_iter()
        .filter(|word| !tokens.tokens.contains(&text[word.clone()]))
    {
        let separator = if result.is_empty() { "" } else { " " };
        replace_gap(&text[gap_start..word.start], separator, &mut result, log);
        result.push_str(&text[word.clone()]);
        gap_start = word.end;
    }
    replace_gap(&text[gap_start..], "", &mut result, log);
    result
}

fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut word_start: Option<usize> = None;
    for (at, c) in text.char_indices() {
        match (c.is_whitespace(), word_start) {
            (true, Some(start)) => {
                ranges.push(start..at);
                word_start = None;
            }
            (false, None) => word_start = Some(at),
            _ => {}
        }
    }
    if let Some(start) = word_start {
        ranges.push(start..text.len());
    }
    ranges
}

fn replace_gap(gap: &str, separator: &str, result: &mut String, log: &mut impl EditLog) {
    let replaced_from = result.len();
    result.push_str(separator);
    if gap != separator {
        log.record(replaced_from..result.len(), gap, separator);
    }
}

fn record_removal(removed: &str, result: &str, log: &mut impl EditLog) {
    let at = result.len();
    log.record(at..at, removed, "");
}

#[allow(clippy::expect_used)]
static URL_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?:https?|ftp)://\S+").expect("static regex"));

#[allow(clippy::expect_used)]
static COLON_EMOTE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r":([A-Za-z0-9_]+):").expect("static regex"));

fn colon_tokens_to_words_logged(text: &str, log: &mut impl EditLog) -> String {
    let mut result = String::with_capacity(text.len());
    let mut copied_until = 0usize;
    for captures in COLON_EMOTE_RE.captures_iter(text) {
        let (Some(whole), Some(word)) = (captures.get(0), captures.get(1)) else {
            continue;
        };
        result.push_str(&text[copied_until..whole.start()]);
        record_removal(&text[whole.start()..word.start()], &result, log);
        result.push_str(word.as_str());
        record_removal(&text[word.end()..whole.end()], &result, log);
        copied_until = whole.end();
    }
    result.push_str(&text[copied_until..]);
    result
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
            ReplacementRule::WholeWord {
                pattern,
                replacement,
            } => {
                current = whole_word_replace(&current, pattern, replacement);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralScope {
    AnywhereInText,
    WholeWords,
}

fn case_insensitive_replace(text: &str, pattern: &str, replacement: &str) -> String {
    literal_replace_logged(
        text,
        pattern,
        replacement,
        LiteralScope::AnywhereInText,
        &mut Unlogged,
    )
}

fn whole_word_replace(text: &str, pattern: &str, replacement: &str) -> String {
    literal_replace_logged(
        text,
        pattern,
        replacement,
        LiteralScope::WholeWords,
        &mut Unlogged,
    )
}

fn literal_replace_logged(
    text: &str,
    pattern: &str,
    replacement: &str,
    scope: LiteralScope,
    log: &mut impl EditLog,
) -> String {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let (Some(&first), Some(&last)) = (pattern_chars.first(), pattern_chars.last()) else {
        return text.to_owned();
    };
    let whole_words = scope == LiteralScope::WholeWords;
    let bounded_start = whole_words && is_word_char(first);
    let bounded_end = whole_words && is_word_char(last);
    let mut result = String::with_capacity(text.len());
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    while let Some(current) = text[cursor..].chars().next() {
        let starts_here = !bounded_start || follows_word_boundary(&text[..cursor]);
        let matched = starts_here
            .then(|| match_len_ignoring_case(&text[cursor..], &pattern_chars))
            .flatten()
            .filter(|&match_len| {
                !bounded_end || precedes_word_boundary(&text[cursor + match_len..])
            });
        match matched {
            Some(match_len) => {
                result.push_str(&text[copied_until..cursor]);
                let replaced_from = result.len();
                result.push_str(replacement);
                log.record(
                    replaced_from..result.len(),
                    &text[cursor..cursor + match_len],
                    replacement,
                );
                cursor += match_len;
                copied_until = cursor;
            }
            None => cursor += current.len_utf8(),
        }
    }
    result.push_str(&text[copied_until..]);
    result
}

fn match_len_ignoring_case(haystack: &str, pattern: &[char]) -> Option<usize> {
    let mut chars = haystack.char_indices();
    for &expected in pattern {
        let (_, actual) = chars.next()?;
        if !chars_equal_ignoring_case(actual, expected) {
            return None;
        }
    }
    Some(chars.offset())
}

fn chars_equal_ignoring_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn stage_word_blocklist(
    text: &str,
    blocklist: &[String],
    mode: &BlocklistMode,
) -> Result<String, SkipReason> {
    stage_word_blocklist_logged(text, blocklist, mode, &mut Unlogged)
}

fn stage_word_blocklist_logged(
    text: &str,
    blocklist: &[String],
    mode: &BlocklistMode,
    log: &mut impl EditLog,
) -> Result<String, SkipReason> {
    if blocklist.is_empty() {
        return Ok(text.to_owned());
    }
    let blocked: Vec<Vec<char>> = blocklist
        .iter()
        .map(|entry| entry.chars().collect::<Vec<char>>())
        .filter(|entry| !entry.is_empty())
        .collect();
    let mut result = String::with_capacity(text.len());
    for (position, word) in text.split_whitespace().enumerate() {
        if position > 0 {
            result.push(' ');
        }
        censor_blocked_in_word(word, &blocked, mode, &mut result, log)?;
    }
    Ok(result)
}

fn censor_blocked_in_word(
    word: &str,
    blocked: &[Vec<char>],
    mode: &BlocklistMode,
    result: &mut String,
    log: &mut impl EditLog,
) -> Result<(), SkipReason> {
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    while let Some(current) = word[cursor..].chars().next() {
        let blocked_len = if follows_word_boundary(&word[..cursor]) {
            longest_blocked_match(&word[cursor..], blocked)
        } else {
            None
        };
        match blocked_len {
            Some(match_len) => {
                if let BlocklistMode::SkipMessage = mode {
                    return Err(SkipReason::BlockedByWordFilter);
                }
                result.push_str(&word[copied_until..cursor]);
                let censored_from = result.len();
                result.push_str(CENSOR_TOKEN);
                log.record(
                    censored_from..result.len(),
                    &word[cursor..cursor + match_len],
                    CENSOR_TOKEN,
                );
                cursor += match_len;
                copied_until = cursor;
            }
            None => cursor += current.len_utf8(),
        }
    }
    result.push_str(&word[copied_until..]);
    Ok(())
}

fn longest_blocked_match(rest: &str, blocked: &[Vec<char>]) -> Option<usize> {
    blocked
        .iter()
        .filter_map(|entry| match_len_ignoring_case(rest, entry))
        .filter(|&match_len| precedes_word_boundary(&rest[match_len..]))
        .max()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn follows_word_boundary(before: &str) -> bool {
    before.chars().next_back().is_none_or(|c| !is_word_char(c))
}

fn precedes_word_boundary(after: &str) -> bool {
    after.chars().next().is_none_or(|c| !is_word_char(c))
}

fn equals_ignoring_case(text: &str, pattern: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    match_len_ignoring_case(text, &pattern_chars) == Some(text.len())
}

fn transform_emotes_logged(text: &str, config: &PipelineConfig, log: &mut impl EditLog) -> String {
    if config.output.emote_to_word {
        colon_tokens_to_words_logged(text, log)
    } else if config.emote_sources.twitch {
        strip_emote_tokens_logged(text, &config.emote_tokens, log)
    } else {
        text.to_owned()
    }
}

fn strip_emoji_logged(text: &str, log: &mut impl EditLog) -> String {
    let mut result = String::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        if is_emoji_char(c) {
            record_removal(&text[at..at + c.len_utf8()], &result, log);
        } else {
            result.push(c);
        }
    }
    result
}

fn sanitize_punctuation_logged(text: &str, log: &mut impl EditLog) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last: Option<char> = None;
    for (at, c) in text.char_indices() {
        if c.is_ascii_punctuation() && last == Some(c) {
            record_removal(&text[at..at + c.len_utf8()], &result, log);
            continue;
        }
        result.push(c);
        last = Some(c);
    }
    result
}

fn spoken_name_prefix(viewer_name: &str) -> String {
    format!("{viewer_name} says: ")
}

fn stage_output(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
) -> String {
    stage_output_logged(text, config, context, prepend_display_name, &mut Unlogged)
}

fn stage_output_logged(
    text: &str,
    config: &PipelineConfig,
    context: &PipelineContext,
    prepend_display_name: bool,
    log: &mut impl EditLog,
) -> String {
    let emote_pass = transform_emotes_logged(text, config, log);
    log.end_pass();
    let emoji_pass = if config.emote_sources.emoji {
        strip_emoji_logged(&emote_pass, log)
    } else {
        emote_pass
    };
    log.end_pass();
    let named = if prepend_display_name {
        let mut named = spoken_name_prefix(context.viewer_name);
        log.record(0..named.len(), "", &named);
        named.push_str(&emoji_pass);
        named
    } else {
        emoji_pass
    };
    log.end_pass();
    let sanitized = if config.output.sanitize_punctuation {
        sanitize_punctuation_logged(&named, log)
    } else {
        named
    };
    log.end_pass();
    sanitized
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

pub fn process(text: &str, config: &PipelineConfig, context: &PipelineContext) -> PipelineResult {
    run_stages(text, config, context, config.output.read_display_name_first)
}

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
    let mut marks: Vec<AppliedReplacement> = Vec::new();
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
            let (stage_out, stage_marks) = trace::run_stage_traced(
                name,
                &input,
                config,
                context,
                config.output.read_display_name_first,
                &marks,
            );
            match stage_out {
                StageOut::Ok(out) => {
                    marks = stage_marks;
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
            replacements: marks.clone(),
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
    fn message_emotes_join_the_configured_set_on_a_copy_of_the_config() {
        let set =
            |codes: &[&str]| -> EmoteTokenSet { codes.iter().map(|c| (*c).to_owned()).collect() };
        let base = PipelineConfig {
            emote_tokens: set(&["Kappa"]),
            strip_reward_emotes: true,
            ..PipelineConfig::default()
        };

        let merged = base.with_message_emotes(&set(&["LUL", "Kappa"]));

        assert_eq!(merged.emote_tokens.tokens, set(&["Kappa", "LUL"]).tokens);
        assert!(merged.strip_reward_emotes);
        assert_eq!(base.emote_tokens.tokens, set(&["Kappa"]).tokens);
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
    fn plain_replacement_keeps_char_boundaries_when_lowercase_changes_utf8_length() {
        for (case, text, pattern, expected) in [
            (
                "kelvin sign before",
                "\u{212A} hello",
                "hello",
                "\u{212A} X",
            ),
            (
                "dotted capital i before",
                "\u{130}stanbul hello",
                "hello",
                "\u{130}stanbul X",
            ),
            (
                "a with stroke before",
                "\u{23A} hello",
                "hello",
                "\u{23A} X",
            ),
            ("kelvin sign after", "hello \u{212A}", "hello", "X \u{212A}"),
            (
                "dotted capital i after",
                "hello \u{130}",
                "hello",
                "X \u{130}",
            ),
            ("a with stroke after", "hello \u{23A}", "hello", "X \u{23A}"),
            (
                "kelvin sign folds to k inside",
                "a \u{212A}elvin b",
                "kelvin",
                "a X b",
            ),
            (
                "a with stroke folds inside",
                "a \u{23A}b c",
                "\u{2C65}b",
                "a X c",
            ),
            (
                "dotted capital i matches itself",
                "go \u{130}stanbul",
                "\u{130}stanbul",
                "go X",
            ),
            (
                "dotted capital i does not fold to a single i",
                "go \u{130}stanbul",
                "istanbul",
                "go \u{130}stanbul",
            ),
            ("every ascii casing", "LOL lol LoL", "lol", "X X X"),
            ("matches do not overlap", "aaa", "aa", "Xa"),
            ("pattern longer than the text", "hel", "hello", "hel"),
            ("pattern at the very end", "say hello", "HELLO", "say X"),
            ("empty text", "", "hello", ""),
            ("empty pattern", "hello", "", "hello"),
        ] {
            assert_eq!(
                case_insensitive_replace(text, pattern, "X"),
                expected,
                "{case}"
            );
        }
    }

    #[test]
    fn whole_word_replacement_needs_a_boundary_only_on_alphanumeric_pattern_edges() {
        for (case, text, pattern, expected) in [
            ("inside a longer word", "POGGERS GG", "GG", "POGGERS X"),
            ("followed by punctuation", "GG!", "GG", "X!"),
            ("wrapped in brackets", "(GG)", "GG", "(X)"),
            ("every casing", "gg Gg gG GG", "GG", "X X X X"),
            ("repeated without a gap", "GGGG", "GG", "GGGG"),
            ("adjacent words", "GG GG", "GG", "X X"),
            ("digits are word characters", "GG2 2GG", "GG", "GG2 2GG"),
            ("cyrillic letter after a latin pattern", "GGі", "GG", "GGі"),
            ("cyrillic pattern", "ДЯКУЮ, дякуюю", "дякую", "X, дякуюю"),
            (
                "pattern spanning two words",
                "good gamer, good game!",
                "good game",
                "good gamer, X!",
            ),
            ("symbol pattern inside a word", "@user", "@", "Xuser"),
            (
                "symbol pattern between letters",
                "rock&roll",
                "&",
                "rockXroll",
            ),
            ("symbol pattern before a digit", "#1", "#", "X1"),
            (
                "symbol start, letter end",
                "hi@user @username",
                "@user",
                "hiX @username",
            ),
            ("letter start, symbol end", "abc++ c++x", "c++", "abc++ Xx"),
            ("empty text", "", "GG", ""),
            ("empty pattern", "GG", "", "GG"),
        ] {
            assert_eq!(whole_word_replace(text, pattern, "X"), expected, "{case}");
        }
    }

    #[test]
    fn literal_rule_scope_decides_whether_a_hit_inside_a_word_is_replaced() {
        for (case, rule, expected) in [
            (
                "inside words",
                ReplacementRule::Text {
                    pattern: "gg".into(),
                    replacement: "X".into(),
                },
                "POXERS X!",
            ),
            (
                "whole words",
                ReplacementRule::WholeWord {
                    pattern: "gg".into(),
                    replacement: "X".into(),
                },
                "POGGERS X!",
            ),
        ] {
            let config = PipelineConfig {
                replacement_rules: vec![rule],
                ..PipelineConfig::default()
            };
            assert_eq!(
                process("POGGERS GG!", &config, &ctx()),
                PipelineResult::Speak(expected.into()),
                "{case}"
            );
        }
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

    fn blocklist(words: &[&str], mode: BlocklistMode) -> PipelineConfig {
        PipelineConfig {
            word_blocklist: words.iter().map(|w| (*w).to_owned()).collect(),
            blocklist_mode: mode,
            ..PipelineConfig::default()
        }
    }

    #[test]
    fn censor_replaces_whole_blocked_words_keeping_surrounding_punctuation() {
        let config = blocklist(&["gtfo", "дурень", "ass", "a$$!"], BlocklistMode::Censor);
        for (input, expected) in [
            ("this is gtfo here", "this is [beep] here"),
            ("GG GTFO!!!", "GG [beep]!!!"),
            ("\"gtfo\"", "\"[beep]\""),
            ("gtfo, then", "[beep], then"),
            ("(GtFo)", "([beep])"),
            ("ДУРЕНЬ!", "[beep]!"),
            ("gtfo,gtfo", "[beep],[beep]"),
            ("gtfo's", "[beep]'s"),
            ("a$$!", "[beep]"),
            ("(A$$!)", "([beep])"),
            ("classic", "classic"),
            ("assist", "assist"),
            ("badass", "badass"),
            ("gtfogtfo", "gtfogtfo"),
            ("дурненький", "дурненький"),
            ("дуренька", "дуренька"),
        ] {
            assert_eq!(
                process(input, &config, &ctx()),
                PipelineResult::Speak(expected.into()),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn censor_prefers_the_longest_entry_that_ends_on_a_word_boundary() {
        let config = blocklist(&["go", "go-away"], BlocklistMode::Censor);
        assert_eq!(
            process("go-away now", &config, &ctx()),
            PipelineResult::Speak("[beep] now".into())
        );
    }

    #[test]
    fn skip_message_mode_skips_only_when_a_whole_word_is_blocked() {
        let config = blocklist(&["gtfo", "ass"], BlocklistMode::SkipMessage);
        for (input, skipped) in [
            ("gtfo!", true),
            ("well \"GTFO\"", true),
            ("classic assist", false),
        ] {
            let result = process(input, &config, &ctx());
            assert_eq!(
                matches!(
                    result,
                    PipelineResult::Skip {
                        reason: SkipReason::BlockedByWordFilter
                    }
                ),
                skipped,
                "input {input:?} gave {result:?}"
            );
        }
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
