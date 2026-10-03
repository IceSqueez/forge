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

#[cfg(test)]
mod tests {
    use super::*;

    const LANGUAGES: [Language; 5] = [
        Language::Html,
        Language::Css,
        Language::JavaScript,
        Language::Json,
        Language::Rhai,
    ];

    pub(super) fn pieces(language: Language, text: &str) -> Vec<(&str, TokenClass)> {
        let mut at = 0;
        highlight(language, text)
            .into_iter()
            .map(|span| {
                let piece = &text[at..at + span.len];
                at += span.len;
                (piece, span.class)
            })
            .collect()
    }

    pub(super) fn assert_pieces(language: Language, text: &str, expected: &[(&str, TokenClass)]) {
        let actual = pieces(language, text);
        for piece in expected {
            assert!(
                actual.contains(piece),
                "{language:?} {text:?}: missing {piece:?} in {actual:?}"
            );
        }
    }

    pub(super) fn class_of(language: Language, text: &str, needle: &str) -> Option<TokenClass> {
        let at = text.find(needle)?;
        let classes = byte_classes(&highlight(language, text));
        let covered = classes.get(at..at + needle.len())?;
        let first = *covered.first()?;
        covered.iter().all(|class| *class == first).then_some(first)
    }

    fn byte_classes(spans: &[HighlightSpan]) -> Vec<TokenClass> {
        spans
            .iter()
            .flat_map(|span| std::iter::repeat_n(span.class, span.len))
            .collect()
    }

    fn incremental_byte_classes(language: Language, text: &str) -> Vec<TokenClass> {
        let mut state = LineState::new(language);
        let mut classes = Vec::new();
        for line in line_ranges(text) {
            let line_len = line.len();
            let (spans, next) = highlight_line(text, line, &state);
            let line_classes = byte_classes(&spans);
            assert_eq!(line_classes.len(), line_len, "{language:?} {text:?}");
            classes.extend(line_classes);
            state = next;
        }
        classes
    }

    fn assert_lexer_contract(language: Language, text: &str) {
        let spans = highlight(language, text);
        let covered: usize = spans.iter().map(|span| span.len).sum();
        assert_eq!(covered, text.len(), "{language:?} {text:?}: coverage");
        let mut at = 0;
        for span in &spans {
            assert!(span.len > 0, "{language:?} {text:?}: empty span");
            at += span.len;
            assert!(
                text.is_char_boundary(at),
                "{language:?} {text:?}: split char"
            );
        }
        assert!(
            spans.windows(2).all(|pair| pair[0].class != pair[1].class),
            "{language:?} {text:?}: adjacent equal classes not merged"
        );
        assert_eq!(
            incremental_byte_classes(language, text),
            byte_classes(&spans),
            "{language:?} {text:?}: incremental differs from whole"
        );
    }

    #[test]
    fn spans_cover_edge_inputs_exactly_in_every_language() {
        let inputs = [
            "",
            "\n",
            "\n\n",
            "\r\n",
            "é",
            "😀",
            "é\"😀\n'é`/*😀",
            "<é a=\"😀\">&é;</é>",
            "{\"é\": \"😀\\\n\", \"k\": [1, -2e+3, null]}",
        ];
        for language in LANGUAGES {
            for text in inputs {
                assert_lexer_contract(language, text);
            }
        }
    }

    #[test]
    fn line_by_line_lexing_with_carried_state_matches_whole_document() {
        let samples = [
            (
                Language::Html,
                "<!DOCTYPE html>\n<!-- a\nb -->\n<a title=\"x\ny\" href=z>t &amp; u</a>\n<style>\na {\n  color: \"x\\\ny\";\n}\n</style>\n<script>\nlet s = `a${ {b:1} }\nc`;\n/* d\ne */ x = /re/g;\n</SCRIPT>\n<p",
            ),
            (
                Language::Css,
                "@media screen {\n  a:hover {\n    --x: 1;\n    content: \"a\\\nb\";\n    color: var(--x) !important;\n  }\n}\n/* open\ncomment */\nb { top: -5px }\n/* tail",
            ),
            (
                Language::JavaScript,
                "const t = `a${`b${c}`}\nd`;\nlet s = 'x\\\ny';\n/* a\nb */\nreturn /[/]/g.test(s) / 2;\n`open\n${ {k: 1} }\ntail",
            ),
            (
                Language::Json,
                "{\n  \"a\"\n  : \"b\",\n  \"c\": [1, -1e+5, nullx],\n  \"d\": \"open\n}",
            ),
            (
                Language::Rhai,
                "fn add (a, b) {\n  // sum\n  let s = \"x\\\ny\";\n  return a + b;\n}\nlet t = \"open\nstill",
            ),
        ];
        for (language, text) in samples {
            assert_lexer_contract(language, text);
        }
    }

