mod css;
mod cursor;
mod html;
mod js;
mod json;
mod rhai;

use std::ops::Range;

use cursor::SpanSink;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Language {
    Html,
    Css,
    JavaScript,
    Json,
    Rhai,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenClass {
    Keyword,
    Function,
    String,
    Number,
    Comment,
    Variable,
    Plain,
    Punctuation,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HighlightSpan {
    pub len: usize,
    pub class: TokenClass,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LineState(LexerState);

#[derive(Clone, PartialEq, Eq, Debug)]
enum LexerState {
    Html(html::HtmlState),
    Css(css::CssState),
    JavaScript(js::JsState),
    Json(json::JsonState),
    Rhai(rhai::RhaiState),
}

impl LineState {
    pub fn new(language: Language) -> Self {
        Self(match language {
            Language::Html => LexerState::Html(html::HtmlState::default()),
            Language::Css => LexerState::Css(css::CssState::default()),
            Language::JavaScript => LexerState::JavaScript(js::JsState::default()),
            Language::Json => LexerState::Json(json::JsonState::default()),
            Language::Rhai => LexerState::Rhai(rhai::RhaiState::default()),
        })
    }

    pub fn language(&self) -> Language {
        match self.0 {
            LexerState::Html(_) => Language::Html,
            LexerState::Css(_) => Language::Css,
            LexerState::JavaScript(_) => Language::JavaScript,
            LexerState::Json(_) => Language::Json,
            LexerState::Rhai(_) => Language::Rhai,
        }
    }
}

pub fn line_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = 0;
    text.split_inclusive('\n').map(move |line| {
        let range = start..start + line.len();
        start = range.end;
        range
    })
}

pub fn highlight_line(
    text: &str,
    line: Range<usize>,
    state: &LineState,
) -> (Vec<HighlightSpan>, LineState) {
    let end = line.end.min(text.len());
    let start = line.start.min(end);
    let mut sink = SpanSink::default();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        sink.push(end - start, TokenClass::Plain);
        return (sink.into_spans(), state.clone());
    }
    let mut next = state.clone();
    lex_range(text, start..end, &mut next, &mut sink);
    (sink.into_spans(), next)
}

pub fn highlight(language: Language, text: &str) -> Vec<HighlightSpan> {
    let mut state = LineState::new(language);
    let mut sink = SpanSink::default();
    for line in line_ranges(text) {
        lex_range(text, line, &mut state, &mut sink);
    }
    sink.into_spans()
}

fn lex_range(text: &str, range: Range<usize>, state: &mut LineState, sink: &mut SpanSink) {
    match &mut state.0 {
        LexerState::Html(html_state) => html::lex(text, range, html_state, sink),
        LexerState::Css(css_state) => css::lex(text, range, css_state, sink),
        LexerState::JavaScript(js_state) => js::lex(text, range, js_state, sink),
        LexerState::Json(json_state) => json::lex(text, range, json_state, sink),
        LexerState::Rhai(rhai_state) => rhai::lex(text, range, rhai_state, sink),
    }
}
