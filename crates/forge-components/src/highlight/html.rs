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
