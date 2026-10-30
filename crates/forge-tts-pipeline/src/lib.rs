use std::collections::HashSet;
use std::ops::{Range, RangeInclusive};
use std::sync::LazyLock;

use forge_speech_types::LanguageCode;
use forge_types::is_bot_account;

mod language;
mod spoken_words;
mod trace;

pub use language::{DetectionOutcome, LanguageDetector};
pub use spoken_words::{SpokenWords, spoken_words};
pub use trace::{AppliedReplacement, ReplacementOrigin};

use trace::{EditLog, Unlogged};

const CENSOR_TOKEN: &str = "[beep]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineResult {
    Speak(String),
    Skip { reason: SkipReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipRule {
    ContainsUrl,
    StartsWithPrefix,
    FromBotAccount,
    ExceedsMaxLength,
    RepeatsRecentMessage,
    EmoteOnly,
    MostlyNonLatin,
    CustomRegex,
}

impl SkipRule {
    pub const fn token(self) -> &'static str {
        match self {
            Self::ContainsUrl => "contains_url",
            Self::StartsWithPrefix => "starts_with_prefix",
            Self::FromBotAccount => "from_bot_account",
            Self::ExceedsMaxLength => "exceeds_max_length",
            Self::RepeatsRecentMessage => "repeats_recent_message",
            Self::EmoteOnly => "emote_only",
            Self::MostlyNonLatin => "mostly_non_latin",
            Self::CustomRegex => "custom_regex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    MatchedSkipRule(SkipRule),
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
    LinkWord {
        compiled: regex::Regex,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlocklistMode {
    Censor,
    SkipMessage,
}

#[derive(Debug, Clone, Default)]
pub struct SkipRulesConfig {
    pub contains_url: bool,
    pub skip_prefix: Option<String>,
    pub from_bot_accounts: bool,
    pub bot_accounts: Vec<String>,
    pub max_chars: Option<usize>,
    pub repeat_of_recent: bool,
    pub emote_only: bool,
    pub mostly_non_latin: bool,
    pub custom_regexes: Vec<regex::Regex>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmoteTreatment {
    Keep,
    StripTwitch,
    ToWord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputConfig {
    pub read_display_name_first: bool,
    pub emote_treatment: EmoteTreatment,
    pub strip_emoji: bool,
    pub sanitize_punctuation: bool,
    pub max_duration_secs: Option<u32>,
    pub language_aware_voice: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineContext<'a> {
    pub viewer_name: &'a str,
    pub recent_messages: &'a [String],
    pub speech_language: Option<LanguageCode>,
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub emote_tokens: EmoteTokenSet,
    pub skip_rules: SkipRulesConfig,
    pub replacement_rules: Vec<ReplacementRule>,
    pub word_blocklist: Vec<String>,
    pub blocklist_mode: BlocklistMode,
    pub output: OutputConfig,
    pub strip_reward_emotes: bool,
}

impl PipelineConfig {
    pub fn with_message_emotes(&self, message_emotes: &EmoteTokenSet) -> PipelineConfig {
        PipelineConfig {
            emote_tokens: self.emote_tokens.merged_with(message_emotes),
            ..self.clone()
        }
    }
}

const MISC_SYMBOLS_AND_DINGBATS: RangeInclusive<u32> = 0x2600..=0x27BF;
const PICTOGRAPH_PLANES: RangeInclusive<u32> = 0x1F000..=0x1FAFF;
const VARIATION_SELECTORS: RangeInclusive<u32> = 0xFE00..=0xFE0F;
const ZERO_WIDTH_JOINER: u32 = 0x200D;
const COMBINING_KEYCAP: u32 = 0x20E3;
const TAG_CHARACTERS: RangeInclusive<u32> = 0xE0020..=0xE007F;
const WATCH_AND_HOURGLASS: RangeInclusive<u32> = 0x231A..=0x231B;
const MEDIA_CONTROLS: RangeInclusive<u32> = 0x23E9..=0x23FA;
const DOUBLE_EXCLAMATION: u32 = 0x203C;
const EXCLAMATION_QUESTION: u32 = 0x2049;
const DIAGONAL_ARROWS: RangeInclusive<u32> = 0x2194..=0x2199;
const HOOKED_ARROWS: RangeInclusive<u32> = 0x21A9..=0x21AA;
const CURVED_ARROWS: RangeInclusive<u32> = 0x2934..=0x2935;
const ARROW_SHAPES: RangeInclusive<u32> = 0x2B05..=0x2B07;
const LARGE_SQUARES: RangeInclusive<u32> = 0x2B1B..=0x2B1C;
const STAR_AND_CIRCLE: [u32; 2] = [0x2B50, 0x2B55];
const WAVY_DASH_AND_ALTERNATION_MARK: [u32; 2] = [0x3030, 0x303D];
const CIRCLED_IDEOGRAPHS: [u32; 2] = [0x3297, 0x3299];

fn is_emoji_char(c: char) -> bool {
    let cp = c as u32;
    MISC_SYMBOLS_AND_DINGBATS.contains(&cp)
        || PICTOGRAPH_PLANES.contains(&cp)
        || VARIATION_SELECTORS.contains(&cp)
        || TAG_CHARACTERS.contains(&cp)
        || WATCH_AND_HOURGLASS.contains(&cp)
        || MEDIA_CONTROLS.contains(&cp)
        || DIAGONAL_ARROWS.contains(&cp)
        || HOOKED_ARROWS.contains(&cp)
        || CURVED_ARROWS.contains(&cp)
        || ARROW_SHAPES.contains(&cp)
        || LARGE_SQUARES.contains(&cp)
        || [
            ZERO_WIDTH_JOINER,
            COMBINING_KEYCAP,
            DOUBLE_EXCLAMATION,
            EXCLAMATION_QUESTION,
        ]
        .contains(&cp)
        || STAR_AND_CIRCLE.contains(&cp)
        || WAVY_DASH_AND_ALTERNATION_MARK.contains(&cp)
        || CIRCLED_IDEOGRAPHS.contains(&cp)
}

const SOFT_HYPHEN: RangeInclusive<u32> = 0x00AD..=0x00AD;
const COMBINING_GRAPHEME_JOINER: RangeInclusive<u32> = 0x034F..=0x034F;
const ARABIC_LETTER_MARK: RangeInclusive<u32> = 0x061C..=0x061C;
const HANGUL_JAMO_FILLERS: RangeInclusive<u32> = 0x115F..=0x1160;
const KHMER_INHERENT_VOWELS: RangeInclusive<u32> = 0x17B4..=0x17B5;
const MONGOLIAN_FORMAT_CONTROLS: RangeInclusive<u32> = 0x180B..=0x180F;
const ZERO_WIDTH_AND_DIRECTION_MARKS: RangeInclusive<u32> = 0x200B..=0x200F;
const BIDI_EMBEDDINGS: RangeInclusive<u32> = 0x202A..=0x202E;
const WORD_JOINER_AND_INVISIBLE_OPERATORS: RangeInclusive<u32> = 0x2060..=0x206F;
const BRAILLE_BLANK: RangeInclusive<u32> = 0x2800..=0x2800;
const HANGUL_FILLER: RangeInclusive<u32> = 0x3164..=0x3164;
const BYTE_ORDER_MARK: RangeInclusive<u32> = 0xFEFF..=0xFEFF;
const HALFWIDTH_HANGUL_FILLER: RangeInclusive<u32> = 0xFFA0..=0xFFA0;
const SPECIALS_RESERVED_AND_ANNOTATIONS: RangeInclusive<u32> = 0xFFF0..=0xFFFB;
const SHORTHAND_FORMAT_CONTROLS: RangeInclusive<u32> = 0x1BCA0..=0x1BCA3;
const MUSICAL_FORMAT_CONTROLS: RangeInclusive<u32> = 0x1D173..=0x1D17A;
const TAGS_AND_VARIATION_SELECTORS_SUPPLEMENT: RangeInclusive<u32> = 0xE0000..=0xE0FFF;

const INVISIBLE_CHARS: [RangeInclusive<u32>; 18] = [
    SOFT_HYPHEN,
    COMBINING_GRAPHEME_JOINER,
    ARABIC_LETTER_MARK,
    HANGUL_JAMO_FILLERS,
    KHMER_INHERENT_VOWELS,
    MONGOLIAN_FORMAT_CONTROLS,
    ZERO_WIDTH_AND_DIRECTION_MARKS,
    BIDI_EMBEDDINGS,
    WORD_JOINER_AND_INVISIBLE_OPERATORS,
    BRAILLE_BLANK,
    HANGUL_FILLER,
    VARIATION_SELECTORS,
    BYTE_ORDER_MARK,
    HALFWIDTH_HANGUL_FILLER,
    SPECIALS_RESERVED_AND_ANNOTATIONS,
    SHORTHAND_FORMAT_CONTROLS,
    MUSICAL_FORMAT_CONTROLS,
    TAGS_AND_VARIATION_SELECTORS_SUPPLEMENT,
];

fn is_invisible_char(c: char) -> bool {
    let cp = c as u32;
    (c.is_control() && !c.is_whitespace())
        || INVISIBLE_CHARS.iter().any(|range| range.contains(&cp))
}

#[derive(Debug, Clone, Copy)]
struct Unspoken {
    emoji: bool,
}

impl Unspoken {
    const INVISIBLE_ONLY: Unspoken = Unspoken { emoji: false };

    fn in_output(output: &OutputConfig) -> Unspoken {
        Unspoken {
            emoji: output.strip_emoji,
        }
    }

    fn contains(self, c: char) -> bool {
        is_invisible_char(c) || (self.emoji && is_emoji_char(c))
    }
}

fn spoken_text(text: &str, unspoken: Unspoken) -> String {
    let kept: String = text.chars().filter(|&c| !unspoken.contains(c)).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[expect(clippy::expect_used, reason = "literal pattern is a valid regex")]
static URL_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?:https?|ftp)://\S+").expect("static regex"));

#[expect(clippy::expect_used, reason = "literal pattern is a valid regex")]
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

fn is_repeat_of_recent(text: &str, recent: &[String], config: &PipelineConfig) -> bool {
    let forms = spoken_forms(text, config);
    recent.iter().any(|message| {
        let seen = spoken_forms(message, config);
        forms
            .iter()
            .any(|form| !form.is_empty() && seen.contains(form))
    })
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

const LATIN_SUPPLEMENT_THROUGH_EXTENDED_B: RangeInclusive<u32> = 0x00C0..=0x024F;
const LATIN_EXTENDED_ADDITIONAL: RangeInclusive<u32> = 0x1E00..=0x1EFF;

fn is_latin_alpha(c: char) -> bool {
    let cp = c as u32;
    c.is_ascii_alphabetic()
        || LATIN_SUPPLEMENT_THROUGH_EXTENDED_B.contains(&cp)
        || LATIN_EXTENDED_ADDITIONAL.contains(&cp)
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
    let [spoken, transformed] = spoken_forms(text, config);
    let forms = [text, spoken.as_str(), transformed.as_str()];
    if skip_rules.contains_url && forms.iter().any(|form| URL_RE.is_match(form)) {
        return Some(SkipReason::MatchedSkipRule(SkipRule::ContainsUrl));
    }
    if let Some(prefix) = skip_rules.skip_prefix.as_deref()
        && !prefix.is_empty()
        && forms.iter().any(|form| form.starts_with(prefix))
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::StartsWithPrefix));
    }
    if skip_rules.from_bot_accounts && is_bot_account(context.viewer_name, &skip_rules.bot_accounts)
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::FromBotAccount));
    }
    if let Some(max_chars) = skip_rules.max_chars
        && spoken.chars().count() > max_chars
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::ExceedsMaxLength));
    }
    if skip_rules.repeat_of_recent && is_repeat_of_recent(text, context.recent_messages, config) {
        return Some(SkipReason::MatchedSkipRule(SkipRule::RepeatsRecentMessage));
    }
    if skip_rules.emote_only
        && forms
            .iter()
            .any(|form| is_emote_only(form, &config.emote_tokens))
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::EmoteOnly));
    }
    if skip_rules.mostly_non_latin
        && (is_mostly_non_latin(&spoken) || is_mostly_non_latin(&transformed))
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::MostlyNonLatin));
    }
    if skip_rules
        .custom_regexes
        .iter()
        .any(|re| forms.iter().any(|form| re.is_match(form)))
    {
        return Some(SkipReason::MatchedSkipRule(SkipRule::CustomRegex));
    }
    None
}

