use std::ops::Range;

use iced::Color;
use iced::advanced::text::Highlighter as IcedHighlighter;

use crate::palette::{CATPPUCCIN_MOCHA, ForgePalette};

#[derive(Debug, Clone, PartialEq)]
pub struct RhaiHighlighterSettings {
    pub error_lines: Vec<usize>,
    pub palette: ForgePalette,
}

impl Default for RhaiHighlighterSettings {
    fn default() -> Self {
        Self {
            error_lines: Vec::new(),
            palette: CATPPUCCIN_MOCHA,
        }
    }
}

/// `in_block_comment` state recorded AFTER processing the line at index N.
pub struct RhaiHighlighter {
    settings: RhaiHighlighterSettings,
    current_line: usize,
    in_block_comment: bool,
    line_states: Vec<bool>,
}

fn token_color(kind: RhaiTokenKind, palette: &ForgePalette) -> Option<Color> {
    match kind {
        RhaiTokenKind::Keyword => Some(palette.code_keyword),
        RhaiTokenKind::FunctionCall => Some(palette.code_fn),
        RhaiTokenKind::StringLit | RhaiTokenKind::TemplateLit => Some(palette.code_str),
        RhaiTokenKind::Number => Some(palette.code_num),
        RhaiTokenKind::Namespace => Some(palette.warning),
        RhaiTokenKind::Comment => Some(palette.code_comment),
        RhaiTokenKind::Identifier | RhaiTokenKind::Operator | RhaiTokenKind::Punctuation => None,
    }
}

impl IcedHighlighter for RhaiHighlighter {
    type Settings = RhaiHighlighterSettings;
    type Highlight = Option<Color>;
    type Iterator<'a>
        = std::vec::IntoIter<(Range<usize>, Option<Color>)>
    where
        Self: 'a;

    fn new(settings: &RhaiHighlighterSettings) -> Self {
        Self {
            settings: settings.clone(),
            current_line: 0,
            in_block_comment: false,
            line_states: Vec::new(),
        }
    }

    fn update(&mut self, new_settings: &RhaiHighlighterSettings) {
        self.settings = new_settings.clone();
        self.current_line = 0;
        self.in_block_comment = false;
        self.line_states.clear();
    }

