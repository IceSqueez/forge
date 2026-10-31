use std::ops::Range;

use super::TokenClass;
use super::cursor::{Cursor, SpanSink};

const KEYWORDS: [&str; 27] = [
    "fn", "let", "const", "if", "else", "return", "for", "in", "while", "loop", "break",
    "continue", "true", "false", "switch", "import", "as", "throw", "private", "try", "catch",
    "do", "until", "export", "global", "this", "Fn",
];
const RADIX_PREFIXES: [&str; 3] = ["0x", "0b", "0o"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum RhaiState {
    #[default]
    Code,
    InString,
    InBlockComment,
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

fn finish_block_comment(cur: &mut Cursor) -> bool {
    while cur.peek().is_some() {
        if cur.starts_with("*/") {
            cur.bump();
            cur.bump();
            return true;
        }
        cur.bump();
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
    } else if *state == RhaiState::InBlockComment {
        let start = cur.pos();
        if finish_block_comment(&mut cur) {
            *state = RhaiState::Code;
        }
        out.push(cur.pos() - start, TokenClass::Comment);
    }
    while let Some(c) = cur.peek() {
        let start = cur.pos();
        let class = if cur.starts_with("//") {
            cur.eat_while(|c| c != '\n');
            TokenClass::Comment
        } else if cur.starts_with("/*") {
            cur.bump();
            cur.bump();
            if !finish_block_comment(&mut cur) && cur.pos() < src.len() {
                *state = RhaiState::InBlockComment;
            }
            TokenClass::Comment
        } else if c == '"' {
            cur.bump();
            if !finish_string(&mut cur) && cur.pos() < src.len() {
                *state = RhaiState::InString;
            }
            TokenClass::String
        } else if RADIX_PREFIXES.iter().any(|prefix| cur.starts_with(prefix)) {
            cur.bump();
            cur.bump();
            cur.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
            TokenClass::Number
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

#[cfg(test)]
mod tests {
    use crate::highlight::Language::Rhai;
    use crate::highlight::TokenClass::{
        Comment, Function, Keyword, Number, Plain, Punctuation, String,
    };
    use crate::highlight::tests::{assert_pieces, class_of, pieces};

    #[test]
    fn a_block_comment_carries_across_lines_and_code_resumes_after_it() {
        let source = "/* one\ntwo */ let";
        assert_eq!(class_of(Rhai, source, "two"), Some(Comment));
        assert_eq!(class_of(Rhai, source, "let"), Some(Keyword));
    }

    #[test]
    fn radix_literals_stay_one_number() {
        for literal in ["0xFF", "0b1010", "0o17"] {
            assert_eq!(class_of(Rhai, literal, literal), Some(Number), "{literal}");
        }
    }

    #[test]
    fn every_late_keyword_is_a_keyword() {
        for word in [
            "try", "catch", "do", "until", "export", "global", "this", "Fn",
        ] {
            assert_eq!(class_of(Rhai, word, word), Some(Keyword), "{word}");
        }
    }

    #[test]
    fn line_comment_stops_before_the_newline() {
        assert_eq!(
            pieces(Rhai, "// c \"x\nlet"),
            vec![("// c \"x", Comment), ("\n", Punctuation), ("let", Keyword)]
        );
    }

    #[test]
    fn identifier_before_paren_is_a_function_even_across_spaces() {
        assert_pieces(
            Rhai,
            "foo  (1); bar(2)",
            &[("foo", Function), ("bar", Function)],
        );
    }

    #[test]
    fn identifier_with_paren_on_the_next_line_is_plain() {
        assert_eq!(class_of(Rhai, "foo\n(1)", "foo"), Some(Plain));
    }

    #[test]
    fn keywords_win_over_function_and_plain() {
        assert_pieces(
            Rhai,
            "fn (x) { let y = true; if (y) { return x } }",
            &[
                ("fn", Keyword),
                ("let", Keyword),
                ("true", Keyword),
                ("if", Keyword),
                ("return", Keyword),
                ("y", Plain),
            ],
        );
    }

    #[test]
    fn numbers_take_dots_underscores_and_exponents() {
        assert_pieces(
            Rhai,
            "1_000.5e3 + x1",
            &[("1_000.5e3", Number), ("x1", Plain)],
        );
    }

    #[test]
    fn string_with_escaped_newline_continues_on_the_next_line() {
        assert_pieces(
            Rhai,
            "\"a\\\nb\" c",
            &[("\"a\\\nb\"", String), ("c", Plain)],
        );
    }

    #[test]
    fn unterminated_string_colors_every_following_line() {
        assert_eq!(pieces(Rhai, "\"a\nb\nc"), vec![("\"a\nb\nc", String)]);
    }
}
