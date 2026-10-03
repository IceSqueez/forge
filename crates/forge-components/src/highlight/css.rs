use std::ops::Range;

use super::TokenClass;
use super::cursor::{Cursor, SpanSink};

const RULE_BLOCK_AT_RULES: [&str; 8] = [
    "media",
    "supports",
    "layer",
    "container",
    "document",
    "scope",
    "keyframes",
    "starting-style",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum CssMode {
    #[default]
    Code,
    Comment,
    QuotedString(char),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum CssContext {
    #[default]
    Selector,
    AtRulePrelude {
        holds_rules: bool,
    },
    DeclarationStart,
    Value,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Block {
    Rules,
    Declarations,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(super) struct CssState {
    mode: CssMode,
    context: CssContext,
    blocks: Vec<Block>,
}

impl CssState {
    fn block_context(&self) -> CssContext {
        match self.blocks.last() {
            Some(Block::Declarations) => CssContext::DeclarationStart,
            Some(Block::Rules) | None => CssContext::Selector,
        }
    }

    fn open_block(&mut self) {
        let block = match self.context {
            CssContext::AtRulePrelude { holds_rules: true } => Block::Rules,
            _ => Block::Declarations,
        };
        self.blocks.push(block);
        self.context = self.block_context();
    }

    fn close_block(&mut self) {
        self.blocks.pop();
        self.context = self.block_context();
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

fn is_ident_start_char(c: char) -> bool {
    c.is_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_valid_escape(first: Option<char>, second: Option<char>) -> bool {
    first == Some('\\') && second.is_some_and(|c| c != '\n')
}

fn starts_ident(cur: &Cursor) -> bool {
    let first = cur.peek();
    let second = cur.peek_nth(1);
    match first {
        Some('-') => {
            second.is_some_and(|c| is_ident_start_char(c) || c == '-')
                || is_valid_escape(second, cur.peek_nth(2))
        }
        Some('\\') => is_valid_escape(first, second),
        Some(c) => is_ident_start_char(c),
        None => false,
    }
}

fn starts_number(cur: &Cursor) -> bool {
    let digit_at = |n: usize| cur.peek_nth(n).is_some_and(|c| c.is_ascii_digit());
    match cur.peek() {
        Some('+' | '-') => digit_at(1) || (cur.peek_nth(1) == Some('.') && digit_at(2)),
        Some('.') => digit_at(1),
        Some(c) => c.is_ascii_digit(),
        None => false,
    }
}

fn eat_name(cur: &mut Cursor) {
    loop {
        if is_valid_escape(cur.peek(), cur.peek_nth(1)) {
            cur.bump();
            cur.bump();
        } else if cur.peek().is_some_and(is_name_char) {
            cur.bump();
        } else {
            break;
        }
    }
}

fn eat_number(cur: &mut Cursor) {
    let digit_at = |cur: &Cursor, n: usize| cur.peek_nth(n).is_some_and(|c| c.is_ascii_digit());
    if matches!(cur.peek(), Some('+' | '-')) {
        cur.bump();
    }
    cur.eat_while(|c| c.is_ascii_digit());
    if cur.peek() == Some('.') && digit_at(cur, 1) {
        cur.bump();
        cur.eat_while(|c| c.is_ascii_digit());
    }
    if matches!(cur.peek(), Some('e' | 'E')) {
        let signed = matches!(cur.peek_nth(1), Some('+' | '-'));
        if digit_at(cur, 1) || (signed && digit_at(cur, 2)) {
            cur.bump();
            if signed {
                cur.bump();
            }
            cur.eat_while(|c| c.is_ascii_digit());
        }
    }
    if starts_ident(cur) {
        eat_name(cur);
    } else if cur.peek() == Some('%') {
        cur.bump();
    }
}

fn holds_rules(at_rule: &str) -> bool {
    let name = at_rule.to_ascii_lowercase();
    RULE_BLOCK_AT_RULES.iter().any(|rule| {
        name.strip_suffix(rule)
            .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('-'))
    })
}

pub(super) fn lex(src: &str, range: Range<usize>, state: &mut CssState, out: &mut SpanSink) {
    let mut cur = Cursor::new(src, range);
    while !cur.is_done() {
        let start = cur.pos();
        match state.mode {
            CssMode::Comment => {
                if cur.eat_until("*/") {
                    state.mode = CssMode::Code;
                }
                out.push(cur.pos() - start, TokenClass::Comment);
            }
            CssMode::QuotedString(quote) => {
                string_body(&mut cur, state, quote);
                out.push(cur.pos() - start, TokenClass::String);
            }
            CssMode::Code => token(&mut cur, state, out),
        }
    }
}

fn string_body(cur: &mut Cursor, state: &mut CssState, quote: char) {
    state.mode = CssMode::QuotedString(quote);
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
            state.mode = CssMode::Code;
            return;
        }
    }
    if !continued {
        state.mode = CssMode::Code;
    }
}

fn token(cur: &mut Cursor, state: &mut CssState, out: &mut SpanSink) {
    let start = cur.pos();
    let Some(c) = cur.peek() else {
        return;
    };
    let class = if c.is_whitespace() {
        cur.eat_while(char::is_whitespace);
        TokenClass::Plain
    } else if cur.starts_with("/*") {
        cur.advance("/*".len());
        state.mode = CssMode::Comment;
        if cur.eat_until("*/") {
            state.mode = CssMode::Code;
        }
        TokenClass::Comment
    } else if c == '"' || c == '\'' {
        cur.bump();
        string_body(cur, state, c);
        TokenClass::String
    } else if c == '@' && {
        let mut probe = *cur;
        probe.bump();
        starts_ident(&probe)
    } {
        cur.bump();
        let name_start = cur.pos();
        eat_name(cur);
        let holds_rules = holds_rules(cur.slice_from(name_start));
        state.context = CssContext::AtRulePrelude { holds_rules };
        TokenClass::Keyword
    } else if c == '{' {
        cur.bump();
        state.open_block();
        TokenClass::Punctuation
    } else if c == '}' {
        cur.bump();
        state.close_block();
        TokenClass::Punctuation
    } else if c == ';' {
        cur.bump();
        state.context = state.block_context();
        TokenClass::Punctuation
    } else {
        return context_token(cur, state, out);
    };
    out.push(cur.pos() - start, class);
}

fn context_token(cur: &mut Cursor, state: &mut CssState, out: &mut SpanSink) {
    match state.context {
        CssContext::Selector => selector_token(cur, out),
        CssContext::DeclarationStart => declaration_start_token(cur, state, out),
        CssContext::Value | CssContext::AtRulePrelude { .. } => {
            value_token(cur, state.context == CssContext::Value, out)
        }
    }
}

fn selector_token(cur: &mut Cursor, out: &mut SpanSink) {
    let start = cur.pos();
    let class = match cur.peek() {
        Some('.')
            if starts_ident(&{
                let mut probe = *cur;
                probe.bump();
                probe
            }) =>
        {
            cur.bump();
            eat_name(cur);
            TokenClass::Function
        }
        Some('#') if cur.peek_nth(1).is_some_and(is_name_char) => {
            cur.bump();
            eat_name(cur);
            TokenClass::Function
        }
        Some(':') => {
            cur.eat_while(|c| c == ':');
            if starts_ident(cur) {
                eat_name(cur);
                TokenClass::Keyword
            } else {
                TokenClass::Punctuation
            }
        }
        _ if starts_number(cur) => {
            eat_number(cur);
            TokenClass::Number
        }
        _ if starts_ident(cur) => {
            eat_name(cur);
            TokenClass::Function
        }
        _ => {
            cur.bump();
            TokenClass::Punctuation
        }
    };
    out.push(cur.pos() - start, class);
}

fn declaration_start_token(cur: &mut Cursor, state: &mut CssState, out: &mut SpanSink) {
    if !starts_ident(cur) {
        if matches!(
            cur.peek(),
            Some('&' | '.' | '#' | ':' | '*' | '>' | '+' | '~' | '[')
        ) {
            state.context = CssContext::Selector;
            selector_token(cur, out);
        } else {
            let start = cur.pos();
            cur.bump();
            out.push(cur.pos() - start, TokenClass::Punctuation);
        }
        return;
    }
    let start = cur.pos();
    eat_name(cur);
    let class = if cur.next_non_space() == Some(':') {
        state.context = CssContext::Value;
        if cur.slice_from(start).starts_with("--") {
            TokenClass::Variable
        } else {
            TokenClass::Keyword
        }
    } else {
        state.context = CssContext::Selector;
        TokenClass::Function
    };
    out.push(cur.pos() - start, class);
}

fn value_token(cur: &mut Cursor, in_declaration: bool, out: &mut SpanSink) {
    let start = cur.pos();
    if starts_number(cur) {
        eat_number(cur);
        out.push(cur.pos() - start, TokenClass::Number);
        return;
    }
    if in_declaration && cur.peek() == Some('#') && cur.peek_nth(1).is_some_and(is_name_char) {
        cur.bump();
        eat_name(cur);
        out.push(cur.pos() - start, TokenClass::Number);
        return;
    }
    if cur.peek() == Some('!') && {
        let mut probe = *cur;
        probe.bump();
        probe.eat_while(|c| c == ' ' || c == '\t');
        starts_ident(&probe)
    } {
        cur.bump();
        cur.eat_while(|c| c == ' ' || c == '\t');
        eat_name(cur);
        out.push(cur.pos() - start, TokenClass::Keyword);
        return;
    }
    if !starts_ident(cur) {
        cur.bump();
        out.push(cur.pos() - start, TokenClass::Punctuation);
        return;
    }
    eat_name(cur);
    let name = cur.slice_from(start);
    if name.starts_with("--") {
        out.push(cur.pos() - start, TokenClass::Variable);
        return;
    }
    if cur.peek() != Some('(') {
        out.push(cur.pos() - start, TokenClass::Plain);
        return;
    }
    let is_url = name.eq_ignore_ascii_case("url");
    out.push(cur.pos() - start, TokenClass::Function);
    let paren = cur.pos();
    cur.bump();
    out.push(cur.pos() - paren, TokenClass::Punctuation);
    let unquoted_url =
        is_url && !matches!(cur.rest().trim_start().chars().next(), Some('"' | '\''));
    if unquoted_url {
        let body = cur.pos();
        cur.eat_while(|c| c != ')' && c != '\n');
        out.push(cur.pos() - body, TokenClass::String);
    }
}

#[cfg(test)]
mod tests {
    use crate::highlight::Language::Css;
    use crate::highlight::TokenClass::{
        Comment, Function, Keyword, Number, Plain, Punctuation, String, Variable,
    };
    use crate::highlight::tests::{assert_pieces, class_of, pieces};

    #[test]
    fn media_block_holds_rules_so_inner_selectors_stay_selectors() {
        assert_pieces(
            Css,
            "@media screen {\n  a:hover { color: red }\n}",
            &[
                ("@media", Keyword),
                ("a", Function),
                (":hover", Keyword),
                ("color", Keyword),
            ],
        );
    }

    #[test]
    fn vendor_prefixed_keyframes_block_holds_rules() {
        assert_pieces(
            Css,
            "@-webkit-keyframes k { 50% { top: 0 } }",
            &[("50%", Number), ("top", Keyword)],
        );
    }

    #[test]
    fn declaration_at_rule_block_holds_properties() {
        assert_pieces(Css, "@font-face { src: x }", &[("src", Keyword)]);
    }

    #[test]
    fn statement_at_rule_ends_at_semicolon_and_returns_to_selectors() {
        assert_pieces(
            Css,
            "@import url(x.css) \"y.css\";\na:hover {}",
            &[
                ("@import", Keyword),
                ("url", Function),
                ("x.css", String),
                ("\"y.css\"", String),
                (":hover", Keyword),
            ],
        );
    }

    #[test]
    fn declaration_start_tells_properties_from_nested_selectors() {
        assert_pieces(
            Css,
            "a {\n  color: red;\n  .b { top: 0 }\n  i { left: 0 }\n  &:hover { right: 0 }\n}",
            &[
                ("color", Keyword),
                (".b", Function),
                ("top", Keyword),
                ("i", Function),
                ("left", Keyword),
                (":hover", Keyword),
                ("right", Keyword),
            ],
        );
    }

    #[test]
    fn custom_property_and_its_var_reference_are_variables() {
        assert_eq!(
            pieces(Css, "a { --x: 1; b: var(--x) }")
                .into_iter()
                .filter(|piece| piece.1 == Variable)
                .collect::<Vec<_>>(),
            vec![("--x", Variable), ("--x", Variable)]
        );
        assert_eq!(class_of(Css, "a { b: var(--x) }", "var"), Some(Function));
    }

    #[test]
    fn numeric_values_with_sign_exponent_unit_and_percent_are_numbers() {
        assert_pieces(
            Css,
            "a { m: -5px 1.5e3 50% .5em +1 #fff }",
            &[
                ("-5px", Number),
                ("1.5e3", Number),
                ("50%", Number),
                (".5em", Number),
                ("+1", Number),
                ("#fff", Number),
            ],
        );
    }

    #[test]
    fn hash_in_selector_is_an_id_selector_not_a_color() {
        assert_eq!(pieces(Css, "#fff{}")[0], ("#fff", Function));
    }

    #[test]
    fn important_flag_is_a_keyword_with_or_without_space() {
        assert_pieces(
            Css,
            "a { b: red !important; c: blue ! important }",
            &[("!important", Keyword), ("! important", Keyword)],
        );
    }

    #[test]
    fn unquoted_url_body_is_a_string() {
        assert_pieces(
            Css,
            "a { b: url(a.png) }",
            &[("url", Function), ("a.png", String)],
        );
    }

    #[test]
    fn string_with_escaped_newline_continues_on_the_next_line() {
        assert_pieces(
            Css,
            "a { b: \"x\\\ny\"; color: red }",
            &[("\"x\\\ny\"", String), ("color", Keyword)],
        );
    }

    #[test]
    fn unescaped_newline_ends_a_bad_string() {
        assert_pieces(Css, "a { b: \"x\ny }", &[("\"x", String)]);
    }

    #[test]
    fn unterminated_comment_colors_the_rest_of_the_document() {
        assert_eq!(
            pieces(Css, "a {}\n/* x\ny {}"),
            vec![
                ("a", Function),
                (" ", Plain),
                ("{}", Punctuation),
                ("\n", Plain),
                ("/* x\ny {}", Comment)
            ]
        );
    }

    #[test]
    fn closed_comment_returns_to_code() {
        assert_eq!(class_of(Css, "/* a */b{}", "b"), Some(Function));
    }
}