    fn change_line(&mut self, line: usize) {
        if line < self.current_line {
            self.in_block_comment = if line == 0 {
                false
            } else {
                self.line_states.get(line - 1).copied().unwrap_or(false)
            };
            self.line_states.truncate(line);
            self.current_line = line;
        }
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let (raw_tokens, next_state) = tokenize_line(line, self.in_block_comment);

        if self.current_line < self.line_states.len() {
            self.line_states[self.current_line] = next_state;
        } else {
            self.line_states.push(next_state);
        }
        self.in_block_comment = next_state;

        let error = self.settings.error_lines.contains(&self.current_line);
        self.current_line += 1;

        let palette = self.settings.palette;
        raw_tokens
            .into_iter()
            .map(move |(range, kind)| {
                let color = if error {
                    Some(palette.random)
                } else {
                    token_color(kind, &palette)
                };
                (range, color)
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RhaiTokenKind {
    Keyword,
    Number,
    StringLit,
    TemplateLit,
    Comment,
    Namespace,
    FunctionCall,
    Identifier,
    Operator,
    Punctuation,
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "const", "if", "else", "while", "for", "loop", "return", "break", "continue",
    "do", "until", "switch", "try", "catch", "throw", "in", "true", "false", "import", "export",
    "as", "private",
];

/// Byte ranges are relative to `line`; caller threads `in_block_comment` across lines.
pub fn tokenize_line(
    line: &str,
    in_block_comment: bool,
) -> (Vec<(Range<usize>, RhaiTokenKind)>, bool) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut tokens: Vec<(Range<usize>, RhaiTokenKind)> = Vec::new();
    let mut pos = 0usize;
    let mut block_open = in_block_comment;

    if block_open {
        match find_block_close(bytes, 0) {
            Some(close_star) => {
                let end = close_star + 2;
                tokens.push((0..end, RhaiTokenKind::Comment));
                block_open = false;
                pos = end;
            }
            None => {
                if !line.is_empty() {
                    tokens.push((0..len, RhaiTokenKind::Comment));
                }
                return (tokens, true);
            }
        }
    }

    while pos < len {
        let b = bytes[pos];

        if matches!(b, b' ' | b'\t' | b'\r') {
            pos += 1;
            continue;
        }

        if pos + 1 < len && b == b'/' && bytes[pos + 1] == b'/' {
            tokens.push((pos..len, RhaiTokenKind::Comment));
            return (tokens, false);
        }

        if pos + 1 < len && b == b'/' && bytes[pos + 1] == b'*' {
            let start = pos;
            pos += 2;
            match find_block_close(bytes, pos) {
                Some(close_star) => {
                    let end = close_star + 2;
                    tokens.push((start..end, RhaiTokenKind::Comment));
                    pos = end;
                }
                None => {
                    tokens.push((start..len, RhaiTokenKind::Comment));
                    return (tokens, true);
                }
            }
            continue;
        }

        if b == b'"' {
            let start = pos;
            pos += 1;
            while pos < len {
                let c = bytes[pos];
                if c == b'\\' {
                    pos += 1;
                    if pos < len {
                        if bytes[pos] == b'x' {
                            pos = (pos + 3).min(len);
                        } else if bytes[pos] == b'u' {
                            pos += 1;
                            if pos < len && bytes[pos] == b'{' {
                                pos += 1;
                                while pos < len && bytes[pos] != b'}' {
                                    pos += 1;
                                }
                                pos = (pos + 1).min(len);
                            }
                        } else {
                            pos += 1;
                        }
                    }
                } else if c == b'"' {
                    pos += 1;
                    break;
                } else {
                    pos += 1;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::StringLit));
            continue;
        }

        if b == b'`' {
            let start = pos;
            pos += 1;
            while pos < len {
                let c = bytes[pos];
                if c == b'\\' {
                    pos += 1;
                    if pos < len {
                        pos += 1;
                    }
                } else if c == b'`' {
                    pos += 1;
                    break;
                } else {
                    pos += 1;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::TemplateLit));
            continue;
        }

        if b.is_ascii_digit() {
            let start = pos;
            if b == b'0'
                && pos + 1 < len
                && matches!(bytes[pos + 1], b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            {
                pos += 2;
                while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                    pos += 1;
                }
                tokens.push((start..pos, RhaiTokenKind::Number));
                continue;
            }
            while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                pos += 1;
            }
            if pos + 1 < len && bytes[pos] == b'.' && bytes[pos + 1].is_ascii_digit() {
                pos += 1;
                while pos < len && (bytes[pos].is_ascii_digit() || bytes[pos] == b'_') {
                    pos += 1;
                }
            }
            if pos < len && matches!(bytes[pos], b'e' | b'E') {
                let exp_start = pos;
                pos += 1;
                if pos < len && matches!(bytes[pos], b'+' | b'-') {
                    pos += 1;
                }
                if pos < len && bytes[pos].is_ascii_digit() {
                    while pos < len && bytes[pos].is_ascii_digit() {
                        pos += 1;
                    }
                } else {
                    pos = exp_start;
                }
            }
            tokens.push((start..pos, RhaiTokenKind::Number));
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            let start = pos;
            pos += 1;
            while pos < len && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'_') {
                pos += 1;
            }
            let word = &line[start..pos];

            if pos + 1 < len && bytes[pos] == b':' && bytes[pos + 1] == b':' {
                tokens.push((start..pos, RhaiTokenKind::Namespace));
                tokens.push((pos..pos + 2, RhaiTokenKind::Punctuation));
                pos += 2;
                continue;
            }

            let mut look = pos;
            while look < len && matches!(bytes[look], b' ' | b'\t') {
                look += 1;
            }
            if look < len && bytes[look] == b'(' {
                if KEYWORDS.contains(&word) {
                    tokens.push((start..pos, RhaiTokenKind::Keyword));
                } else {
                    tokens.push((start..pos, RhaiTokenKind::FunctionCall));
                }
                continue;
            }

            if KEYWORDS.contains(&word) {
                tokens.push((start..pos, RhaiTokenKind::Keyword));
            } else {
                tokens.push((start..pos, RhaiTokenKind::Identifier));
            }
            continue;
        }

        if pos + 2 < len && bytes[pos] == b'.' && bytes[pos + 1] == b'.' && bytes[pos + 2] == b'=' {
            tokens.push((pos..pos + 3, RhaiTokenKind::Operator));
            pos += 3;
            continue;
        }

        if pos + 1 < len {
            let (a, c) = (bytes[pos], bytes[pos + 1]);
            if matches!(
                (a, c),
                (b'=', b'=')
                    | (b'!', b'=')
                    | (b'<', b'=')
                    | (b'>', b'=')
                    | (b'&', b'&')
                    | (b'|', b'|')
                    | (b'?', b'?')
                    | (b'?', b'.')
                    | (b'.', b'.')
                    | (b'<', b'<')
                    | (b'>', b'>')
                    | (b'+', b'=')
                    | (b'-', b'=')
                    | (b'*', b'=')
                    | (b'/', b'=')
                    | (b'%', b'=')
            ) {
                tokens.push((pos..pos + 2, RhaiTokenKind::Operator));
                pos += 2;
                continue;
            }
        }

        if matches!(
            b,
            b'+' | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'&'
                | b'|'
                | b'^'
                | b'!'
                | b'?'
        ) {
            tokens.push((pos..pos + 1, RhaiTokenKind::Operator));
            pos += 1;
            continue;
        }

        tokens.push((pos..pos + 1, RhaiTokenKind::Punctuation));
        pos += 1;
    }

    (tokens, block_open)
}

fn find_block_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kind_eq_reflexive() {
        assert_eq!(RhaiTokenKind::Keyword, RhaiTokenKind::Keyword);
        assert_eq!(RhaiTokenKind::Number, RhaiTokenKind::Number);
        assert_eq!(RhaiTokenKind::StringLit, RhaiTokenKind::StringLit);
        assert_eq!(RhaiTokenKind::TemplateLit, RhaiTokenKind::TemplateLit);
        assert_eq!(RhaiTokenKind::Comment, RhaiTokenKind::Comment);
        assert_eq!(RhaiTokenKind::Namespace, RhaiTokenKind::Namespace);
        assert_eq!(RhaiTokenKind::FunctionCall, RhaiTokenKind::FunctionCall);
        assert_eq!(RhaiTokenKind::Identifier, RhaiTokenKind::Identifier);
        assert_eq!(RhaiTokenKind::Operator, RhaiTokenKind::Operator);
        assert_eq!(RhaiTokenKind::Punctuation, RhaiTokenKind::Punctuation);
    }

