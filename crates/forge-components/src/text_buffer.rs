use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;

use crate::text_edit::{
    next_grapheme_boundary, offset_from_utf16, previous_grapheme_boundary, range_from_utf16,
};

const HISTORY_LIMIT: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EditKind {
    Typing,
    DeleteBackward,
    DeleteForward,
    Standalone,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Selection {
    pub(crate) range: Range<usize>,
    pub(crate) reversed: bool,
}

#[derive(Debug)]
struct Change {
    start: usize,
    removed: String,
    inserted: String,
    kind: EditKind,
    selection_before: Selection,
    selection_after: Selection,
}

impl Change {
    fn inserted_end(&self) -> usize {
        self.start + self.inserted.len()
    }

    fn continues_with(&self, kind: EditKind, range: &Range<usize>, text: &str) -> bool {
        if self.kind != kind {
            return false;
        }
        match kind {
            EditKind::Typing => {
                range.start >= self.start
                    && range.end <= self.inserted_end()
                    && !starts_new_word(&self.inserted, text)
            }
            EditKind::DeleteBackward => text.is_empty() && range.end == self.start,
            EditKind::DeleteForward => text.is_empty() && range.start == self.start,
            EditKind::Standalone => false,
        }
    }

    fn absorb(&mut self, document: &str, range: &Range<usize>, text: &str) {
        match self.kind {
            EditKind::DeleteBackward => {
                self.removed.insert_str(0, &document[range.clone()]);
                self.start = range.start;
            }
            EditKind::DeleteForward => {
                self.removed.push_str(&document[range.clone()]);
            }
            EditKind::Typing | EditKind::Standalone => {
                let local = range.start - self.start..range.end - self.start;
                self.inserted.replace_range(local, text);
            }
        }
    }
}

fn starts_new_word(previous: &str, next: &str) -> bool {
    let previous_ends_in_word = previous
        .chars()
        .next_back()
        .is_some_and(|ch| !ch.is_whitespace());
    let next_starts_with_space = next.chars().next().is_some_and(char::is_whitespace);
    previous_ends_in_word && next_starts_with_space
}

fn spliced(text: &str, range: Range<usize>, replacement: &str) -> Arc<str> {
    let mut out = String::with_capacity(text.len() - range.len() + replacement.len());
    out.push_str(&text[..range.start]);
    out.push_str(replacement);
    out.push_str(&text[range.end..]);
    out.into()
}

#[derive(Debug)]
pub(crate) struct TextBuffer {
    text: Arc<str>,
    selection: Selection,
    marked: Option<Range<usize>>,
    undo_stack: VecDeque<Change>,
    redo_stack: Vec<Change>,
    group_open: bool,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            text: Arc::from(""),
            selection: Selection::default(),
            marked: None,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            group_open: false,
        }
    }
}

impl TextBuffer {
    pub(crate) fn text(&self) -> &Arc<str> {
        &self.text
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn selected_range(&self) -> &Range<usize> {
        &self.selection.range
    }

    pub(crate) fn is_reversed(&self) -> bool {
        self.selection.reversed
    }

    pub(crate) fn marked_range(&self) -> Option<&Range<usize>> {
        self.marked.as_ref()
    }

    pub(crate) fn cursor(&self) -> usize {
        if self.selection.reversed {
            self.selection.range.start
        } else {
            self.selection.range.end
        }
    }

    pub(crate) fn selected_text(&self) -> Option<&str> {
        let range = self.selection.range.clone();
        (!range.is_empty()).then(|| &self.text[range])
    }

    pub(crate) fn reset(&mut self, text: Arc<str>) {
        let end = text.len();
        self.text = text;
        self.selection = Selection {
            range: end..end,
            reversed: false,
        };
        self.marked = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.group_open = false;
    }

    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }

    pub(crate) fn move_to(&mut self, offset: usize) {
        self.selection.range = offset..offset;
        self.group_open = false;
    }

    pub(crate) fn select_to(&mut self, offset: usize) {
        if self.selection.reversed {
            self.selection.range.start = offset;
        } else {
            self.selection.range.end = offset;
        }
        if self.selection.range.end < self.selection.range.start {
            self.selection.reversed = !self.selection.reversed;
            self.selection.range = self.selection.range.end..self.selection.range.start;
        }
        self.group_open = false;
    }

    pub(crate) fn move_left(&mut self) {
        if self.selection.range.is_empty() {
            self.move_to(previous_grapheme_boundary(&self.text, self.cursor()));
        } else {
            self.move_to(self.selection.range.start);
        }
    }

    pub(crate) fn move_right(&mut self) {
        if self.selection.range.is_empty() {
            self.move_to(next_grapheme_boundary(&self.text, self.selection.range.end));
        } else {
            self.move_to(self.selection.range.end);
        }
    }

