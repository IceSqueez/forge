use std::ops::Range;

use super::TokenClass;
use super::cursor::{Cursor, SpanSink};

const KEYWORDS: [&str; 43] = [
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "enum",
];

const EXPRESSION_KEYWORDS: [&str; 15] = [
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
    "extends",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum JsMode {
    #[default]
    Code,
    BlockComment,
    Template,
    QuotedString(char),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct JsState {
    mode: JsMode,
    regex_allowed: bool,
    template_braces: Vec<u32>,
}

impl Default for JsState {
    fn default() -> Self {
        Self {
            mode: JsMode::Code,
            regex_allowed: true,
            template_braces: Vec::new(),
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub(super) fn lex(src: &str, range: Range<usize>, state: &mut JsState, out: &mut SpanSink) {
    let mut cur = Cursor::new(src, range);
    while !cur.is_done() {
        let start = cur.pos();
        let class = match state.mode {
            JsMode::BlockComment => {
                if cur.eat_until("*/") {
                    state.mode = JsMode::Code;
                }
                TokenClass::Comment
            }
            JsMode::QuotedString(quote) => {
                quoted_string_body(&mut cur, state, quote);
                TokenClass::String
            }
            JsMode::Template => {
                if lex_template_until_interpolation(&mut cur, state) {
                    out.push(cur.pos() - start, TokenClass::String);
                    let open = cur.pos();
                    cur.advance("${".len());
                    out.push(cur.pos() - open, TokenClass::Punctuation);
                    state.template_braces.push(0);
                    state.mode = JsMode::Code;
                    state.regex_allowed = true;
                    continue;
                }
                TokenClass::String
            }
            JsMode::Code => code_token(&mut cur, state),
        };
        out.push(cur.pos() - start, class);
    }
}

fn quoted_string_body(cur: &mut Cursor, state: &mut JsState, quote: char) {
    state.mode = JsMode::QuotedString(quote);
    let mut continued = false;
    while let Some(c) = cur.peek() {
        if c == '\n' {
            break;
        }
        cur.bump();
        continued = false;
        if c == '\\' {
            continued = cur.bump() == Some('\n');
        } else if c == quote {
            state.mode = JsMode::Code;
            state.regex_allowed = false;
            return;
        }
    }
    if !continued {
        state.mode = JsMode::Code;
        state.regex_allowed = false;
    }
}

fn lex_template_until_interpolation(cur: &mut Cursor, state: &mut JsState) -> bool {
    while let Some(c) = cur.peek() {
        if cur.starts_with("${") {
            return true;
        }
        cur.bump();
        match c {
            '\\' => {
                cur.bump();
            }
            '`' => {
                state.mode = JsMode::Code;
                state.regex_allowed = false;
                return false;
            }
            _ => {}
        }
    }
    false
}

fn code_token(cur: &mut Cursor, state: &mut JsState) -> TokenClass {
    let Some(c) = cur.peek() else {
        return TokenClass::Plain;
    };
    if c.is_whitespace() {
        cur.eat_while(char::is_whitespace);
        return TokenClass::Plain;
    }
    if cur.starts_with("//") {
        cur.eat_while(|c| c != '\n');
        return TokenClass::Comment;
    }
    if cur.starts_with("/*") {
        cur.advance("/*".len());
        state.mode = JsMode::BlockComment;
        if cur.eat_until("*/") {
            state.mode = JsMode::Code;
        }
        return TokenClass::Comment;
    }
    if c == '/' && state.regex_allowed && regex_literal(cur) {
        state.regex_allowed = false;
        return TokenClass::String;
    }
    if c == '"' || c == '\'' {
        cur.bump();
        quoted_string_body(cur, state, c);
        return TokenClass::String;
    }
    if c == '`' {
        cur.bump();
        state.mode = JsMode::Template;
        return TokenClass::String;
    }
    if c.is_ascii_digit() || (c == '.' && cur.peek_nth(1).is_some_and(|n| n.is_ascii_digit())) {
        number(cur);
        state.regex_allowed = false;
        return TokenClass::Number;
    }
    if is_ident_start(c) {
        let start = cur.pos();
        cur.eat_while(is_ident_continue);
        let word = cur.slice_from(start);
        if KEYWORDS.contains(&word) {
            state.regex_allowed = EXPRESSION_KEYWORDS.contains(&word);
            return TokenClass::Keyword;
        }
        state.regex_allowed = false;
        return if cur.next_non_space() == Some('(') {
            TokenClass::Function
        } else {
            TokenClass::Plain
        };
    }
    cur.bump();
    match c {
        '{' => {
            if let Some(depth) = state.template_braces.last_mut() {
                *depth += 1;
            }
            state.regex_allowed = true;
        }
        '}' => {
            match state.template_braces.last_mut() {
                Some(0) => {
                    state.template_braces.pop();
                    state.mode = JsMode::Template;
                }
                Some(depth) => *depth -= 1,
                None => {}
            }
            state.regex_allowed = false;
        }
        ')' | ']' => state.regex_allowed = false,
        _ if c.is_ascii_punctuation() => state.regex_allowed = true,
        _ => {
            state.regex_allowed = false;
            return TokenClass::Plain;
        }
    }
    TokenClass::Punctuation
}

fn regex_literal(cur: &mut Cursor) -> bool {
    let mut probe = *cur;
    probe.bump();
    let mut in_class = false;
    loop {
        match probe.bump() {
            None | Some('\n') => return false,
            Some('\\') => {
                if matches!(probe.bump(), None | Some('\n')) {
                    return false;
                }
            }
            Some('[') => in_class = true,
            Some(']') => in_class = false,
            Some('/') if !in_class => break,
            Some(_) => {}
        }
    }
    probe.eat_while(is_ident_continue);
    *cur = probe;
    true
}

fn number(cur: &mut Cursor) {
    let hex = cur.starts_with("0x") || cur.starts_with("0X");
    let mut previous = '\0';
    while let Some(c) = cur.peek() {
        let exponent_sign = matches!(c, '+' | '-') && matches!(previous, 'e' | 'E') && !hex;
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || exponent_sign) {
            break;
        }
        cur.bump();
        previous = c;
    }
}

#[cfg(test)]
mod tests {
    use crate::highlight::Language::JavaScript;
    use crate::highlight::TokenClass::{
        Comment, Function, Keyword, Number, Plain, Punctuation, String,
    };
    use crate::highlight::tests::{assert_pieces, class_of, pieces};

    #[test]
    fn template_interpolation_switches_to_code_and_back() {
        assert_eq!(
            pieces(JavaScript, "`a${b}c`"),
            vec![
                ("`a", String),
                ("${", Punctuation),
                ("b", Plain),
                ("}", Punctuation),
                ("c`", String)
            ]
        );
    }

    #[test]
    fn nested_template_returns_to_the_outer_template() {
        assert_pieces(
            JavaScript,
            "`a${`b${c}`}d` + e",
            &[("c", Plain), ("d`", String)],
        );
        assert_eq!(class_of(JavaScript, "`a${`b${c}`}d` + e", "e"), Some(Plain));
    }

    #[test]
    fn object_braces_inside_interpolation_do_not_close_it() {
        assert_pieces(
            JavaScript,
            "`${ {k: 1} }x`",
            &[("k", Plain), ("1", Number), ("x`", String)],
        );
    }

    #[test]
    fn template_spans_lines() {
        assert_pieces(JavaScript, "`a\nb` + c", &[("`a\nb`", String)]);
        assert_eq!(class_of(JavaScript, "`a\nb` + c", "c"), Some(Plain));
    }

    #[test]
    fn slash_after_operator_or_expression_keyword_starts_a_regex() {
        for (text, regex) in [
            ("x = /ab/g;", "/ab/g"),
            ("return /a b/", "/a b/"),
            ("f(/x\\/y/)", "/x\\/y/"),
            ("/re/.test(s)", "/re/"),
        ] {
            assert_pieces(JavaScript, text, &[(regex, String)]);
        }
    }

    #[test]
    fn slash_after_operand_is_division() {
        for text in [
            "a / b / c",
            "(a) / b / c",
            "a[0] / b / c",
            "1 / b / c",
            "'s' / b / c",
        ] {
            assert_eq!(
                class_of(JavaScript, text, "/"),
                Some(Punctuation),
                "{text:?}"
            );
            assert_eq!(class_of(JavaScript, text, "b"), Some(Plain), "{text:?}");
        }
    }

    #[test]
    fn slash_inside_regex_class_does_not_end_the_regex() {
        assert_pieces(JavaScript, "x = /[/]/;", &[("/[/]/", String)]);
    }

    #[test]
    fn unterminated_regex_falls_back_to_punctuation() {
        assert_eq!(
            pieces(JavaScript, "x = /ab\nc"),
            vec![
                ("x ", Plain),
                ("=", Punctuation),
                (" ", Plain),
                ("/", Punctuation),
                ("ab\nc", Plain)
            ]
        );
    }

    #[test]
    fn quoted_string_with_escaped_newline_continues() {
        assert_pieces(JavaScript, "'a\\\nb' + c", &[("'a\\\nb'", String)]);
        assert_eq!(class_of(JavaScript, "'a\\\nb' + c", "c"), Some(Plain));
    }

    #[test]
    fn quoted_string_without_escape_ends_at_the_newline() {
        assert_pieces(JavaScript, "'a\nb + c", &[("'a", String)]);
        assert_eq!(class_of(JavaScript, "'a\nb + c", "b"), Some(Plain));
    }

    #[test]
    fn block_comment_spans_lines() {
        assert_eq!(
            pieces(JavaScript, "/* a\nb */ c"),
            vec![("/* a\nb */", Comment), (" c", Plain)]
        );
    }

    #[test]
    fn number_literals_with_suffix_prefix_leading_dot_and_exponent() {
        assert_pieces(
            JavaScript,
            "[1n, 0x1F, .5, 1e-5, 1_000]",
            &[
                ("1n", Number),
                ("0x1F", Number),
                (".5", Number),
                ("1e-5", Number),
                ("1_000", Number),
            ],
        );
    }

    #[test]
    fn identifier_before_paren_is_function_and_keywords_win() {
        assert_pieces(
            JavaScript,
            "if (f (x)) return typeof y",
            &[
                ("if", Keyword),
                ("f", Function),
                ("return", Keyword),
                ("typeof", Keyword),
            ],
        );
        assert_eq!(class_of(JavaScript, "return typeof z", "z"), Some(Plain));
    }
}