    #[test]
    fn token_kind_ne_distinct() {
        assert_ne!(RhaiTokenKind::Keyword, RhaiTokenKind::Identifier);
        assert_ne!(RhaiTokenKind::Number, RhaiTokenKind::StringLit);
        assert_ne!(RhaiTokenKind::Comment, RhaiTokenKind::FunctionCall);
    }

    #[test]
    fn token_kind_clone_round_trip() {
        let original = RhaiTokenKind::Namespace;
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn highlighter_settings_default_has_empty_error_lines() {
        let s = RhaiHighlighterSettings::default();
        assert!(s.error_lines.is_empty());
    }

    #[test]
    fn highlighter_settings_default_has_catppuccin_palette() {
        let s = RhaiHighlighterSettings::default();
        assert_eq!(s.palette, CATPPUCCIN_MOCHA);
    }

    #[test]
    fn highlighter_settings_error_lines_round_trip() {
        let s = RhaiHighlighterSettings {
            error_lines: vec![1, 5, 12],
            ..Default::default()
        };
        assert_eq!(s.error_lines, [1, 5, 12]);
    }

    #[test]
    fn highlighter_forward_walk_line_states_and_current_line() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let mut h = RhaiHighlighter::new(&RhaiHighlighterSettings::default());
        assert_eq!(h.current_line(), 0);

        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 1);
        assert_eq!(h.line_states.len(), 1);
        assert!(!h.line_states[0]);

