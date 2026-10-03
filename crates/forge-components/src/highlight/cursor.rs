use std::ops::Range;

use super::{HighlightSpan, TokenClass};

#[derive(Default)]
pub(super) struct SpanSink {
    spans: Vec<HighlightSpan>,
}

impl SpanSink {
    pub(super) fn push(&mut self, len: usize, class: TokenClass) {
        if len == 0 {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.class == class
        {
            last.len += len;
            return;
        }
        self.spans.push(HighlightSpan { len, class });
    }

    pub(super) fn into_spans(self) -> Vec<HighlightSpan> {
        self.spans
    }
}

#[derive(Clone, Copy)]
pub(super) struct Cursor<'a> {
    src: &'a str,
    pos: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(src: &'a str, range: Range<usize>) -> Self {
        Self {
            src,
            pos: range.start,
            end: range.end,
        }
    }

    pub(super) fn pos(&self) -> usize {
        self.pos
    }

    pub(super) fn end(&self) -> usize {
        self.end
    }

    pub(super) fn is_done(&self) -> bool {
        self.pos >= self.end
    }

    pub(super) fn rest(&self) -> &'a str {
        self.src.get(self.pos..self.end).unwrap_or("")
    }

    pub(super) fn slice_from(&self, start: usize) -> &'a str {
        self.src.get(start..self.pos).unwrap_or("")
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub(super) fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    pub(super) fn eat_while(&mut self, mut accept: impl FnMut(char) -> bool) {
        while let Some(c) = self.peek() {
            if !accept(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    pub(super) fn eat_until(&mut self, pattern: &str) -> bool {
        match self.rest().find(pattern) {
            Some(offset) => {
                self.pos += offset + pattern.len();
                true
            }
            None => {
                self.pos = self.end;
                false
            }
        }
    }

    pub(super) fn eat_past(&mut self, delimiter: char) -> bool {
        match self.rest().find(delimiter) {
            Some(offset) => {
                self.pos += offset + delimiter.len_utf8();
                true
            }
            None => {
                self.pos = self.end;
                false
            }
        }
    }

    pub(super) fn full_src(&self) -> &'a str {
        self.src
    }

    pub(super) fn starts_with(&self, pattern: &str) -> bool {
        self.rest().starts_with(pattern)
    }

    pub(super) fn starts_with_ignore_case(&self, pattern: &str) -> bool {
        self.rest()
            .as_bytes()
            .get(..pattern.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(pattern.as_bytes()))
    }

    pub(super) fn advance(&mut self, bytes: usize) {
        let target = (self.pos + bytes).min(self.end);
        if self.src.is_char_boundary(target) {
            self.pos = target;
        }
    }

    pub(super) fn jump_to(&mut self, pos: usize) {
        let target = pos.clamp(self.pos, self.end);
        if self.src.is_char_boundary(target) {
            self.pos = target;
        }
    }

    pub(super) fn next_non_space(&self) -> Option<char> {
        self.rest().chars().find(|c| *c != ' ' && *c != '\t')
    }
}
