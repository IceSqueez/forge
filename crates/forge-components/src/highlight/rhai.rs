use std::ops::Range;

use super::TokenClass;
use super::cursor::{Cursor, SpanSink};

const KEYWORDS: [&str; 19] = [
    "fn", "let", "const", "if", "else", "return", "for", "in", "while", "loop", "break",
    "continue", "true", "false", "switch", "import", "as", "throw", "private",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum RhaiState {
    #[default]
    Code,
    InString,
}

fn finish_string(cur: &mut Cursor) -> bool {
    while let Some(c) = cur.bump() {
        match c {
            '\\' => {
                cur.bump();
            }
            '"' => return true,
            _ => {}
        }
    }
    false
}

fn word_class(cur: &Cursor, word: &str) -> TokenClass {
    if KEYWORDS.contains(&word) {
        return TokenClass::Keyword;
    }
    if cur.rest().trim_start_matches(' ').starts_with('(') {
        TokenClass::Function
    } else {
        TokenClass::Plain
    }
}

pub(super) fn lex(src: &str, range: Range<usize>, state: &mut RhaiState, out: &mut SpanSink) {
    let mut cur = Cursor::new(src, range);
    if *state == RhaiState::InString {
        let start = cur.pos();
        if finish_string(&mut cur) {
            *state = RhaiState::Code;
        }
        out.push(cur.pos() - start, TokenClass::String);
    }
    while let Some(c) = cur.peek() {
        let start = cur.pos();
        let class = if cur.starts_with("//") {
            cur.eat_while(|c| c != '\n');
            TokenClass::Comment
        } else if c == '"' {
            cur.bump();
            if !finish_string(&mut cur) && cur.pos() < src.len() {
                *state = RhaiState::InString;
            }
            TokenClass::String
        } else if c.is_ascii_digit() {
            cur.bump();
            cur.eat_while(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | 'e' | 'E'));
            TokenClass::Number
        } else if c.is_alphabetic() || c == '_' {
            cur.bump();
            cur.eat_while(|c| c.is_alphanumeric() || c == '_');
            word_class(&cur, cur.slice_from(start))
        } else if c.is_whitespace() {
            cur.eat_while(char::is_whitespace);
            TokenClass::Punctuation
        } else {
            cur.bump();
            TokenClass::Punctuation
        };
        out.push(cur.pos() - start, class);
    }
}