        let _ = h.highlight_line("/* open").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 2);
        assert_eq!(h.line_states.len(), 2);
        assert!(h.line_states[1]);

        let _ = h.highlight_line("still in comment */").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 3);
        assert_eq!(h.line_states.len(), 3);
        assert!(!h.line_states[2]);
    }

    #[test]
    fn highlighter_backward_change_line_rewinds_block_comment_state() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let mut h = RhaiHighlighter::new(&RhaiHighlighterSettings::default());
        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        let _ = h.highlight_line("/* open block").collect::<Vec<_>>();
        let _ = h.highlight_line("still inside */").collect::<Vec<_>>();
        assert_eq!(h.current_line(), 3);

        h.change_line(0);
        assert_eq!(h.current_line(), 0);
        assert!(!h.in_block_comment);
        assert!(h.line_states.is_empty());
    }

    #[test]
    fn highlighter_error_lines_override_all_token_colors_to_random() {
        use iced::advanced::text::Highlighter as IcedHighlighter;

        let settings = RhaiHighlighterSettings {
            error_lines: vec![1],
            palette: CATPPUCCIN_MOCHA,
        };
        let mut h = RhaiHighlighter::new(&settings);
        let _ = h.highlight_line("let x = 1;").collect::<Vec<_>>();
        let tokens: Vec<_> = h.highlight_line("let y = \"hello\";").collect();
        assert!(!tokens.is_empty());
        for (_, color) in &tokens {
            assert_eq!(*color, Some(CATPPUCCIN_MOCHA.random));
        }
    }

    #[test]
    fn empty_line_no_block_comment() {
        let (tokens, next) = tokenize_line("", false);
        assert!(tokens.is_empty());
        assert!(!next);
    }

    #[test]
    fn empty_line_in_block_comment() {
        let (tokens, next) = tokenize_line("", true);
        assert!(tokens.is_empty());
        assert!(next);
    }

    #[test]
    fn globals_call_exact_token_sequence() {
        let line = r#"let q = sl::globals::get("counter");"#;
        let (tokens, next) = tokenize_line(line, false);
        assert!(!next);
        let kinds: Vec<RhaiTokenKind> = tokens.iter().map(|(_, k)| *k).collect();
        assert_eq!(
            kinds,
            [
                RhaiTokenKind::Keyword,
                RhaiTokenKind::Identifier,
                RhaiTokenKind::Operator,
                RhaiTokenKind::Namespace,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::Namespace,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::FunctionCall,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::StringLit,
                RhaiTokenKind::Punctuation,
                RhaiTokenKind::Punctuation,
            ]
        );
        assert_eq!(tokens[0].0, 0..3);
        assert_eq!(tokens[1].0, 4..5);
        assert_eq!(tokens[2].0, 6..7);
        assert_eq!(tokens[3].0, 8..10);
        assert_eq!(tokens[4].0, 10..12);
        assert_eq!(tokens[5].0, 12..19);
        assert_eq!(tokens[6].0, 19..21);
        assert_eq!(tokens[7].0, 21..24);
        assert_eq!(tokens[8].0, 24..25);
        assert_eq!(tokens[9].0, 25..34);
        assert_eq!(tokens[10].0, 34..35);
        assert_eq!(tokens[11].0, 35..36);
    }

    #[test]
    fn block_comment_across_three_lines() {
        let (t1, s1) = tokenize_line("/* a", false);
        assert_eq!(t1, [(0..4, RhaiTokenKind::Comment)]);
        assert!(s1);

        let (t2, s2) = tokenize_line("b", s1);
        assert_eq!(t2, [(0..1, RhaiTokenKind::Comment)]);
        assert!(s2);

        let (t3, s3) = tokenize_line("c */", s2);
        assert_eq!(t3, [(0..4, RhaiTokenKind::Comment)]);
        assert!(!s3);
    }

    #[test]
    fn template_literal_entire_span() {
        let line = "`hello ${name}`";
        let (tokens, next) = tokenize_line(line, false);
        assert!(!next);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].1, RhaiTokenKind::TemplateLit);
        assert_eq!(tokens[0].0, 0..line.len());
    }

    #[test]
    fn number_decimal_integer() {
        let (tokens, _) = tokenize_line("42", false);
        assert_eq!(tokens, [(0..2, RhaiTokenKind::Number)]);
    }

    #[test]
    fn number_float() {
        let (tokens, _) = tokenize_line("3.14", false);
        assert_eq!(tokens, [(0..4, RhaiTokenKind::Number)]);
    }

    #[test]
    fn number_hex_prefix() {
        let (tokens, _) = tokenize_line("0xFF", false);
        assert_eq!(tokens, [(0..4, RhaiTokenKind::Number)]);
    }

    #[test]
    fn number_underscore_separator() {
        let (tokens, _) = tokenize_line("1_000", false);
        assert_eq!(tokens, [(0..5, RhaiTokenKind::Number)]);
    }

    #[test]
    fn number_float_exponent() {
        let (tokens, _) = tokenize_line("1.5e10", false);
        assert_eq!(tokens, [(0..6, RhaiTokenKind::Number)]);
    }

    #[test]
    fn number_binary_prefix() {
        let (tokens, _) = tokenize_line("0b1010", false);
        assert_eq!(tokens, [(0..6, RhaiTokenKind::Number)]);
    }
}
