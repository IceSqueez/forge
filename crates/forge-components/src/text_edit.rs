//! Pure, buffer-agnostic text helpers shared by the single-line [`crate::text_input`]
//! and the multi-line [`crate::text_area`] editors: extended-grapheme-cluster cursor
//! motion and UTF-8 ↔ UTF-16 offset mapping (the latter for IME coordinate exchange).
//!
//! These operate on a `&str` and hold no editor state, so both entities call them
//! rather than each reimplementing grapheme walking — the one place cursor-boundary
//! correctness lives.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The byte offset of the grapheme-cluster boundary immediately before `offset`,
/// or `0` when `offset` is already at or before the start. Steps one whole extended
/// grapheme cluster so the caret never lands inside a multi-byte cluster (emoji, ZWJ
/// sequence, regional-indicator flag).
pub(crate) fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

/// The byte offset of the grapheme-cluster boundary immediately after `offset`, or
/// `text.len()` when `offset` is at or past the end.
pub(crate) fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

/// Maps a UTF-8 byte offset into `text` to the matching UTF-16 code-unit offset
/// (astral-plane characters count as a surrogate pair, i.e. two units).
pub(crate) fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    let mut utf8_count = 0;
    for ch in text.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }
    utf16_offset
}

/// Inverse of [`offset_to_utf16`]: maps a UTF-16 code-unit offset back to the
/// UTF-8 byte offset into `text`.
pub(crate) fn offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;
    for ch in text.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }
    utf8_offset
}

/// [`offset_to_utf16`] applied to both ends of a byte range.
pub(crate) fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

/// [`offset_from_utf16`] applied to both ends of a UTF-16 range.
pub(crate) fn range_from_utf16(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range_utf16.start)..offset_from_utf16(text, range_utf16.end)
}
