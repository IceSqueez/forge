use std::mem;
use std::sync::Arc;

use crate::highlight::{HighlightSpan, Language, LineState, highlight_line};

pub(crate) struct CodeLine<P> {
    text: Arc<str>,
    start: usize,
    chars: usize,
    start_state: LineState,
    end_state: LineState,
    spans: Vec<HighlightSpan>,
    pub(crate) payload: P,
}

impl<P> CodeLine<P> {
    pub(crate) fn text(&self) -> &Arc<str> {
        &self.text
    }

    pub(crate) fn start(&self) -> usize {
        self.start
    }

    pub(crate) fn end(&self) -> usize {
        self.start + self.text.len()
    }

    pub(crate) fn chars(&self) -> usize {
        self.chars
    }

    pub(crate) fn spans(&self) -> &[HighlightSpan] {
        &self.spans
    }
}

pub(crate) struct CodeLines<P> {
    language: Language,
    source: Arc<str>,
    lines: Vec<CodeLine<P>>,
}

impl<P: Default> CodeLines<P> {
    pub(crate) fn new(language: Language) -> Self {
        let mut lines = Self {
            language,
            source: Arc::from(""),
            lines: Vec::new(),
        };
        lines.relex_all();
        lines
    }

    pub(crate) fn language(&self) -> Language {
        self.language
    }

    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    pub(crate) fn lines(&self) -> &[CodeLine<P>] {
        &self.lines
    }

    pub(crate) fn line(&self, index: usize) -> Option<&CodeLine<P>> {
        self.lines.get(index)
    }

    pub(crate) fn line_mut(&mut self, index: usize) -> Option<&mut CodeLine<P>> {
        self.lines.get_mut(index)
    }

    pub(crate) fn line_index_at(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|line| line.start <= offset)
            .saturating_sub(1)
    }

    pub(crate) fn reset_payloads(&mut self) {
        for line in &mut self.lines {
            line.payload = P::default();
        }
    }

    pub(crate) fn set_language(&mut self, language: Language) -> bool {
        if self.language == language {
            return false;
        }
        self.language = language;
        self.relex_all();
        true
    }

    pub(crate) fn sync(&mut self, text: &Arc<str>) -> bool {
        if Arc::ptr_eq(&self.source, text) {
            return false;
        }
        self.rebuild(text);
        true
    }

    fn rebuild(&mut self, text: &Arc<str>) {
        let new_texts: Vec<&str> = text.split('\n').collect();
        let old = mem::take(&mut self.lines);
        let old_len = old.len();
        let new_len = new_texts.len();
        let shared = old_len.min(new_len);
        let prefix = old
            .iter()
            .zip(&new_texts)
            .take_while(|(line, new)| &*line.text == **new)
            .count();
        let suffix = old
            .iter()
            .rev()
            .zip(new_texts.iter().rev())
            .take(shared - prefix)
            .take_while(|(line, new)| &*line.text == **new)
            .count();
        let relex_from = if self.language == Language::Json {
            0
        } else {
            prefix.min(old_len.saturating_sub(1))
        };
        let suffix_from = new_len - suffix;
        let old_suffix_from = old_len - suffix;

        let mut slots: Vec<Option<CodeLine<P>>> = old.into_iter().map(Some).collect();
        let mut lines: Vec<CodeLine<P>> = Vec::with_capacity(new_len);
        lines.extend(slots[..relex_from].iter_mut().filter_map(Option::take));
        let mut state = lines.last().map_or_else(
            || LineState::new(self.language),
            |line| line.end_state.clone(),
        );
        let mut start = lines.last().map_or(0, |line| line.end() + 1);

        for (index, new_text) in new_texts.iter().enumerate().skip(relex_from) {
            let counterpart = if index >= suffix_from {
                Some(index - suffix_from + old_suffix_from)
            } else if index < prefix {
                Some(index)
            } else {
                None
            };
            if index >= suffix_from
                && let Some(old_index) = counterpart
                && slots[old_index]
                    .as_ref()
                    .is_some_and(|line| line.start_state == state)
            {
                for mut line in slots[old_index..].iter_mut().filter_map(Option::take) {
                    line.start = start;
                    start = line.end() + 1;
                    lines.push(line);
                }
                break;
            }

            let has_newline = index + 1 < new_len;
            let range = start..start + new_text.len() + usize::from(has_newline);
            let (spans, next_state) = highlight_line(text, range, &state);
            let spans = trimmed(spans, new_text.len());
            let (line_text, chars, payload) = match counterpart.and_then(|ix| slots[ix].take()) {
                Some(old_line) if old_line.spans == spans => {
                    (old_line.text, old_line.chars, old_line.payload)
                }
                Some(old_line) => (old_line.text, old_line.chars, P::default()),
                None => (Arc::from(*new_text), new_text.chars().count(), P::default()),
            };
            lines.push(CodeLine {
                text: line_text,
                start,
                chars,
                start_state: state,
                end_state: next_state.clone(),
                spans,
                payload,
            });
            state = next_state;
            start += new_text.len() + 1;
        }

        self.lines = lines;
        self.source = Arc::clone(text);
    }

    fn relex_all(&mut self) {
        let source = Arc::clone(&self.source);
        self.lines.clear();
        self.rebuild(&source);
    }
}

