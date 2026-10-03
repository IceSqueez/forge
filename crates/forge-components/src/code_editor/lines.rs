use std::mem;
use std::sync::Arc;

use crate::highlight::{HighlightSpan, Language, LineState, highlight_line};

const JSON_LOOKBEHIND_LINES: usize = 1;

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
            prefix.saturating_sub(JSON_LOOKBEHIND_LINES)
        } else {
            prefix
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
