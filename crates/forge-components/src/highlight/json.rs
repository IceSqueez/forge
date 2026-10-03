use std::ops::Range;

use super::TokenClass;
use super::cursor::{Cursor, SpanSink};

const LITERALS: [&str; 3] = ["true", "false", "null"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum JsonState {
    #[default]
    Value,
    InString {
        key: bool,
    },
}

fn string_class(key: bool) -> TokenClass {
    if key {
        TokenClass::Variable
    } else {
        TokenClass::String
    }
}

fn string_end_or_text_end(src: &str, from: usize) -> usize {
    let body = src.get(from..).unwrap_or("");
    let mut chars = body.char_indices();
    while let Some((offset, c)) = chars.next() {
        match c {
            '\\' => {
                if chars.next().is_none() {
                    break;
                }
            }
            '"' => return from + offset + c.len_utf8(),
            _ => {}
        }
    }
    src.len()
}

fn is_key(src: &str, string_end: usize) -> bool {
    src.get(string_end..)
        .unwrap_or("")
        .chars()
        .find(|c| !c.is_whitespace())
        == Some(':')
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

pub(super) fn lex(src: &str, range: Range<usize>, state: &mut JsonState, out: &mut SpanSink) {
    let mut cur = Cursor::new(src, range);
    if let JsonState::InString { key } = *state {
        let start = cur.pos();
        if finish_string(&mut cur) {
            *state = JsonState::Value;
        }
        out.push(cur.pos() - start, string_class(key));
    }
    while let Some(c) = cur.peek() {
        let start = cur.pos();
        let class = if c.is_whitespace() {
            cur.eat_while(char::is_whitespace);
            TokenClass::Punctuation
        } else if c == '"' {
            cur.bump();
            let closed = finish_string(&mut cur);
            let key = if closed {
                is_key(src, cur.pos())
            } else {
                is_key(src, string_end_or_text_end(src, cur.pos()))
            };
            if !closed && cur.pos() < src.len() {
                *state = JsonState::InString { key };
            }
            string_class(key)
        } else if c == '-' || c.is_ascii_digit() {
            cur.bump();
            cur.eat_while(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
            TokenClass::Number
        } else if let Some(literal) = LITERALS.iter().find(|word| cur.starts_with(word)) {
            cur.advance(literal.len());
            TokenClass::Keyword
        } else {
            cur.bump();
            TokenClass::Plain
        };
        out.push(cur.pos() - start, class);
    }
}

#[cfg(test)]
mod tests {
    use crate::highlight::Language::Json;
    use crate::highlight::TokenClass::{Keyword, Number, Plain, Punctuation, String, Variable};
    use crate::highlight::tests::{assert_pieces, pieces};

    #[test]
    fn string_followed_by_colon_is_a_key_and_otherwise_a_value() {
        assert_pieces(
            Json,
            "{\"a\": \"b\", \"c\" :[\"d\"]}",
            &[
                ("\"a\"", Variable),
                ("\"b\"", String),
                ("\"c\"", Variable),
                ("\"d\"", String),
            ],
        );
    }

    #[test]
    fn key_whose_colon_sits_on_the_next_line_is_still_a_key() {
        assert_pieces(Json, "{\"a\"\n  : 1}", &[("\"a\"", Variable)]);
    }

    #[test]
    fn escaped_quote_does_not_end_the_string() {
        assert_pieces(
            Json,
            "[\"a\\\"b\", 1]",
            &[("\"a\\\"b\"", String), ("1", Number)],
        );
    }

    #[test]
    fn unterminated_string_colors_to_the_end_of_the_document() {
        assert_eq!(
            pieces(Json, "[1, \"ab\n2]\n"),
            vec![
                ("[", Plain),
                ("1", Number),
                (",", Plain),
                (" ", Punctuation),
                ("\"ab\n2]\n", String)
            ]
        );
    }

    #[test]
    fn unterminated_key_that_closes_on_a_later_line_keeps_key_class() {
        assert_pieces(Json, "{\"a\nb\": 1}", &[("\"a\nb\"", Variable)]);
    }

    #[test]
    fn literal_prefix_glued_to_a_word_splits_into_keyword_and_plain() {
        assert_eq!(pieces(Json, "nullx"), vec![("null", Keyword), ("x", Plain)]);
    }

    #[test]
    fn signed_exponent_number_is_one_number_token() {
        assert_eq!(pieces(Json, "-1e+5"), vec![("-1e+5", Number)]);
    }

    #[test]
    fn true_false_null_are_keywords() {
        assert_pieces(
            Json,
            "[true,false,null]",
            &[("true", Keyword), ("false", Keyword), ("null", Keyword)],
        );
    }
}
