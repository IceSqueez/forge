use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

pub(crate) const MASK_GLYPH: &str = "\u{2022}";

pub(crate) fn mask_graphemes(text: &str) -> String {
    MASK_GLYPH.repeat(text.graphemes(true).count())
}

pub(crate) fn mask_offset(text: &str, byte_offset: usize) -> usize {
    let clusters_before = text
        .grapheme_indices(true)
        .take_while(|(idx, _)| *idx < byte_offset)
        .count();
    clusters_before * MASK_GLYPH.len()
}

pub(crate) fn content_offset_for_mask(text: &str, mask_offset: usize) -> usize {
    let cluster_index = mask_offset / MASK_GLYPH.len();
    text.grapheme_indices(true)
        .map(|(idx, _)| idx)
        .nth(cluster_index)
        .unwrap_or(text.len())
}

pub(crate) fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

pub(crate) fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

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

pub(crate) fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

pub(crate) fn range_from_utf16(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    offset_from_utf16(text, range_utf16.start)..offset_from_utf16(text, range_utf16.end)
}