fn trimmed(spans: Vec<HighlightSpan>, len: usize) -> Vec<HighlightSpan> {
    let mut left = len;
    let mut out = Vec::with_capacity(spans.len());
    for span in spans {
        if left == 0 {
            break;
        }
        let take = span.len.min(left);
        out.push(HighlightSpan {
            len: take,
            class: span.class,
        });
        left -= take;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::{TokenClass, highlight};

    const ALL_LANGUAGES: [Language; 5] = [
        Language::Html,
        Language::Css,
        Language::JavaScript,
        Language::Json,
        Language::Rhai,
    ];

    fn synced(language: Language, text: &str) -> CodeLines<u32> {
        let mut lines = CodeLines::new(language);
        lines.sync(&Arc::from(text));
        lines
    }

    fn mark_all(lines: &mut CodeLines<u32>) {
        for index in 0..lines.len() {
            if let Some(line) = lines.line_mut(index) {
                line.payload = u32::try_from(index).unwrap_or(u32::MAX) + 1;
            }
        }
    }

    fn payloads(lines: &CodeLines<u32>) -> Vec<u32> {
        lines.lines().iter().map(|line| line.payload).collect()
    }

    fn expand(spans: &[HighlightSpan]) -> Vec<TokenClass> {
        spans
            .iter()
            .flat_map(|span| std::iter::repeat_n(span.class, span.len))
            .collect()
    }

    fn mismatch(lines: &CodeLines<u32>, text: &str) -> Option<String> {
        let fresh = expand(&highlight(lines.language(), text));
        let mut start = 0;
        let texts: Vec<&str> = text.split('\n').collect();
        if lines.len() != texts.len() {
            return Some(format!("{} lines, expected {}", lines.len(), texts.len()));
        }
        for (index, (line, expected)) in lines.lines().iter().zip(&texts).enumerate() {
            if &**line.text() != *expected || line.start() != start {
                return Some(format!("line {index} text or start differs"));
            }
            if line.chars() != expected.chars().count() {
                return Some(format!("line {index} char count differs"));
            }
            let classes = expand(line.spans());
            if classes.as_slice() != &fresh[start..start + expected.len()] {
                return Some(format!("line {index} {expected:?} classes differ"));
            }
            start += expected.len() + 1;
        }
        None
    }

    fn assert_edit_matches_fresh(language: Language, before: &str, after: &str) {
        let mut lines = synced(language, before);
        lines.sync(&Arc::from(after));
        assert_eq!(
            mismatch(&lines, after),
            None,
            "{language:?} {before:?} -> {after:?}"
        );
    }

    #[test]
    fn edits_that_flip_lexer_state_across_the_unchanged_tail_match_a_fresh_highlight() {
        for (language, before, after) in [
            (
                Language::JavaScript,
                "let a = 1;\nlet b = 2;\nlet c = 3;",
                "/* let a = 1;\nlet b = 2;\nlet c = 3;",
            ),
            (
                Language::JavaScript,
                "/* let a = 1;\nlet b = 2;\nlet c = 3;",
                "let a = 1;\nlet b = 2;\nlet c = 3;",
            ),
            (
                Language::JavaScript,
                "/* a\nb\n*/ let c = 3;\nlet d = 4;",
                "/* a\nb */\n*/ let c = 3;\nlet d = 4;",
            ),
            (
                Language::JavaScript,
                "let a = x;\nlet b = y;\nlet c = z;",
                "let a = `x;\nlet b = y;\nlet c = z;",
            ),
            (
                Language::JavaScript,
                "f(1);\n// note\ng(2);",
                "f(1);\n// note\ng(2);\nh(3);",
            ),
            (
                Language::Css,
                "a { color: red; }\nb { margin: 0; }\nc { top: 1px; }",
                "a { color: red; }\n/* b { margin: 0; }\nc { top: 1px; }",
            ),
            (
                Language::Css,
                "a\nb\nc { top: 1px; }",
                "a {\nb\nc { top: 1px; }",
            ),
            (
                Language::Html,
                "<p>one</p>\n<p>two</p>\n<p>three</p>",
                "<!-- <p>one</p>\n<p>two</p>\n<p>three</p>",
            ),
            (
                Language::Html,
                "<div>\nlet a = 1;\n</div>\n<p class=\"x\">t</p>",
                "<script>\nlet a = 1;\n</script>\n<p class=\"x\">t</p>",
            ),
            (
                Language::Html,
                "<p>\na { color: red; }\n</p>",
                "<style>\na { color: red; }\n</p>",
            ),
            (
                Language::Html,
                "<p a=x>t</p>\n<b c=y>u</b>",
                "<p a=\"x>t</p>\n<b c=y>u</b>",
            ),
            (
                Language::Rhai,
                "let a = 1;\nlet b = 2;",
                "let a = \"1;\nlet b = 2;",
            ),
            (Language::JavaScript, "let a = `x", "let a = `x\nb"),
            (Language::JavaScript, "/* x", "/* x\nb"),
            (Language::Css, "a { b: \"x", "a { b: \"x\nc"),
            (Language::Css, "/* x", "/* x\nc"),
            (Language::Html, "<p a=\"x", "<p a=\"x\nb"),
            (
                Language::Html,
                "<script>\nlet a = `x",
                "<script>\nlet a = `x\nb",
            ),
            (Language::Json, "{\"a\"\n1}", "{\"a\"\n: 1}"),
            (
                Language::Json,
                "{\"a\": 1,\n\"b\": 2}",
                "{\"a\": \"1,\n\"b\": 2}",
            ),
            (Language::Json, "[\"a\"", "[\"a\"\n, 1]"),
        ] {
            assert_edit_matches_fresh(language, before, after);
        }
    }

    #[test]
    fn appending_a_line_after_a_string_left_open_at_document_end_continues_the_string() {
        for (language, before, after) in [
            (Language::Json, "[\n\"x", "[\n\"x\n1, 2"),
            (Language::Rhai, "let a = \"x", "let a = \"x\nlet b = 2;"),
        ] {
            assert_edit_matches_fresh(language, before, after);
        }
    }

    #[test]
    fn json_key_whose_colon_arrives_two_lines_later_matches_a_fresh_highlight() {
        assert_edit_matches_fresh(Language::Json, "{\"a\"\n\n1}", "{\"a\"\n\n: 1}");
    }

    #[test]
    fn structural_edits_match_a_fresh_highlight() {
        for language in ALL_LANGUAGES {
            for (before, after) in [
                ("", "a\nb"),
                ("a\nb", ""),
                ("a\nb\nc", "a\nx\nb\nc"),
                ("a\nx\nb\nc", "a\nb\nc"),
                ("a\nb\nc", "a\nb\nc\n"),
                ("a\nb\nc\n", "a\nb\nc"),
                ("a\na\na", "a\na\na\na"),
                ("a\r\nb\r\nc", "a\r\nb2\r\nc"),
                (
                    "\u{439}\u{43a}\n\u{1f600} x\nz",
                    "\u{439}\u{43a}\n\u{1f600}\u{1f600} x\nz",
                ),
                ("x\n\n\n", "\n\n\nx"),
            ] {
                assert_edit_matches_fresh(language, before, after);
            }
        }
    }

    #[test]
    fn random_edit_sequences_stay_equal_to_a_fresh_highlight() {
        let fragments = [
            "\n",
            "/*",
            "*/",
            "\"",
            "'",
            "`",
            "<",
            ">",
            "</script>",
            "<script>",
            "<style>",
            "</style>",
            "<!--",
            "-->",
            "{",
            "}",
            ":",
            ";",
            "//",
            "a",
            " ",
            "1",
            "\\",
            "=",
            "\u{439}",
            "\u{1f600}",
            "\t",
        ];
        for language in ALL_LANGUAGES {
            let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
            let mut next = |bound: usize| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                usize::try_from(seed >> 33).unwrap_or(0) % bound.max(1)
            };
            let mut text = String::from("a\nb\nc\nd\ne\nf");
            let mut lines = synced(language, &text);
            for step in 0..600 {
                let boundaries: Vec<usize> = (0..=text.len())
                    .filter(|offset| text.is_char_boundary(*offset))
                    .collect();
                let from = boundaries[next(boundaries.len())];
                let to = if next(3) == 0 {
                    let later: Vec<usize> = boundaries
                        .iter()
                        .copied()
                        .filter(|offset| *offset >= from && *offset <= from + 12)
                        .collect();
                    later[next(later.len())]
                } else {
                    from
                };
                let insert = if from != to && next(2) == 0 {
                    ""
                } else {
                    fragments[next(fragments.len())]
                };
                text.replace_range(from..to, insert);
                lines.sync(&Arc::from(text.as_str()));
                assert_eq!(
                    mismatch(&lines, &text),
                    None,
                    "{language:?} step {step} {text:?}"
                );
            }
        }
    }

    #[test]
    fn sync_with_the_same_source_reports_no_change() {
        let source: Arc<str> = Arc::from("a\nb");
        let mut lines: CodeLines<u32> = CodeLines::new(Language::JavaScript);
        assert!(lines.sync(&source));
        assert!(!lines.sync(&source));
    }

    #[test]
    fn editing_one_line_keeps_the_layout_of_every_other_line() {
        let mut lines = synced(
            Language::JavaScript,
            "let a = 1;\nlet b = 2;\nlet c = 3;\nlet d = 4;",
        );
        mark_all(&mut lines);
        lines.sync(&Arc::from(
            "let a = 1;\nlet b = 22;\nlet c = 3;\nlet d = 4;",
        ));
        assert_eq!(payloads(&lines), vec![1, 0, 3, 4]);
    }

    #[test]
    fn inserting_lines_keeps_the_layout_of_the_shifted_tail() {
        let mut lines = synced(Language::Css, "a {\ncolor: red;\n}\nb {}");
        mark_all(&mut lines);
        lines.sync(&Arc::from("a {\ncolor: red;\ntop: 0;\nleft: 0;\n}\nb {}"));
        assert_eq!(payloads(&lines), vec![1, 2, 0, 0, 3, 4]);
    }

    #[test]
    fn tail_lines_whose_colors_change_lose_their_layout_and_the_rest_keep_it() {
        let mut lines = synced(
            Language::JavaScript,
            "let a = 1;\nlet b = 2;\n*/\nlet c = 3;",
        );
        mark_all(&mut lines);
        lines.sync(&Arc::from("/* let a = 1;\nlet b = 2;\n*/\nlet c = 3;"));
        assert_eq!(payloads(&lines), vec![0, 0, 0, 4]);
    }

    #[test]
    fn json_lookback_line_keeps_its_layout_when_its_colors_do_not_change() {
        let mut lines = synced(Language::Json, "{\n\"a\": 1,\n\"b\": 2\n}");
        mark_all(&mut lines);
        lines.sync(&Arc::from("{\n\"a\": 1,\n\"b\": 3\n}"));
        assert_eq!(payloads(&lines), vec![1, 2, 0, 4]);
    }

    #[test]
    fn json_lookback_line_loses_its_layout_when_the_edit_recolors_it() {
        let mut lines = synced(Language::Json, "[\n\"a\"\n1]");
        mark_all(&mut lines);
        lines.sync(&Arc::from("[\n\"a\"\n: 1]"));
        assert_eq!(payloads(&lines), vec![1, 0, 0]);
    }

    #[test]
    fn identical_content_in_a_new_source_keeps_every_layout() {
        let mut lines = synced(Language::Html, "<p>\nx\n</p>");
        mark_all(&mut lines);
        lines.sync(&Arc::from("<p>\nx\n</p>"));
        assert_eq!(payloads(&lines), vec![1, 2, 3]);
    }

    #[test]
    fn switching_language_relexes_and_drops_every_layout() {
        let text = "a { color: red; }\n/* x */";
        let mut lines = synced(Language::JavaScript, text);
        mark_all(&mut lines);
        assert!(lines.set_language(Language::Css));
        assert_eq!(payloads(&lines), vec![0, 0]);
        assert_eq!(mismatch(&lines, text), None);
    }

    #[test]
    fn setting_the_current_language_is_a_no_op_that_keeps_layouts() {
        let mut lines = synced(Language::Css, "a {}\nb {}");
        mark_all(&mut lines);
        assert!(!lines.set_language(Language::Css));
        assert_eq!(payloads(&lines), vec![1, 2]);
    }

    #[test]
    fn line_index_at_maps_offsets_including_newlines_and_past_the_end() {
        let lines = synced(Language::JavaScript, "ab\n\ncd");
        for (offset, expected) in [(0, 0), (2, 0), (3, 1), (4, 2), (6, 2), (99, 2)] {
            assert_eq!(lines.line_index_at(offset), expected, "offset {offset}");
        }
    }

    #[test]
    fn empty_document_has_one_empty_line() {
        let lines: CodeLines<u32> = CodeLines::new(Language::Html);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines.line_index_at(0), 0);
    }
}