    pub(crate) fn select_left(&mut self) {
        self.select_to(previous_grapheme_boundary(&self.text, self.cursor()));
    }

    pub(crate) fn select_right(&mut self) {
        self.select_to(next_grapheme_boundary(&self.text, self.cursor()));
    }

    pub(crate) fn select_all(&mut self) {
        self.move_to(0);
        self.select_to(self.text.len());
    }

    pub(crate) fn cursor_line_bounds(&self) -> Range<usize> {
        let cursor = self.cursor();
        let start = self.text[..cursor].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| cursor + i);
        start..end
    }

    pub(crate) fn move_to_line_start(&mut self) {
        self.move_to(self.cursor_line_bounds().start);
    }

    pub(crate) fn move_to_line_end(&mut self) {
        self.move_to(self.cursor_line_bounds().end);
    }

    pub(crate) fn insert(&mut self, text: &str, kind: EditKind) {
        let range = self.target_range(None);
        self.apply(range, text, kind);
        self.marked = None;
    }

    pub(crate) fn insert_newline_keeping_indent(&mut self) {
        let at = self.target_range(None).start;
        let line_start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let before_caret = &self.text[line_start..at];
        let indent_len = before_caret.len() - before_caret.trim_start_matches([' ', '\t']).len();
        let newline = format!("\n{}", &before_caret[..indent_len]);
        self.insert(&newline, EditKind::Standalone);
    }

    pub(crate) fn delete_backward(&mut self) {
        self.delete_toward(EditKind::DeleteBackward);
    }

    pub(crate) fn delete_forward(&mut self) {
        self.delete_toward(EditKind::DeleteForward);
    }

    pub(crate) fn replace_in_utf16_range(&mut self, range_utf16: Option<Range<usize>>, text: &str) {
        let range = self.target_range(range_utf16);
        self.apply(range, text, EditKind::Typing);
        self.marked = None;
    }

    pub(crate) fn replace_and_mark_in_utf16_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16_in_text: Option<Range<usize>>,
    ) {
        let range = self.target_range(range_utf16);
        let start = range.start;
        let recorded = self.apply(range, text, EditKind::Typing);
        self.marked = (!text.is_empty()).then(|| start..start + text.len());
        if let Some(local) = selected_utf16_in_text {
            let local_start = offset_from_utf16(text, local.start);
            let local_end = offset_from_utf16(text, local.end).max(local_start);
            self.selection = Selection {
                range: start + local_start..start + local_end,
                reversed: false,
            };
            if recorded {
                self.set_last_selection_after();
            }
        }
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(change) = self.undo_stack.pop_back() else {
            return false;
        };
        let range = change.start..change.inserted_end();
        self.text = spliced(&self.text, range, &change.removed);
        self.selection = change.selection_before.clone();
        self.marked = None;
        self.group_open = false;
        self.redo_stack.push(change);
        true
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(change) = self.redo_stack.pop() else {
            return false;
        };
        let range = change.start..change.start + change.removed.len();
        self.text = spliced(&self.text, range, &change.inserted);
        self.selection = change.selection_after.clone();
        self.marked = None;
        self.group_open = false;
        self.undo_stack.push_back(change);
        true
    }

    fn delete_toward(&mut self, kind: EditKind) {
        if self.selection.range.is_empty() {
            let cursor = self.cursor();
            let target = if kind == EditKind::DeleteBackward {
                previous_grapheme_boundary(&self.text, cursor)
            } else {
                next_grapheme_boundary(&self.text, cursor)
            };
            let extended = cursor.min(target)..cursor.max(target);
            let range = self.marked.clone().unwrap_or(extended);
            self.apply(range, "", kind);
        } else {
            let range = self.target_range(None);
            self.apply(range, "", EditKind::Standalone);
        }
        self.marked = None;
    }

    fn target_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        let range = range_utf16
            .map(|range_utf16| range_from_utf16(&self.text, &range_utf16))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.range.clone());
        let len = self.text.len();
        let end = range.end.min(len);
        range.start.min(end)..end
    }

    fn apply(&mut self, range: Range<usize>, text: &str, kind: EditKind) -> bool {
        let selection_before = self.selection.clone();
        let caret = range.start + text.len();
        let selection_after = Selection {
            range: caret..caret,
            reversed: false,
        };
        let changes_text = !(range.is_empty() && text.is_empty());
        if changes_text {
            self.record(
                range.clone(),
                text,
                kind,
                selection_before,
                &selection_after,
            );
            self.text = spliced(&self.text, range, text);
        }
        self.selection = selection_after;
        changes_text
    }

    fn record(
        &mut self,
        range: Range<usize>,
        text: &str,
        kind: EditKind,
        selection_before: Selection,
        selection_after: &Selection,
    ) {
        self.redo_stack.clear();
        let merged = match self.undo_stack.back_mut() {
            Some(last) if self.group_open && last.continues_with(kind, &range, text) => {
                last.absorb(&self.text, &range, text);
                last.selection_after = selection_after.clone();
                true
            }
            _ => false,
        };
        if !merged {
            self.undo_stack.push_back(Change {
                start: range.start,
                removed: self.text[range].to_string(),
                inserted: text.to_string(),
                kind,
                selection_before,
                selection_after: selection_after.clone(),
            });
            if self.undo_stack.len() > HISTORY_LIMIT {
                self.undo_stack.pop_front();
            }
        }
        self.group_open = kind != EditKind::Standalone;
    }

    fn set_last_selection_after(&mut self) {
        if let Some(last) = self.undo_stack.back_mut() {
            last.selection_after = self.selection.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type BufferOp = fn(&mut TextBuffer);

    fn type_chars(buffer: &mut TextBuffer, text: &str) {
        for ch in text.chars() {
            buffer.replace_in_utf16_range(None, &ch.to_string());
        }
    }

    fn typed(text: &str) -> TextBuffer {
        let mut buffer = TextBuffer::default();
        type_chars(&mut buffer, text);
        buffer
    }

    fn loaded(text: &str) -> TextBuffer {
        let mut buffer = TextBuffer::default();
        buffer.reset(Arc::from(text));
        buffer
    }

    fn undo_trail(buffer: &mut TextBuffer) -> Vec<String> {
        let mut trail = Vec::new();
        while buffer.undo() {
            trail.push(buffer.as_str().to_string());
        }
        trail
    }

    #[test]
    fn typed_text_undoes_in_steps_that_break_at_a_space_following_a_word() {
        for (input, expected) in [
            ("hello", vec![""]),
            ("hi there", vec!["hi", ""]),
            ("a b c", vec!["a b", "a", ""]),
            ("hi   x", vec!["hi", ""]),
            ("  hi", vec![""]),
        ] {
            let mut buffer = typed(input);
            assert_eq!(undo_trail(&mut buffer), expected, "input {input:?}");
        }
    }

    #[test]
    fn caret_navigation_closes_the_typing_group() {
        let navigations: [BufferOp; 2] = [|b| b.move_to(2), |b| b.select_to(2)];
        for navigate in navigations {
            let mut buffer = typed("ab");
            navigate(&mut buffer);
            type_chars(&mut buffer, "c");
            assert_eq!(undo_trail(&mut buffer), vec!["ab", ""]);
        }
    }

    #[test]
    fn typing_after_an_undo_starts_a_new_step() {
        let mut buffer = typed("ab cd");
        buffer.undo();
        type_chars(&mut buffer, "x");
        assert_eq!(undo_trail(&mut buffer), vec!["ab", ""]);
    }

    #[test]
    fn consecutive_deletes_in_one_direction_undo_as_one_step() {
        let deletes: [(BufferOp, usize, &str); 2] = [
            (TextBuffer::delete_backward, 5, "he"),
            (TextBuffer::delete_forward, 0, "lo"),
        ];
        for (delete, caret, after_deletes) in deletes {
            let mut buffer = loaded("hello");
            buffer.move_to(caret);
            for _ in 0..3 {
                delete(&mut buffer);
            }
            assert_eq!(buffer.as_str(), after_deletes);
            assert_eq!(undo_trail(&mut buffer), vec!["hello"]);
        }
    }

    #[test]
    fn switching_delete_direction_starts_a_new_step() {
        let mut buffer = loaded("abcd");
        buffer.move_to(2);
        buffer.delete_backward();
        buffer.delete_forward();
        assert_eq!(undo_trail(&mut buffer), vec!["acd", "abcd"]);
    }

    #[test]
    fn backspace_after_typing_starts_a_new_step() {
        let mut buffer = typed("abc");
        buffer.delete_backward();
        assert_eq!(undo_trail(&mut buffer), vec!["abc", ""]);
    }

    #[test]
    fn deleting_a_selection_is_its_own_step() {
        let mut buffer = loaded("abcdef");
        buffer.move_to(4);
        buffer.select_to(2);
        buffer.delete_backward();
        buffer.delete_backward();
        assert_eq!(undo_trail(&mut buffer), vec!["abef", "abcdef"]);
    }

    #[test]
    fn undo_of_a_cut_restores_the_reversed_selection_that_was_cut() {
        let mut buffer = loaded("abcdef");
        buffer.move_to(4);
        buffer.select_to(1);
        buffer.insert("", EditKind::Standalone);
        buffer.undo();
        assert_eq!(
            (
                buffer.as_str(),
                buffer.selected_range().clone(),
                buffer.is_reversed()
            ),
            ("abcdef", 1..4, true)
        );
    }

    #[test]
    fn redo_restores_the_caret_from_after_the_edit() {
        let mut buffer = loaded("xyz");
        buffer.move_to(1);
        type_chars(&mut buffer, "ab");
        buffer.move_to(0);
        buffer.undo();
        assert_eq!(buffer.selected_range(), &(1..1));
        buffer.redo();
        assert_eq!(
            (buffer.as_str(), buffer.selected_range().clone()),
            ("xabyz", 3..3)
        );
    }

    #[test]
    fn a_new_edit_after_undo_discards_the_redo_history() {
        let mut buffer = typed("ab");
        buffer.undo();
        type_chars(&mut buffer, "c");
        assert!(!buffer.redo());
        assert_eq!(buffer.as_str(), "c");
    }

    #[test]
    fn reset_drops_history_and_puts_the_caret_at_the_end() {
        let mut buffer = typed("ab cd");
        buffer.undo();
        buffer.reset(Arc::from("привіт"));
        assert_eq!(
            (
                buffer.undo(),
                buffer.redo(),
                buffer.selected_range().clone()
            ),
            (false, false, 12..12)
        );
    }

    #[test]
    fn history_keeps_only_the_newest_edits_up_to_the_limit() {
        for (edits, expected_undos) in [
            (HISTORY_LIMIT - 1, HISTORY_LIMIT - 1),
            (HISTORY_LIMIT, HISTORY_LIMIT),
            (HISTORY_LIMIT + 1, HISTORY_LIMIT),
        ] {
            const LETTERS: [&str; 7] = ["a", "b", "c", "d", "e", "f", "g"];
            let mut buffer = TextBuffer::default();
            for i in 0..edits {
                buffer.insert(LETTERS[i % LETTERS.len()], EditKind::Standalone);
            }
            let first_letter = buffer.as_str()[..1].to_string();
            let undos = undo_trail(&mut buffer).len();
            let survivor = if edits > HISTORY_LIMIT {
                first_letter
            } else {
                String::new()
            };
            assert_eq!(
                (undos, buffer.as_str()),
                (expected_undos, survivor.as_str())
            );
        }
    }

    #[test]
    fn an_edit_that_changes_nothing_writes_no_history_step() {
        let mut buffer = typed("ab");
        buffer.insert("", EditKind::Standalone);
        assert_eq!(undo_trail(&mut buffer), vec![""]);
    }

    #[test]
    fn an_empty_composition_keeps_the_previous_step_redo_caret() {
        let mut buffer = typed("ab");
        buffer.move_to(0);
        buffer.replace_and_mark_in_utf16_range(None, "", Some(0..0));
        buffer.undo();
        buffer.redo();
        assert_eq!(buffer.selected_range(), &(2..2));
    }

    #[test]
    fn composition_selection_maps_utf16_offsets_into_non_ascii_text() {
        let mut buffer = loaded("x");
        buffer.replace_and_mark_in_utf16_range(None, "привіт", Some(2..3));
        assert_eq!(
            (
                buffer.marked_range().cloned(),
                buffer.selected_range().clone()
            ),
            (Some(1..13), 5..7)
        );
    }

    #[test]
    fn composition_updates_and_commit_undo_as_one_step() {
        let mut buffer = TextBuffer::default();
        buffer.replace_and_mark_in_utf16_range(None, "н", Some(1..1));
        buffer.replace_and_mark_in_utf16_range(None, "ні", Some(2..2));
        buffer.replace_in_utf16_range(None, "ніч");
        assert_eq!(undo_trail(&mut buffer), vec![""]);
    }

    #[test]
    fn commit_replaces_the_marked_range_and_clears_the_mark() {
        let mut buffer = loaded("ab");
        buffer.move_to(1);
        buffer.replace_and_mark_in_utf16_range(None, "x", None);
        buffer.replace_in_utf16_range(None, "yz");
        assert_eq!(
            (buffer.as_str(), buffer.marked_range(), buffer.cursor()),
            ("ayzb", None, 3)
        );
    }

    #[test]
    fn out_of_bounds_utf16_ranges_are_clamped_to_the_text() {
        for (range, expected) in [
            (1..99, "aZ"),
            (5..9, "abcZ"),
            (Range { start: 3, end: 1 }, "aZbc"),
        ] {
            let mut buffer = loaded("abc");
            buffer.replace_in_utf16_range(Some(range.clone()), "Z");
            assert_eq!(buffer.as_str(), expected, "range {range:?}");
        }
    }
}
