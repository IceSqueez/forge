use std::ops::Range;

use super::TokenClass;
use super::css::{self, CssState};
use super::cursor::{Cursor, SpanSink};
use super::js::{self, JsState};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RawElement {
    Style,
    Script,
    Title,
    Textarea,
}

impl RawElement {
    fn from_tag_name(name: &str) -> Option<Self> {
        [Self::Style, Self::Script, Self::Title, Self::Textarea]
            .into_iter()
            .find(|element| element.tag_name().eq_ignore_ascii_case(name))
    }

    fn tag_name(self) -> &'static str {
        match self {
            Self::Style => "style",
            Self::Script => "script",
            Self::Title => "title",
            Self::Textarea => "textarea",
        }
    }

    fn content(self) -> RawContent {
        match self {
            Self::Style => RawContent::Style(CssState::default()),
            Self::Script => RawContent::Script(JsState::default()),
            Self::Title | Self::Textarea => RawContent::Text(self),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum RawContent {
    Style(CssState),
    Script(JsState),
    Text(RawElement),
}

impl RawContent {
    fn element(&self) -> RawElement {
        match self {
            Self::Style(_) => RawElement::Style,
            Self::Script(_) => RawElement::Script,
            Self::Text(element) => *element,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TagPhase {
    BeforeAttribute,
    AfterAttributeName,
    BeforeValue,
    QuotedValue(char),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Tag {
    phase: TagPhase,
    opens: Option<RawElement>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
enum HtmlMode {
    #[default]
    Data,
    Comment,
    BogusComment,
    Doctype,
    Tag(Tag),
    Raw(RawContent),
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(super) struct HtmlState {
    mode: HtmlMode,
}

pub(super) fn lex(src: &str, range: Range<usize>, state: &mut HtmlState, out: &mut SpanSink) {
    let mut cur = Cursor::new(src, range);
    while !cur.is_done() {
        let start = cur.pos();
        match state.mode {
            HtmlMode::Data => data_token(&mut cur, state, out),
            HtmlMode::Comment => {
                if close_comment(&mut cur) {
                    state.mode = HtmlMode::Data;
                }
                out.push(cur.pos() - start, TokenClass::Comment);
            }
            HtmlMode::BogusComment => {
                if cur.eat_past('>') {
                    state.mode = HtmlMode::Data;
                }
                out.push(cur.pos() - start, TokenClass::Comment);
            }
            HtmlMode::Doctype => doctype_token(&mut cur, state, out),
            HtmlMode::Tag(tag) => tag_token(&mut cur, state, tag, out),
            HtmlMode::Raw(_) => {
                let closed = match &mut state.mode {
                    HtmlMode::Raw(content) => lex_raw_text_until_end_tag(&mut cur, content, out),
                    _ => false,
                };
                if closed {
                    open_tag(&mut cur, state, true, out);
                }
            }
        }
    }
}

fn close_comment(cur: &mut Cursor) -> bool {
    let rest = cur.rest();
    let close = ["-->", "--!>"]
        .iter()
        .filter_map(|pattern| rest.find(pattern).map(|at| at + pattern.len()))
        .min();
    match close {
        Some(end) => {
            cur.advance(end);
            true
        }
        None => {
            cur.jump_to(cur.end());
            false
        }
    }
}

fn data_token(cur: &mut Cursor, state: &mut HtmlState, out: &mut SpanSink) {
    let start = cur.pos();
    match cur.peek() {
        Some('<') => markup(cur, state, out),
        Some('&') => {
            cur.bump();
            let named = if cur.peek() == Some('#') {
                cur.bump();
                if matches!(cur.peek(), Some('x' | 'X')) {
                    cur.bump();
                    let digits = cur.pos();
                    cur.eat_while(|c| c.is_ascii_hexdigit());
                    cur.pos() > digits
                } else {
                    let digits = cur.pos();
                    cur.eat_while(|c| c.is_ascii_digit());
                    cur.pos() > digits
                }
            } else {
                let name = cur.pos();
                cur.eat_while(|c| c.is_ascii_alphanumeric());
                cur.pos() > name
            };
            if named && cur.peek() == Some(';') {
                cur.bump();
            }
            let class = if named {
                TokenClass::Variable
            } else {
                TokenClass::Plain
            };
            out.push(cur.pos() - start, class);
        }
        _ => {
            cur.bump();
            cur.eat_while(|c| c != '<' && c != '&');
            out.push(cur.pos() - start, TokenClass::Plain);
        }
    }
}

fn markup(cur: &mut Cursor, state: &mut HtmlState, out: &mut SpanSink) {
    let start = cur.pos();
    if cur.starts_with("<!--") {
        cur.advance("<!--".len());
        if cur.starts_with(">") {
            cur.advance(">".len());
        } else if cur.starts_with("->") {
            cur.advance("->".len());
        } else {
            state.mode = HtmlMode::Comment;
            if close_comment(cur) {
                state.mode = HtmlMode::Data;
            }
        }
        out.push(cur.pos() - start, TokenClass::Comment);
        return;
    }
    if cur.starts_with("<!") {
        let mut probe = *cur;
        probe.advance("<!".len());
        if probe.starts_with_ignore_case("doctype") {
            out.push(probe.pos() - start, TokenClass::Punctuation);
            let keyword = probe.pos();
            probe.advance("doctype".len());
            out.push(probe.pos() - keyword, TokenClass::Keyword);
            *cur = probe;
            state.mode = HtmlMode::Doctype;
            return;
        }
    }
    if cur.starts_with("<!") || cur.starts_with("<?") {
        state.mode = HtmlMode::BogusComment;
        cur.advance("<!".len());
        if cur.eat_past('>') {
            state.mode = HtmlMode::Data;
        }
        out.push(cur.pos() - start, TokenClass::Comment);
        return;
    }
    if cur.starts_with("</>") {
        cur.advance("</>".len());
        out.push(cur.pos() - start, TokenClass::Punctuation);
        return;
    }
    let closing = cur.starts_with("</");
    let name_at = if closing { "</".len() } else { "<".len() };
    if cur
        .peek_nth(name_at)
        .is_some_and(|c| c.is_ascii_alphabetic())
    {
        open_tag(cur, state, closing, out);
        return;
    }
    cur.bump();
    out.push(cur.pos() - start, TokenClass::Plain);
}

fn open_tag(cur: &mut Cursor, state: &mut HtmlState, closing: bool, out: &mut SpanSink) {
    let start = cur.pos();
    cur.advance(if closing { "</".len() } else { "<".len() });
    out.push(cur.pos() - start, TokenClass::Punctuation);
    let name_start = cur.pos();
    cur.eat_while(|c| !c.is_whitespace() && c != '/' && c != '>');
    out.push(cur.pos() - name_start, TokenClass::Keyword);
    let opens = if closing {
        None
    } else {
        RawElement::from_tag_name(cur.slice_from(name_start))
    };
    state.mode = HtmlMode::Tag(Tag {
        phase: TagPhase::BeforeAttribute,
        opens,
    });
}

fn doctype_token(cur: &mut Cursor, state: &mut HtmlState, out: &mut SpanSink) {
    let start = cur.pos();
    match cur.peek() {
        Some('>') => {
            cur.bump();
            state.mode = HtmlMode::Data;
            out.push(cur.pos() - start, TokenClass::Punctuation);
        }
        Some(quote @ ('"' | '\'')) => {
            cur.bump();
            cur.eat_while(|c| c != quote && c != '>');
            if cur.peek() == Some(quote) {
                cur.bump();
            }
            out.push(cur.pos() - start, TokenClass::String);
        }
        _ => {
            cur.bump();
            cur.eat_while(|c| c != '>' && c != '"' && c != '\'');
            out.push(cur.pos() - start, TokenClass::Plain);
        }
    }
}

fn tag_token(cur: &mut Cursor, state: &mut HtmlState, tag: Tag, out: &mut SpanSink) {
    let start = cur.pos();
    let Some(c) = cur.peek() else {
        return;
    };
    let mut next = tag;
    let class = match tag.phase {
        TagPhase::QuotedValue(quote) => {
            if cur.eat_past(quote) {
                next.phase = TagPhase::BeforeAttribute;
            }
            TokenClass::String
        }
        _ if c.is_whitespace() => {
            cur.eat_while(char::is_whitespace);
            TokenClass::Plain
        }
        _ if c == '>' => {
            cur.bump();
            out.push(cur.pos() - start, TokenClass::Punctuation);
            state.mode = match tag.opens {
                Some(element) => HtmlMode::Raw(element.content()),
                None => HtmlMode::Data,
            };
            return;
        }
        TagPhase::BeforeValue => {
            if c == '"' || c == '\'' {
                cur.bump();
                next.phase = TagPhase::QuotedValue(c);
                if cur.eat_past(c) {
                    next.phase = TagPhase::BeforeAttribute;
                }
            } else {
                cur.eat_while(|c| !c.is_whitespace() && c != '>');
                next.phase = TagPhase::BeforeAttribute;
            }
            TokenClass::String
        }
        TagPhase::AfterAttributeName if c == '=' => {
            cur.bump();
            next.phase = TagPhase::BeforeValue;
            TokenClass::Punctuation
        }
        _ if c == '/' => {
            cur.bump();
            next.phase = TagPhase::BeforeAttribute;
            TokenClass::Punctuation
        }
        TagPhase::BeforeAttribute | TagPhase::AfterAttributeName => {
            cur.bump();
            cur.eat_while(|c| !c.is_whitespace() && c != '/' && c != '>' && c != '=');
            next.phase = TagPhase::AfterAttributeName;
            TokenClass::Function
        }
    };
    state.mode = HtmlMode::Tag(next);
    out.push(cur.pos() - start, class);
}

fn lex_raw_text_until_end_tag(
    cur: &mut Cursor,
    content: &mut RawContent,
    out: &mut SpanSink,
) -> bool {
    let element = content.element();
    let close = find_end_tag(cur, element.tag_name());
    let segment_end = close.unwrap_or(cur.end());
    let segment = cur.pos()..segment_end;
    let src_end = segment.end;
    match content {
        RawContent::Style(css_state) => css::lex(src_of(cur, src_end), segment, css_state, out),
        RawContent::Script(js_state) => js::lex(src_of(cur, src_end), segment, js_state, out),
        RawContent::Text(_) => out.push(segment.end - segment.start, TokenClass::Plain),
    }
    cur.jump_to(segment_end);
    close.is_some()
}

fn src_of<'a>(cur: &Cursor<'a>, end: usize) -> &'a str {
    cur.full_src().get(..end).unwrap_or("")
}

fn find_end_tag(cur: &Cursor, name: &str) -> Option<usize> {
    let rest = cur.rest();
    rest.match_indices("</").find_map(|(at, _)| {
        let after = rest.get(at + "</".len()..)?;
        let head = after.as_bytes().get(..name.len())?;
        if !head.eq_ignore_ascii_case(name.as_bytes()) {
            return None;
        }
        let boundary = after.get(name.len()..)?.chars().next();
        let ends_name = boundary.is_none_or(|c| c.is_whitespace() || c == '/' || c == '>');
        ends_name.then_some(cur.pos() + at)
    })
}

#[cfg(test)]
mod tests {
    use crate::highlight::Language::Html;
    use crate::highlight::TokenClass::{
        Comment, Function, Keyword, Number, Plain, Punctuation, String, Variable,
    };
    use crate::highlight::tests::{assert_pieces, class_of, pieces};

    #[test]
    fn comment_spans_lines_until_its_close() {
        assert_eq!(
            pieces(Html, "<!-- a\n<b> -->x"),
            vec![("<!-- a\n<b> -->", Comment), ("x", Plain)]
        );
    }

    #[test]
    fn abruptly_closed_comments_end_immediately() {
        for text in ["<!-->x<b>", "<!--->x<b>"] {
            assert_eq!(class_of(Html, text, "x"), Some(Plain), "{text:?}");
            assert_eq!(class_of(Html, text, "b"), Some(Keyword), "{text:?}");
        }
    }

    #[test]
    fn comment_closes_at_bang_close_sequence() {
        assert_eq!(
            pieces(Html, "<!-- a --!>x"),
            vec![("<!-- a --!>", Comment), ("x", Plain)]
        );
    }

    #[test]
    fn unterminated_comment_colors_the_rest_of_the_document() {
        assert_eq!(pieces(Html, "<!-- a\n<b>"), vec![("<!-- a\n<b>", Comment)]);
    }

    #[test]
    fn doctype_is_case_insensitive_and_returns_to_data_after_close() {
        for (text, keyword) in [
            ("<!DOCTYPE html \"x\">t", "DOCTYPE"),
            ("<!doctype html \"x\">t", "doctype"),
        ] {
            assert_eq!(
                pieces(Html, text),
                vec![
                    ("<!", Punctuation),
                    (keyword, Keyword),
                    (" html ", Plain),
                    ("\"x\"", String),
                    (">", Punctuation),
                    ("t", Plain),
                ],
                "{text:?}"
            );
        }
    }

    #[test]
    fn bang_and_question_mark_markup_are_bogus_comments() {
        for text in ["<!x\ny>t", "<?xml v?>t"] {
            let bogus = &text[..text.len() - 1];
            assert_eq!(
                pieces(Html, text),
                vec![(bogus, Comment), ("t", Plain)],
                "{text:?}"
            );
        }
    }

    #[test]
    fn empty_end_tag_is_punctuation_and_leaves_data_state() {
        assert_eq!(
            pieces(Html, "</>x"),
            vec![("</>", Punctuation), ("x", Plain)]
        );
    }

    #[test]
    fn less_than_not_followed_by_a_letter_is_text() {
        assert_eq!(pieces(Html, "a < 1 <3"), vec![("a < 1 <3", Plain)]);
    }

    #[test]
    fn tag_name_attribute_name_and_values_are_classified() {
        assert_pieces(
            Html,
            "<a href=x title='y' data-k>t</a>",
            &[
                ("a", Keyword),
                ("href", Function),
                ("=", Punctuation),
                ("x", String),
                ("'y'", String),
                ("data-k", Function),
                ("t", Plain),
            ],
        );
    }

    #[test]
    fn quoted_attribute_value_continues_across_lines() {
        assert_pieces(
            Html,
            "<a title=\"x\ny > z\" id=k>t",
            &[("\"x\ny > z\"", String), ("id", Function), ("t", Plain)],
        );
    }

    #[test]
    fn character_references_are_variables_and_bare_ampersand_is_text() {
        assert_pieces(
            Html,
            "a&amp;b&#x1F;c&#38;d & e",
            &[
                ("&amp;", Variable),
                ("&#x1F;", Variable),
                ("&#38;", Variable),
            ],
        );
        assert_eq!(class_of(Html, "x & y", "x & y"), Some(Plain));
    }

    #[test]
    fn style_body_is_lexed_as_css() {
        assert_pieces(
            Html,
            "<style>\na { color: #fff }\n</style>",
            &[("a", Function), ("color", Keyword), ("#fff", Number)],
        );
    }

    #[test]
    fn script_body_is_lexed_as_javascript() {
        assert_pieces(
            Html,
            "<script>\nreturn f(1)\n</script>",
            &[("return", Keyword), ("f", Function), ("1", Number)],
        );
    }

    #[test]
    fn title_body_stays_plain_text() {
        assert_eq!(
            class_of(Html, "<title>return <b> 1</title>", "return <b> 1"),
            Some(Plain)
        );
    }

    #[test]
    fn script_end_tag_matches_in_any_case() {
        for text in ["<script>a</SCRIPT>return", "<script>a</Script >return"] {
            assert_eq!(class_of(Html, text, "return"), Some(Plain), "{text:?}");
        }
    }

    #[test]
    fn longer_tag_name_does_not_close_the_script() {
        assert_eq!(
            class_of(Html, "<script>a</scriptx>return</script>", "return"),
            Some(Keyword)
        );
    }

    #[test]
    fn markup_after_closed_style_is_html_again() {
        assert_pieces(
            Html,
            "<style>a{}</style><b c=d>",
            &[("b", Keyword), ("c", Function), ("d", String)],
        );
    }

    #[test]
    fn unterminated_tag_at_end_of_document_keeps_coloring_attributes() {
        assert_eq!(
            pieces(Html, "<a b=\"x"),
            vec![
                ("<", Punctuation),
                ("a", Keyword),
                (" ", Plain),
                ("b", Function),
                ("=", Punctuation),
                ("\"x", String)
            ]
        );
    }

    #[test]
    fn self_closing_slash_is_punctuation() {
        assert_eq!(class_of(Html, "<img a/>", "/>"), Some(Punctuation));
        assert_eq!(class_of(Html, "<img a/>", "a/"), None);
    }

    #[test]
    fn comment_inside_tag_text_body_is_not_a_comment_token() {
        assert_eq!(
            class_of(Html, "<title><!-- x --></title>", "<!-- x -->"),
            Some(Plain)
        );
    }
}