    fn multiline_js_comment_state() -> (&'static str, LineState) {
        let text = "/* é\nb */";
        let (_, state) = highlight_line(text, 0..5, &LineState::new(Language::JavaScript));
        (text, state)
    }

    #[test]
    fn highlight_line_on_mid_char_range_returns_one_plain_span_and_keeps_state() {
        let (text, state) = multiline_js_comment_state();
        for range in [4..5, 3..4, 1..4] {
            let (spans, next) = highlight_line(text, range.clone(), &state);
            assert_eq!(
                spans,
                vec![HighlightSpan {
                    len: range.len(),
                    class: TokenClass::Plain
                }],
                "{range:?}"
            );
            assert_eq!(next, state, "{range:?}");
        }
    }

    #[test]
    fn highlight_line_on_empty_reversed_or_out_of_range_returns_nothing_and_keeps_state() {
        let (text, state) = multiline_js_comment_state();
        let ranges =
            [(5, 2), (4, 3), (50, 90), (9, 9), (90, 9)].map(|(start, end)| Range { start, end });
        for range in ranges {
            let (spans, next) = highlight_line(text, range.clone(), &state);
            assert!(spans.is_empty(), "{range:?}: {spans:?}");
            assert_eq!(next, state, "{range:?}");
        }
    }

    #[test]
    fn highlight_line_clamps_end_past_text_to_the_text_end() {
        let (text, state) = multiline_js_comment_state();
        let (spans, next) = highlight_line(text, 5..400, &state);
        assert_eq!(
            spans,
            vec![HighlightSpan {
                len: text.len() - 5,
                class: TokenClass::Comment
            }]
        );
        assert_eq!(next, LineState::new(Language::JavaScript));
    }

    #[test]
    fn line_ranges_split_after_each_newline_and_keep_a_trailing_partial_line() {
        let ranges: Vec<_> = line_ranges("ab\n\ncd").collect();
        assert_eq!(ranges, vec![0..3, 3..4, 4..6]);
        assert_eq!(line_ranges("").count(), 0);
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 as usize
        }
    }

    fn fragments(language: Language) -> &'static [&'static str] {
        match language {
            Language::Html => &[
                "<",
                ">",
                "</",
                "<!--",
                "-->",
                "--!>",
                "<!",
                "<?",
                "<!DOCTYPE",
                "\"",
                "'",
                "=",
                " ",
                "\n",
                "&",
                "&amp;",
                "&#x",
                "#",
                ";",
                "a",
                "div",
                "style",
                "script",
                "title",
                "</style>",
                "</script>",
                "</SCRIPT",
                "{",
                "}",
                ":",
                "/",
                "/*",
                "*/",
                "`",
                "${",
                "é",
                "😀",
                "\\",
            ],
            Language::Css => &[
                "@media",
                "@import",
                "{",
                "}",
                ":",
                ";",
                "a",
                ".b",
                "#f",
                "--x",
                "var(",
                "url(",
                ")",
                "\"",
                "'",
                "\\",
                "\n",
                " ",
                "/*",
                "*/",
                "-",
                "5",
                ".5",
                "e3",
                "%",
                "!important",
                "&",
                "é",
                "😀",
                "+",
                "@",
                "\\\n",
            ],
            Language::JavaScript => &[
                "`", "${", "}", "{", "/", "/*", "*/", "//", "\"", "'", "\\", "\n", " ", "return",
                "x", "=", "(", ")", "[", "]", "1", "0x", "n", ".", "e", "-", "é", "😀",
            ],
            Language::Json => &[
                "{", "}", "[", "]", "\"", "\\", ":", ",", " ", "\n", "true", "null", "nullx", "-",
                "1", "e", "+", ".", "é", "😀", "k",
            ],
            Language::Rhai => &[
                "fn", "let", "//", "\"", "\\", "\n", " ", "(", ")", "x", "1", ".", "_", "e", "é",
                "😀", "{", "}", ";",
            ],
        }
    }

    #[test]
    fn generated_inputs_hold_coverage_and_incremental_contract_in_every_language() {
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        for language in LANGUAGES {
            let alphabet = fragments(language);
            for _ in 0..300 {
                let count = rng.next() % 40;
                let text: String = (0..count)
                    .map(|_| alphabet[rng.next() % alphabet.len()])
                    .collect();
                assert_lexer_contract(language, &text);
            }
        }
    }
}