fn stage_text_replacements(
    text: &str,
    rules: &[ReplacementRule],
    language: Option<LanguageCode>,
) -> String {
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
            ReplacementRule::LinkWord { compiled } => {
                current = compiled
                    .replace_all(&current, spoken_words(language).link)
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

fn spoken_match_len(haystack: &str, pattern: &[char], unspoken: Unspoken) -> Option<usize> {
    let mut remaining = pattern.iter().copied();
    let mut expected = remaining.next();
    let mut previous: Option<char> = None;
    let mut matched_len = 0usize;
    for (at, actual) in haystack.char_indices() {
        let Some(wanted) = expected else {
            break;
        };
        if chars_equal_ignoring_case(actual, wanted)
            || (actual.is_whitespace() && wanted.is_whitespace())
        {
            previous = Some(wanted);
            matched_len = at + actual.len_utf8();
            expected = remaining.next();
            continue;
        }
        let extends_whitespace =
            actual.is_whitespace() && previous.is_some_and(char::is_whitespace);
        if previous.is_none() || !(unspoken.contains(actual) || extends_whitespace) {
            return None;
        }
    }
    expected.is_none().then_some(matched_len)
}

fn blocked_pattern(entry: &str) -> Vec<char> {
    spoken_text(entry, Unspoken::INVISIBLE_ONLY)
        .chars()
        .collect()
}

fn blocked_patterns(blocklist: &[String]) -> Vec<Vec<char>> {
    blocklist
        .iter()
        .map(|entry| blocked_pattern(entry))
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn censor_blocked_words(
    text: &str,
    config: &PipelineConfig,
    own_tokens: &[Range<usize>],
    log: &mut impl EditLog,
) -> Result<String, SkipReason> {
    censor_blocked_text(
        text,
        &blocked_patterns(&config.word_blocklist),
        &config.blocklist_mode,
        Unspoken::in_output(&config.output),
        own_tokens,
        log,
    )
}

fn censor_blocked_text(
    word: &str,
    blocked: &[Vec<char>],
    mode: &BlocklistMode,
    unspoken: Unspoken,
    own_tokens: &[Range<usize>],
    log: &mut impl EditLog,
) -> Result<String, SkipReason> {
    let mut result = String::with_capacity(word.len());
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    while let Some(current) = word[cursor..].chars().next() {
        if let Some(own_token) = own_tokens.iter().find(|token| token.start == cursor) {
            cursor = own_token.end;
            continue;
        }
        let blocked_len =
            longest_blocked_match(&word[..cursor], &word[cursor..], blocked, unspoken)
                .filter(|&match_len| match_len > 0);
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
    Ok(result)
}

fn longest_blocked_match(
    before: &str,
    rest: &str,
    blocked: &[Vec<char>],
    unspoken: Unspoken,
) -> Option<usize> {
    let has_word_edge = |edge: Option<&char>| edge.is_some_and(|&c| is_word_char(c));
    blocked
        .iter()
        .filter(|entry| !has_word_edge(entry.first()) || follows_spoken_boundary(before, unspoken))
        .filter_map(|entry| {
            let match_len = spoken_match_len(rest, entry, unspoken)?;
            (!has_word_edge(entry.last()) || precedes_spoken_boundary(&rest[match_len..], unspoken))
                .then_some(match_len)
        })
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

fn follows_spoken_boundary(before: &str, unspoken: Unspoken) -> bool {
    before
        .chars()
        .rev()
        .find(|&c| !unspoken.contains(c))
        .is_none_or(|c| !is_word_char(c))
}

fn precedes_spoken_boundary(after: &str, unspoken: Unspoken) -> bool {
    after
        .chars()
        .find(|&c| !unspoken.contains(c))
        .is_none_or(|c| !is_word_char(c))
}

fn is_spoken_as_entry(text: &str, entry: &str, unspoken: Unspoken) -> bool {
    spoken_match_len(text, &blocked_pattern(entry), unspoken) == Some(text.len())
}

fn transform_emotes_logged(text: &str, config: &PipelineConfig, log: &mut impl EditLog) -> String {
    match config.output.emote_treatment {
        EmoteTreatment::ToWord => colon_tokens_to_words_logged(text, log),
        EmoteTreatment::StripTwitch => strip_emote_tokens_logged(text, &config.emote_tokens, log),
        EmoteTreatment::Keep => text.to_owned(),
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
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        let (run, after_run) = rest.split_at(rest.len() - rest.trim_start_matches(c).len());
        if c.is_ascii_punctuation() && run.len() > c.len_utf8() {
            let collapsed_to = result.len();
            result.push(c);
            log.record(collapsed_to..result.len(), run, &run[..c.len_utf8()]);
        } else {
            result.push_str(run);
        }
        rest = after_run;
    }
    result
}

fn output_transforms_logged(text: &str, config: &PipelineConfig, log: &mut impl EditLog) -> String {
    let emote_pass = transform_emotes_logged(text, config, log);
    log.end_pass();
    let emoji_pass = if config.output.strip_emoji {
        strip_emoji_logged(&emote_pass, log)
    } else {
        emote_pass
    };
    log.end_pass();
    let sanitized = if config.output.sanitize_punctuation {
        sanitize_punctuation_logged(&emoji_pass, log)
    } else {
        emoji_pass
    };
    log.end_pass();
    sanitized
}

fn spoken_forms(text: &str, config: &PipelineConfig) -> [String; 2] {
    let unspoken = Unspoken::in_output(&config.output);
    let transformed = output_transforms_logged(text, config, &mut Unlogged);
    [
        spoken_text(text, unspoken),
        spoken_text(&transformed, unspoken),
    ]
}

fn has_nothing_to_speak(text_before_output: &str, config: &PipelineConfig) -> bool {
    let [_, transformed] = spoken_forms(text_before_output, config);
    transformed.is_empty()
}

fn spoken_name_prefix(viewer_name: &str, says: &str) -> String {
    format!("{viewer_name} {says}: ")
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
    let emoji_pass = if config.output.strip_emoji {
        strip_emoji_logged(&emote_pass, log)
    } else {
        emote_pass
    };
    log.end_pass();
    let named = if prepend_display_name {
        let mut named = spoken_name_prefix(
            context.viewer_name,
            spoken_words(context.speech_language).says,
        );
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
        StageName::WordBlocklist => trace::blocklist_traced(text, config).0,
        StageName::TextReplacements => StageOut::Ok(stage_text_replacements(
            text,
            &config.replacement_rules,
            context.speech_language,
        )),
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
        if stage == StageName::Output && has_nothing_to_speak(&current, config) {
            return PipelineResult::Skip {
                reason: SkipReason::EmptyAfterProcessing,
            };
        }
        match run_stage(stage, &current, config, context, prepend_display_name) {
            StageOut::Ok(s) => current = s,
            StageOut::Skip(r) => return PipelineResult::Skip { reason: r },
        }
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
    } else if outcomes
        .last()
        .is_some_and(|output_stage| has_nothing_to_speak(&output_stage.input, config))
    {
        PipelineResult::Skip {
            reason: SkipReason::EmptyAfterProcessing,
        }
    } else {
        PipelineResult::Speak(current)
    };

    (final_result, outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral_output() -> OutputConfig {
        OutputConfig {
            read_display_name_first: false,
            emote_treatment: EmoteTreatment::Keep,
            strip_emoji: false,
            sanitize_punctuation: false,
            max_duration_secs: None,
            language_aware_voice: false,
        }
    }

    fn neutral_config() -> PipelineConfig {
        PipelineConfig {
            emote_tokens: EmoteTokenSet::default(),
            skip_rules: SkipRulesConfig::default(),
            replacement_rules: Vec::new(),
            word_blocklist: Vec::new(),
            blocklist_mode: BlocklistMode::Censor,
            output: neutral_output(),
            strip_reward_emotes: false,
        }
    }

    fn ctx() -> PipelineContext<'static> {
        PipelineContext {
            viewer_name: "viewer",
            recent_messages: &[],
            speech_language: None,
        }
    }

    #[test]
    fn process_passthrough_no_config() {
        let config = neutral_config();
        let result = process("hello world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello world".into()));
    }

    #[test]
    fn each_skip_rule_reports_its_own_rule_when_it_is_the_one_that_matched() {
        let recent = vec!["same again".to_owned()];
        let mut emote_config = neutral_config();
        emote_config.emote_tokens.tokens.insert("LUL".into());
        let cases: Vec<(&str, SkipRulesConfig, &str, &str, SkipRule)> = vec![
            (
                "url",
                SkipRulesConfig {
                    contains_url: true,
                    ..SkipRulesConfig::default()
                },
                "see https://a.example",
                "viewer",
                SkipRule::ContainsUrl,
            ),
            (
                "prefix",
                SkipRulesConfig {
                    skip_prefix: Some("!".into()),
                    ..SkipRulesConfig::default()
                },
                "!command",
                "viewer",
                SkipRule::StartsWithPrefix,
            ),
            (
                "bot",
                SkipRulesConfig {
                    from_bot_accounts: true,
                    bot_accounts: vec!["relaybot".into()],
                    ..SkipRulesConfig::default()
                },
                "hello",
                "relaybot",
                SkipRule::FromBotAccount,
            ),
            (
                "length",
                SkipRulesConfig {
                    max_chars: Some(3),
                    ..SkipRulesConfig::default()
                },
                "hello",
                "viewer",
                SkipRule::ExceedsMaxLength,
            ),
            (
                "repeat",
                SkipRulesConfig {
                    repeat_of_recent: true,
                    ..SkipRulesConfig::default()
                },
                "same again",
                "viewer",
                SkipRule::RepeatsRecentMessage,
            ),
            (
                "emote",
                SkipRulesConfig {
                    emote_only: true,
                    ..SkipRulesConfig::default()
                },
                "LUL LUL",
                "viewer",
                SkipRule::EmoteOnly,
            ),
            (
                "script",
                SkipRulesConfig {
                    mostly_non_latin: true,
                    ..SkipRulesConfig::default()
                },
                "привіт світ",
                "viewer",
                SkipRule::MostlyNonLatin,
            ),
            (
                "regex",
                SkipRulesConfig {
                    custom_regexes: vec![regex::Regex::new("spam+").unwrap()],
                    ..SkipRulesConfig::default()
                },
                "so spamm",
                "viewer",
                SkipRule::CustomRegex,
            ),
        ];
        for (label, skip_rules, text, viewer, rule) in cases {
            let config = PipelineConfig {
                skip_rules,
                ..emote_config.clone()
            };
            let context = PipelineContext {
                viewer_name: viewer,
                recent_messages: &recent,
                speech_language: None,
            };
            assert_eq!(
                process(text, &config, &context),
                PipelineResult::Skip {
                    reason: SkipReason::MatchedSkipRule(rule)
                },
                "{label}"
            );
        }
    }

    #[test]
    fn skip_rule_tokens_are_distinct_snake_case_identifiers() {
        let tokens: Vec<&str> = [
            SkipRule::ContainsUrl,
            SkipRule::StartsWithPrefix,
            SkipRule::FromBotAccount,
            SkipRule::ExceedsMaxLength,
            SkipRule::RepeatsRecentMessage,
            SkipRule::EmoteOnly,
            SkipRule::MostlyNonLatin,
            SkipRule::CustomRegex,
        ]
        .into_iter()
        .map(SkipRule::token)
        .collect();
        let distinct: std::collections::BTreeSet<&str> = tokens.iter().copied().collect();
        assert_eq!(distinct.len(), tokens.len());
        assert!(
            tokens
                .iter()
                .all(|t| t.chars().all(|c| c.is_ascii_lowercase() || c == '_')),
            "{tokens:?}"
        );
    }

    #[test]
    fn skip_rules_contains_url() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                contains_url: true,
                ..SkipRulesConfig::default()
            },
            ..neutral_config()
        };
        let result = process("visit https://spam.com", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule(SkipRule::ContainsUrl)
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
            ..neutral_config()
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
            ..neutral_config()
        };
        let builtin_ctx = PipelineContext {
            viewer_name: "NightBot",
            recent_messages: &[],
            speech_language: None,
        };
        assert!(matches!(
            process("hi chat", &config, &builtin_ctx),
            PipelineResult::Skip { .. }
        ));
        let custom_ctx = PipelineContext {
            viewer_name: "CustomBot",
            recent_messages: &[],
            speech_language: None,
        };
        assert!(matches!(
            process("hi chat", &config, &custom_ctx),
            PipelineResult::Skip { .. }
        ));
        let human_ctx = PipelineContext {
            viewer_name: "a_real_viewer",
            recent_messages: &[],
            speech_language: None,
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
                max_chars: Some(5),
                ..SkipRulesConfig::default()
            },
            ..neutral_config()
        };
        let result = process("hello world", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Skip {
                reason: SkipReason::MatchedSkipRule(SkipRule::ExceedsMaxLength)
            }
        );
        let fits = process("hello", &config, &ctx());
        assert_eq!(fits, PipelineResult::Speak("hello".into()));
    }

    #[test]
    fn max_chars_counts_characters_not_bytes_at_the_boundary() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                max_chars: Some(5),
                ..SkipRulesConfig::default()
            },
            ..neutral_config()
        };
        for (text, skipped) in [("приві", false), ("привіт", true), ("", false)] {
            assert_eq!(
                matches!(process(text, &config, &ctx()), PipelineResult::Skip { reason } if reason == SkipReason::MatchedSkipRule(SkipRule::ExceedsMaxLength)),
                skipped,
                "{text:?}"
            );
        }
    }

    #[test]
    fn skip_rules_repeat_of_recent_is_trimmed_case_sensitive() {
        let config = PipelineConfig {
            skip_rules: SkipRulesConfig {
                repeat_of_recent: true,
                ..SkipRulesConfig::default()
            },
            ..neutral_config()
        };
        let recent = vec!["hello chat".to_owned()];
        let repeat_ctx = PipelineContext {
            viewer_name: "viewer",
            recent_messages: &recent,
            speech_language: None,
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
        let mut config = neutral_config();
        config.output.emote_treatment = EmoteTreatment::StripTwitch;
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
            ..neutral_config()
        };

        let merged = base.with_message_emotes(&set(&["LUL", "Kappa"]));

        assert_eq!(merged.emote_tokens.tokens, set(&["Kappa", "LUL"]).tokens);
        assert!(merged.strip_reward_emotes);
        assert_eq!(base.emote_tokens.tokens, set(&["Kappa"]).tokens);
    }

    #[test]
    fn emote_stripper_strips_emoji() {
        let mut config = neutral_config();
        config.output.strip_emoji = true;
        let result = process("hello 🎉 world", &config, &ctx());
        assert_eq!(result, PipelineResult::Speak("hello  world".into()));
    }

    #[test]
    fn strip_emoji_removes_symbols_outside_the_main_pictograph_blocks() {
        let mut config = neutral_config();
        config.output.strip_emoji = true;
        let emoji = [
            '\u{231A}',
            '\u{23E9}',
            '\u{23FA}',
            '\u{203C}',
            '\u{2049}',
            '\u{2194}',
            '\u{2199}',
            '\u{21A9}',
            '\u{21AA}',
            '\u{2934}',
            '\u{2935}',
            '\u{2B05}',
            '\u{2B07}',
            '\u{2B1B}',
            '\u{2B1C}',
            '\u{2B50}',
            '\u{2B55}',
            '\u{3030}',
            '\u{303D}',
            '\u{3297}',
            '\u{3299}',
            '\u{E0067}',
        ];
        for c in emoji {
            let input = format!("a{c}b");
            assert_eq!(
                process(&input, &config, &ctx()),
                PipelineResult::Speak("ab".into()),
                "U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn strip_emoji_keeps_the_neighbours_of_each_emoji_range_boundary() {
        let mut config = neutral_config();
        config.output.strip_emoji = true;
        for c in [
            '\u{2193}', '\u{219A}', '\u{23E8}', '\u{23FB}', '\u{2933}', '\u{2936}', '\u{2B04}',
            '\u{2B08}', '\u{2B1A}', '\u{2B1D}', '\u{2B4F}', '\u{2B51}', '\u{2192}', '\u{00A9}',
            '!', '?',
        ] {
            let input = format!("a{c}b");
            assert_eq!(
                process(&input, &config, &ctx()),
                PipelineResult::Speak(input.clone()),
                "U+{:04X}",
                c as u32
            );
        }
    }

    #[test]
    fn strip_emoji_removes_a_zwj_family_sequence_and_a_keycap_entirely() {
        let mut config = neutral_config();
        config.output.strip_emoji = true;
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let keycap = "1\u{FE0F}\u{20E3}";
        let input = format!("x {family} y {keycap} z");
        assert_eq!(
            process(&input, &config, &ctx()),
            PipelineResult::Speak("x  y 1 z".into())
        );
    }

    #[test]
    fn output_emote_to_word_converts_colon_tokens_and_keeps_known_tokens() {
        let mut config = PipelineConfig {
            output: OutputConfig {
                emote_treatment: EmoteTreatment::ToWord,
                ..neutral_output()
            },
            ..neutral_config()
        };
        config.emote_tokens.tokens.insert("LUL".into());
        let result = process("hello :pog: LUL world", &config, &ctx());
        assert_eq!(
            result,
            PipelineResult::Speak("hello pog LUL world".into()),
            "colon tokens become bare words; known-list tokens are left as spoken words"
        );
    }

    #[test]
    fn output_read_display_name_first_prefixes_viewer_name_with_the_voice_language_says_word() {
        let config = PipelineConfig {
            output: OutputConfig {
                read_display_name_first: true,
                ..neutral_output()
            },
            ..neutral_config()
        };
        let context = PipelineContext {
            viewer_name: "koval_dev",
            recent_messages: &[],
            speech_language: LanguageCode::from_locale("uk-UA"),
        };
        let result = process("hi chat", &config, &context);
        assert_eq!(
            result,
            PipelineResult::Speak("koval_dev каже: hi chat".into())
        );
    }

    #[test]
    fn link_word_rule_speaks_the_voice_language_link_word_for_each_language() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::LinkWord {
                compiled: regex::Regex::new(r"https?://\S+").unwrap(),
            }],
            ..neutral_config()
        };
        let spoken = |language: Option<LanguageCode>| {
            let context = PipelineContext {
                speech_language: language,
                ..ctx()
            };
            process("see https://example.com now", &config, &context)
        };
        assert_eq!(
            (
                spoken(LanguageCode::from_locale("uk")),
                spoken(LanguageCode::from_locale("en-US")),
                spoken(None),
            ),
            (
                PipelineResult::Speak("see посилання now".into()),
                PipelineResult::Speak("see link now".into()),
                PipelineResult::Speak("see link now".into()),
            )
        );
    }

    #[test]
    fn traced_link_word_rule_records_the_voice_language_link_word() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::LinkWord {
                compiled: regex::Regex::new(r"https?://\S+").unwrap(),
            }],
            ..neutral_config()
        };
        let context = PipelineContext {
            speech_language: LanguageCode::from_locale("uk"),
            ..ctx()
        };
        let (result, _) = preview("see https://example.com now", &config, &context);
        assert_eq!(result, PipelineResult::Speak("see посилання now".into()));
    }

    #[test]
    fn text_replacement_case_insensitive() {
        let config = PipelineConfig {
            replacement_rules: vec![ReplacementRule::Text {
                pattern: "lol".into(),
                replacement: "(laugh)".into(),
            }],
            ..neutral_config()
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
                ..neutral_config()
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
            ..neutral_config()
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
            ..neutral_config()
        }
    }

    struct RefusesEdits;

    impl EditLog for RefusesEdits {
        fn record(&mut self, _output: Range<usize>, _original: &str, _replacement: &str) {
            unreachable!("an empty pattern must not censor");
        }
    }

    #[test]
    fn an_empty_pattern_reaching_the_censor_loop_matches_nothing_in_either_mode() {
        for mode in [BlocklistMode::Censor, BlocklistMode::SkipMessage] {
            for text in ["hello", "\u{200B}", ""] {
                assert_eq!(
                    censor_blocked_text(
                        text,
                        &[Vec::new()],
                        &mode,
                        Unspoken::INVISIBLE_ONLY,
                        &[],
                        &mut RefusesEdits,
                    ),
                    Ok(text.to_owned())
                );
            }
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
    fn censor_replaces_a_multi_word_entry_as_one_phrase_and_keeps_the_spacing_around_it() {
        let config = blocklist(&["bad word", "Go Away"], BlocklistMode::Censor);
        for (input, expected) in [
            ("a bad word here", "a [beep] here"),
            ("BAD WORD!", "[beep]!"),
            ("say go away now", "say [beep] now"),
            ("bad word bad word", "[beep] [beep]"),
            ("bad  word", "[beep]"),
            ("bad words", "bad words"),
            ("a bad", "a bad"),
            ("word bad", "word bad"),
            ("abad word", "abad word"),
        ] {
            assert_eq!(
                process(input, &config, &ctx()),
                PipelineResult::Speak(expected.into()),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn skip_message_mode_skips_on_a_multi_word_phrase_but_not_on_its_parts() {
        let config = blocklist(&["bad word"], BlocklistMode::SkipMessage);
        for (input, skipped) in [
            ("this is a bad word indeed", true),
            ("Bad Word", true),
            ("bad  word", true),
            ("bad words", false),
            ("bad", false),
            ("word", false),
            ("bad then word", false),
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
    fn the_blocklist_stage_preserves_whitespace_it_does_not_censor() {
        for mode in [BlocklistMode::Censor, BlocklistMode::SkipMessage] {
            let config = blocklist(&["gtfo"], mode);
            for input in [
                "a   b",
                "line one\nline two",
                "tab\tseparated",
                "  padded  ",
                "trail ",
            ] {
                assert_eq!(
                    process(input, &config, &ctx()),
                    PipelineResult::Speak(input.into()),
                    "input {input:?}"
                );
            }
        }
    }

    #[test]
    fn censoring_keeps_the_whitespace_around_the_blocked_word() {
        let config = blocklist(&["gtfo"], BlocklistMode::Censor);
        assert_eq!(
            process("  a   gtfo\n\tb  ", &config, &ctx()),
            PipelineResult::Speak("  a   [beep]\n\tb  ".into())
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
            ..neutral_config()
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
            ..neutral_config()
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
                ..neutral_output()
            },
            ..neutral_config()
        };
        let context = PipelineContext {
            viewer_name: "koval_dev",
            recent_messages: &[],
            speech_language: None,
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
            ..neutral_config()
        };
        assert!(process_for_language("see https://example.com", &config, &ctx()).is_none());
    }
}
