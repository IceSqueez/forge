use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One line of forge's file log, split into what a scenario may match on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub file: PathBuf,
    pub level: String,
    pub target: String,
    /// Values as rendered, with the quoting of string values removed.
    pub fields: BTreeMap<String, String>,
    pub line: String,
}

impl LogRecord {
    /// `None` for lines outside the file layer's `<time> <LEVEL> [spans: ]<target>: <message> <fields>` shape.
    pub fn parse(file: PathBuf, line: &str) -> Option<Self> {
        let mut words = line.split(' ').filter(|word| !word.is_empty());
        let _timestamp = words.next()?;
        let level = words.next()?;
        if !is_level(level) {
            return None;
        }
        let after_level = line.split_once(level)?.1;
        let (target, message_and_fields) = split_target(after_level)?;
        Some(Self {
            file,
            level: level.to_owned(),
            target: target.to_owned(),
            fields: trailing_fields(message_and_fields),
            line: line.to_owned(),
        })
    }

    pub fn has_fields(&self, expected: &BTreeMap<String, String>) -> bool {
        expected
            .iter()
            .all(|(key, value)| self.fields.get(key) == Some(value))
    }
}

fn is_level(word: &str) -> bool {
    matches!(word, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR")
}

fn is_target(word: &str) -> bool {
    !word.is_empty()
        && word.split("::").all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

/// The first word ending in `:` with a module-path shape; span prefixes carry braces and never qualify.
fn split_target(rest: &str) -> Option<(&str, &str)> {
    let mut offset = 0;
    for word in rest.split(' ') {
        let end = offset + word.len();
        if let Some(target) = word.strip_suffix(':')
            && is_target(target)
        {
            let remainder = rest.get(end..).unwrap_or_default();
            return Some((target, remainder.strip_prefix(' ').unwrap_or(remainder)));
        }
        offset = end + 1;
    }
    None
}

enum Token {
    Field(String, String),
    Prose,
}

/// Fields follow the message, so only the unbroken run of `key=value` tokens at the end counts.
fn trailing_fields(text: &str) -> BTreeMap<String, String> {
    let mut run: Vec<(String, String)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (token, remainder) = next_token(rest);
        match token {
            Token::Field(key, value) => run.push((key, value)),
            Token::Prose => run.clear(),
        }
        rest = remainder.trim_start_matches(' ');
    }
    run.into_iter().collect()
}

fn next_token(text: &str) -> (Token, &str) {
    let word_end = text.find(' ').unwrap_or(text.len());
    let Some((key, _)) = text[..word_end].split_once('=') else {
        return (Token::Prose, &text[word_end..]);
    };
    if !is_field_name(key) {
        return (Token::Prose, &text[word_end..]);
    }
    let value_start = key.len() + 1;
    let value = &text[value_start..];
    if let Some(quoted) = value.strip_prefix('"') {
        return match unquote(quoted) {
            Some((unquoted, consumed)) => {
                let after = &quoted[consumed..];
                if after.is_empty() || after.starts_with(' ') {
                    (Token::Field(key.to_owned(), unquoted), after)
                } else {
                    (Token::Prose, skip_word(after))
                }
            }
            None => (Token::Prose, ""),
        };
    }
    (
        Token::Field(key.to_owned(), text[value_start..word_end].to_owned()),
        &text[word_end..],
    )
}

fn skip_word(text: &str) -> &str {
    &text[text.find(' ').unwrap_or(text.len())..]
}

fn is_field_name(key: &str) -> bool {
    key.chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

/// Undoes Rust `Debug` string escaping up to the closing quote; yields the text and bytes consumed.
fn unquote(text: &str) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '"' => return Some((out, index + 1)),
            '\\' => {
                let (_, escaped) = chars.next()?;
                match escaped {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    'u' => {
                        let (_, open) = chars.next()?;
                        if open != '{' {
                            return None;
                        }
                        let mut hex = String::new();
                        loop {
                            let (_, digit) = chars.next()?;
                            if digit == '}' {
                                break;
                            }
                            hex.push(digit);
                        }
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    other => out.push(other),
                }
            }
            other => out.push(other),
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> Option<LogRecord> {
        LogRecord::parse(PathBuf::from("forge.log.2026-09-13"), line)
    }

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn file_layer_lines_yield_level_target_and_the_trailing_fields() {
        let cases = [
            (
                r#"2026-09-13T10:00:00.123456Z DEBUG forge::trigger: command trigger did not fire event=01K4Z instance=01K50 kind=twitch.chat.command reason="phrase_mismatch""#,
                "DEBUG",
                "forge::trigger",
                fields(&[
                    ("event", "01K4Z"),
                    ("instance", "01K50"),
                    ("kind", "twitch.chat.command"),
                    ("reason", "phrase_mismatch"),
                ]),
            ),
            (
                "2026-09-13T10:00:00.1Z  INFO forge_desktop: file logging enabled path=~/.local/share/forge/logs",
                "INFO",
                "forge_desktop",
                fields(&[("path", "~/.local/share/forge/logs")]),
            ),
            (
                r#"2026-09-13T10:00:00Z  WARN boot{attempt=1 mode="safe run"}: forge::server: bind retry: a=b in prose arg_count=2 note="said \"hi\" to лісоруб\tthen\u{1b}left""#,
                "WARN",
                "forge::server",
                fields(&[
                    ("arg_count", "2"),
                    ("note", "said \"hi\" to лісоруб\tthen\u{1b}left"),
                ]),
            ),
            (
                "2026-09-13T10:00:00Z ERROR forge::trigger: reason=cooldown mentioned in prose only",
                "ERROR",
                "forge::trigger",
                BTreeMap::new(),
            ),
            (
                "2026-09-13T10:00:00Z TRACE forge::command: viewer=\"a b\"",
                "TRACE",
                "forge::command",
                fields(&[("viewer", "a b")]),
            ),
        ];
        for (line, level, target, expected_fields) in cases {
            let record = parsed(line).unwrap_or_else(|| panic!("not a record: {line}"));
            assert_eq!(
                (
                    record.level.as_str(),
                    record.target.as_str(),
                    &record.fields
                ),
                (level, target, &expected_fields),
                "{line}"
            );
        }
    }

    #[test]
    fn lines_without_the_file_layer_shape_are_not_records() {
        for line in [
            "",
            "   continuation of a multi-line message",
            "2026-09-13T10:00:00Z NOTICE forge::trigger: x=1",
            "2026-09-13T10:00:00Z DEBUG no target here x=1",
            "thread 'main' panicked at src/main.rs:1:1:",
        ] {
            assert!(parsed(line).is_none(), "{line:?}");
        }
    }

    #[test]
    fn a_record_holds_fields_only_when_every_expected_pair_is_equal() {
        let record = parsed(
            r#"2026-09-13T10:00:00Z DEBUG forge::trigger: command matched phrase="!ping" arg_count=0"#,
        )
        .unwrap();
        for (expected, held) in [
            (fields(&[("phrase", "!ping")]), true),
            (fields(&[("phrase", "!ping"), ("arg_count", "0")]), true),
            (fields(&[("phrase", "!Ping")]), false),
            (fields(&[("phrase", "\"!ping\"")]), false),
            (fields(&[("viewer", "alice")]), false),
        ] {
            assert_eq!(record.has_fields(&expected), held, "{expected:?}");
        }
    }
}
