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

#[derive(Debug, Clone)]
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

struct LineEdit {
    at: usize,
    removed: usize,
    inserted: usize,
}

fn shifted(offset: usize, edits: &[LineEdit], stays_at_line_start: bool) -> usize {
    let mut added = 0;
    let mut dropped = 0;
    for edit in edits {
        if offset < edit.at || (stays_at_line_start && offset == edit.at) {
            break;
        }
        added += edit.inserted;
        dropped += edit.removed.min(offset - edit.at);
    }
    offset + added - dropped
}

fn outdent_width(line: &str, unit: &str) -> usize {
    if line.starts_with('\t') {
        return 1;
    }
    let spaces = line.len() - line.trim_start_matches(' ').len();
    spaces.min(unit.len())
}

fn line_bounds_at_index(text: &str, line_index: usize) -> Range<usize> {
    let mut start = 0;
    for _ in 0..line_index {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => break,
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    start..end
}

fn spliced(text: &str, range: Range<usize>, replacement: &str) -> Arc<str> {
    let mut out = String::with_capacity(text.len() - range.len() + replacement.len());
    out.push_str(&text[..range.start]);
    out.push_str(replacement);
    out.push_str(&text[range.end..]);
    out.into()
}

#[derive(Debug)]
struct CompositionOrigin {
    text: Arc<str>,
    selection: Selection,
    group_open: bool,
    pushes: u64,
    last_change: Option<Change>,
    redo_stack: Vec<Change>,
}

#[derive(Debug)]
pub(crate) struct TextBuffer {
    text: Arc<str>,
    selection: Selection,
    marked: Option<Range<usize>>,
    undo_stack: VecDeque<Change>,
    redo_stack: Vec<Change>,
    group_open: bool,
    pushes: u64,
    composition_origin: Option<CompositionOrigin>,
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
            pushes: 0,
            composition_origin: None,
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
        self.composition_origin = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.group_open = false;
    }

    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }

    pub(crate) fn discard_marked(&mut self) -> bool {
        let Some(range) = self.marked.take() else {
            return false;
        };
        match self.composition_origin.take() {
            Some(origin) => self.roll_back_composition(origin),
            None => {
                self.apply(range, "", EditKind::Standalone);
            }
        }
        true
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

    pub(crate) fn replace_all_keeping_caret_line(&mut self, text: &str) {
        let cursor = self.cursor();
        let line_index = self.text[..cursor].matches('\n').count();
        let column = cursor - self.cursor_line_bounds().start;
        if !self.apply(0..self.text.len(), text, EditKind::Standalone) {
            return;
        }
        let line = line_bounds_at_index(&self.text, line_index);
        let mut offset = (line.start + column).min(line.end);
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        self.selection = Selection {
            range: offset..offset,
            reversed: false,
        };
        self.marked = None;
        self.set_last_selection_after();
    }

    pub(crate) fn insert_newline_keeping_indent(&mut self) {
        let at = self.target_range(None).start;
        let line_start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
        let before_caret = &self.text[line_start..at];
        let indent_len = before_caret.len() - before_caret.trim_start_matches([' ', '\t']).len();
        let newline = format!("\n{}", &before_caret[..indent_len]);
        self.insert(&newline, EditKind::Standalone);
    }

    pub(crate) fn indent(&mut self, unit: &str) {
        let range = self.target_range(None);
        if !self.text[range].contains('\n') {
            self.insert(unit, EditKind::Standalone);
            return;
        }
        self.reshape_touched_lines(|line| (!line.is_empty()).then_some((0, unit)));
    }

    pub(crate) fn outdent(&mut self, unit: &str) {
        self.reshape_touched_lines(|line| {
            let removed = outdent_width(line, unit);
            (removed > 0).then_some((removed, ""))
        });
    }

    fn touched_lines(&self) -> Range<usize> {
        let range = self.target_range(None);
        let start = self.text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let last = if range.end > range.start && self.text[..range.end].ends_with('\n') {
            range.end - 1
        } else {
            range.end
        };
        let end = self.text[last..]
            .find('\n')
            .map_or(self.text.len(), |i| last + i);
        start..end
    }

    fn reshape_touched_lines<'u>(&mut self, edit_for: impl Fn(&str) -> Option<(usize, &'u str)>) {
        let block = self.touched_lines();
        let mut reshaped = String::with_capacity(block.len());
        let mut edits = Vec::new();
        let mut line_start = block.start;
        for (index, line) in self.text[block.clone()].split('\n').enumerate() {
            if index > 0 {
                reshaped.push('\n');
            }
            match edit_for(line) {
                Some((removed, prefix)) => {
                    reshaped.push_str(prefix);
                    reshaped.push_str(&line[removed..]);
                    edits.push(LineEdit {
                        at: line_start,
                        removed,
                        inserted: prefix.len(),
                    });
                }
                None => reshaped.push_str(line),
            }
            line_start += line.len() + 1;
        }
        if edits.is_empty() {
            return;
        }

        let before = self.selection.clone();
        let selection = Selection {
            range: shifted(before.range.start, &edits, true)
                ..shifted(before.range.end, &edits, false),
            reversed: before.reversed,
        };
        self.apply(block, &reshaped, EditKind::Standalone);
        self.selection = selection;
        self.marked = None;
        self.set_last_selection_after();
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
        if self.marked.is_none() {
            self.composition_origin = Some(self.capture_composition_origin());
        }
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

    fn capture_composition_origin(&self) -> CompositionOrigin {
        CompositionOrigin {
            text: self.text.clone(),
            selection: self.selection.clone(),
            group_open: self.group_open,
            pushes: self.pushes,
            last_change: self.undo_stack.back().cloned(),
            redo_stack: self.redo_stack.clone(),
        }
    }

    fn roll_back_composition(&mut self, origin: CompositionOrigin) {
        for _ in 0..self.pushes.saturating_sub(origin.pushes) {
            self.undo_stack.pop_back();
        }
        if let (Some(last), Some(slot)) = (origin.last_change, self.undo_stack.back_mut()) {
            *slot = last;
        }
        self.redo_stack = origin.redo_stack;
        self.text = origin.text;
        self.selection = origin.selection;
        self.group_open = origin.group_open;
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
            self.pushes += 1;
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
    fn discarding_removes_only_a_live_composition_and_reports_whether_there_was_one() {
        let untouched: BufferOp = |_| {};
        let empty_composition: BufferOp = |b| b.replace_and_mark_in_utf16_range(None, "", None);
        let live_composition: BufferOp =
            |b| b.replace_and_mark_in_utf16_range(None, "ні", Some(2..2));
        let committed_composition: BufferOp = |b| {
            b.replace_and_mark_in_utf16_range(None, "ні", None);
            b.replace_in_utf16_range(None, "ніч");
        };
        for (label, prepare, expected) in [
            ("nothing composed", untouched, (false, "ab", 1)),
            ("empty composition", empty_composition, (false, "ab", 1)),
            ("live composition", live_composition, (true, "ab", 1)),
            (
                "committed composition",
                committed_composition,
                (false, "aнічb", 7),
            ),
        ] {
            let mut buffer = loaded("ab");
            buffer.move_to(1);
            prepare(&mut buffer);

            let discarded = buffer.discard_marked();

            assert_eq!(
                (discarded, buffer.as_str(), buffer.cursor()),
                expected,
                "{label}"
            );
            assert_eq!(buffer.marked_range(), None, "{label}");
        }
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

    #[test]
    fn newline_copies_the_space_and_tab_indent_before_the_caret() {
        for (text, caret, expected, cursor) in [
            ("  ab", 4, "  ab\n  ", 7),
            ("  ab", 1, " \n  ab", 3),
            ("  ab", 0, "\n  ab", 1),
            ("ab", 2, "ab\n", 3),
            ("\t\tx", 3, "\t\tx\n\t\t", 6),
            (" \tx", 3, " \tx\n \t", 6),
            ("a\n    b", 7, "a\n    b\n    ", 12),
            ("  ab", 3, "  a\n  b", 6),
            ("    ", 4, "    \n    ", 9),
            (
                "  \u{43f}\u{440}\u{438}",
                8,
                "  \u{43f}\u{440}\u{438}\n  ",
                11,
            ),
            ("\u{3000}x", 4, "\u{3000}x\n", 5),
        ] {
            let mut buffer = loaded(text);
            buffer.move_to(caret);
            buffer.insert_newline_keeping_indent();
            assert_eq!(
                (buffer.as_str(), buffer.cursor()),
                (expected, cursor),
                "{text:?} at {caret}"
            );
        }
    }

    #[test]
    fn newline_replaces_the_selection_and_takes_the_indent_from_its_start_line() {
        for (text, select, reversed, expected) in [
            ("  ab cd", 4..7, false, "  ab\n  "),
            ("  ab cd", 4..7, true, "  ab\n  "),
            ("  a\n    b", 2..8, false, "  \n  b"),
            ("\tx\n  y", 5..6, false, "\tx\n  \n  "),
        ] {
            let mut buffer = loaded(text);
            if reversed {
                buffer.move_to(select.end);
                buffer.select_to(select.start);
            } else {
                buffer.move_to(select.start);
                buffer.select_to(select.end);
            }
            buffer.insert_newline_keeping_indent();
            assert_eq!(buffer.as_str(), expected, "{text:?} {select:?}");
        }
    }

    #[test]
    fn newline_with_indent_undoes_as_its_own_single_step() {
        let mut buffer = typed("  ab");
        buffer.insert_newline_keeping_indent();
        type_chars(&mut buffer, "cd");
        assert_eq!(undo_trail(&mut buffer), vec!["  ab\n  ", "  ab", ""]);
    }

    fn selecting(text: &str, range: Range<usize>, reversed: bool) -> TextBuffer {
        let mut buffer = loaded(text);
        if reversed {
            buffer.move_to(range.end);
            buffer.select_to(range.start);
        } else {
            buffer.move_to(range.start);
            buffer.select_to(range.end);
        }
        buffer
    }

    fn shape(buffer: &TextBuffer) -> (&str, Range<usize>, bool) {
        (
            buffer.as_str(),
            buffer.selected_range().clone(),
            buffer.is_reversed(),
        )
    }

    #[test]
    fn indent_prefixes_each_non_empty_touched_line_and_moves_the_selection_with_it() {
        for (text, select, reversed, expected, remapped) in [
            ("ab\ncd", 1..4, false, "  ab\n  cd", 3..8),
            ("ab\ncd", 1..4, true, "  ab\n  cd", 3..8),
            ("ab\ncd", 0..5, false, "  ab\n  cd", 0..9),
            ("ab\ncd\nef", 0..6, false, "  ab\n  cd\nef", 0..10),
            ("ab\ncd", 2..3, false, "  ab\ncd", 4..5),
            ("ab\n\ncd", 0..6, false, "  ab\n\n  cd", 0..10),
            (
                "\u{43f}\u{440}\n\u{432}\u{456}",
                2..7,
                false,
                "  \u{43f}\u{440}\n  \u{432}\u{456}",
                4..11,
            ),
        ] {
            let mut buffer = selecting(text, select.clone(), reversed);
            buffer.indent("  ");
            assert_eq!(
                shape(&buffer),
                (expected, remapped, reversed),
                "{text:?} {select:?}"
            );
        }
    }

    #[test]
    fn outdent_removes_at_most_one_unit_or_one_leading_tab_from_each_touched_line() {
        for (text, select, reversed, expected, remapped) in [
            ("    a\n  b\n c\nd", 0..14, false, "  a\nb\nc\nd", 0..9),
            ("  a\n  b", 1..7, true, "a\nb", 0..3),
            ("\t\tx\n\ty", 0..6, false, "\tx\ny", 0..4),
            ("  a\n  b", 0..4, false, "a\n  b", 0..2),
            ("    ab", 6..6, false, "  ab", 4..4),
            ("    ab", 1..1, false, "  ab", 0..0),
            (" \tx", 3..3, false, "\tx", 2..2),
            (
                "  \u{43f}\u{440}\n  \u{432}\u{456}",
                2..13,
                false,
                "\u{43f}\u{440}\n\u{432}\u{456}",
                0..9,
            ),
        ] {
            let mut buffer = selecting(text, select.clone(), reversed);
            buffer.outdent("  ");
            assert_eq!(
                shape(&buffer),
                (expected, remapped, reversed),
                "{text:?} {select:?}"
            );
        }
    }

    #[test]
    fn reshaping_lines_that_need_no_change_leaves_the_text_and_history_untouched() {
        let reshapes: [(BufferOp, &str, Range<usize>); 3] = [
            (|b| b.outdent("  "), "ab\ncd", 0..5),
            (|b| b.outdent("  "), "ab", 1..1),
            (|b| b.indent("  "), "\n\n", 0..2),
        ];
        for (reshape, text, select) in reshapes {
            let mut buffer = selecting(text, select.clone(), false);
            reshape(&mut buffer);
            let after = (buffer.as_str().to_string(), buffer.selected_range().clone());
            assert_eq!(
                (after, buffer.undo()),
                ((text.to_string(), select), false),
                "{text:?}"
            );
        }
    }

    #[test]
    fn block_indent_and_outdent_each_undo_as_one_step() {
        let reshapes: [(BufferOp, &str); 2] = [
            (|b| b.indent("  "), "ab\ncd\nef"),
            (|b| b.outdent("  "), "  ab\n  cd\n  ef"),
        ];
        for (reshape, text) in reshapes {
            let mut buffer = typed("x");
            buffer.reset(Arc::from(text));
            buffer.select_all();
            reshape(&mut buffer);
            assert_eq!(undo_trail(&mut buffer), vec![text], "{text:?}");
        }
    }

    #[test]
    fn undo_of_a_block_indent_restores_the_selection_and_redo_restores_the_shifted_one() {
        let mut buffer = selecting("ab\ncd", 1..4, true);
        buffer.indent("  ");
        buffer.undo();
        let undone = (
            buffer.as_str().to_string(),
            buffer.selected_range().clone(),
            buffer.is_reversed(),
        );
        buffer.redo();
        assert_eq!(
            (undone, shape(&buffer)),
            (
                ("ab\ncd".to_string(), 1..4, true),
                ("  ab\n  cd", 3..8, true)
            )
        );
    }

    #[test]
    fn tab_within_a_single_line_inserts_the_unit_in_place_of_the_selection() {
        for (text, select, expected, cursor) in [
            ("ab", 1..1, "a  b", 3),
            ("abc", 1..2, "a  c", 3),
            ("ab\ncd", 4..4, "ab\nc  d", 6),
        ] {
            let mut buffer = selecting(text, select.clone(), false);
            buffer.indent("  ");
            assert_eq!(
                (buffer.as_str(), buffer.cursor()),
                (expected, cursor),
                "{text:?} {select:?}"
            );
        }
    }

    #[test]
    fn replacing_all_text_keeps_the_caret_line_number_and_byte_column_clamped_to_the_new_text() {
        for (text, caret, replacement, expected) in [
            ("ab\ncd\nef", 4, "x\nyyyy\nz", 3),
            ("ab\ncdef", 5, "ab\ncd", 5),
            ("ab\ncdef", 6, "ab\ncd", 5),
            ("ab\ncdef", 7, "AB\nC", 4),
            ("ab\ncd", 4, "\n\nab\ncd", 1),
            ("a\nb\nc", 5, "xyz", 1),
            ("a\nb\nc", 5, "x\nyz", 3),
            ("abc", 2, "", 0),
            ("", 0, "abc", 0),
            ("x", 1, "  x\n  y", 1),
            ("при\nвіт", 9, "x\nвіт!", 4),
        ] {
            let mut buffer = loaded(text);
            buffer.move_to(caret);
            buffer.replace_all_keeping_caret_line(replacement);
            assert_eq!(
                (buffer.as_str(), buffer.selected_range().clone()),
                (replacement, expected..expected),
                "{text:?} at {caret} -> {replacement:?}"
            );
        }
    }

    #[test]
    fn a_caret_column_inside_a_multibyte_char_of_the_new_text_snaps_back_to_its_start() {
        for (caret, replacement, expected) in [
            (1, "привіт", 0),
            (2, "привіт", 2),
            (3, "привіт", 2),
            (2, "€x", 0),
            (3, "€x", 3),
            (4, "a🚀", 1),
        ] {
            let mut buffer = loaded("abcdef");
            buffer.move_to(caret);
            buffer.replace_all_keeping_caret_line(replacement);
            assert_eq!(
                buffer.cursor(),
                expected,
                "column {caret} into {replacement:?}"
            );
        }
    }

    #[test]
    fn replacing_all_text_collapses_a_selection_to_the_line_and_column_of_its_caret_end() {
        for (reversed, expected) in [(false, 6), (true, 1)] {
            let mut buffer = selecting("ab\ncdef", 1..6, reversed);
            buffer.replace_all_keeping_caret_line("AB\nCDEF");
            assert_eq!(
                shape(&buffer),
                ("AB\nCDEF", expected..expected, false),
                "reversed {reversed}"
            );
        }
    }

    #[test]
    fn replacing_all_text_after_typing_undoes_as_its_own_single_step() {
        let mut buffer = typed("ab");
        buffer.replace_all_keeping_caret_line("a b");
        assert_eq!(undo_trail(&mut buffer), vec!["ab", ""]);
    }

    #[test]
    fn undo_of_a_full_replace_restores_the_selection_and_redo_restores_the_kept_caret() {
        let mut buffer = selecting("ab\ncdef", 4..7, true);
        buffer.replace_all_keeping_caret_line("ab\nCD");
        buffer.undo();
        let undone = (
            buffer.as_str().to_string(),
            buffer.selected_range().clone(),
            buffer.is_reversed(),
        );
        buffer.redo();
        assert_eq!(
            (undone, shape(&buffer)),
            (
                ("ab\ncdef".to_string(), 4..7, true),
                ("ab\nCD", 4..4, false)
            )
        );
    }
}
