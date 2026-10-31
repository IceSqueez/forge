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

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY_ZWJ: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

    fn mixed() -> String {
        format!("a{FAMILY_ZWJ}e\u{301}z")
    }

    #[test]
    fn a_mask_shows_one_glyph_per_user_perceived_character() {
        for (text, clusters) in [
            ("", 0),
            ("abc", 3),
            (FAMILY_ZWJ, 1),
            ("e\u{301}", 1),
            ("\r\n", 1),
        ] {
            assert_eq!(mask_graphemes(text).chars().count(), clusters, "{text:?}");
        }
    }

    #[test]
    fn every_grapheme_boundary_round_trips_through_the_mask_offset() {
        let text = mixed();
        let boundaries = text
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain([text.len()]);
        for boundary in boundaries {
            assert_eq!(
                content_offset_for_mask(&text, mask_offset(&text, boundary)),
                boundary
            );
        }
    }

    #[test]
    fn a_mask_offset_counts_the_clusters_before_the_caret() {
        let text = mixed();
        let after_family = 1 + FAMILY_ZWJ.len();

        assert_eq!(mask_offset(&text, 0), 0);
        assert_eq!(mask_offset(&text, 1), MASK_GLYPH.len());
        assert_eq!(mask_offset(&text, after_family), 2 * MASK_GLYPH.len());
    }

    #[test]
    fn a_caret_inside_a_cluster_masks_to_the_end_of_that_cluster() {
        let text = mixed();
        let inside_family = 1 + "\u{1F468}".len();

        assert_eq!(mask_offset(&text, inside_family), 2 * MASK_GLYPH.len());
    }

    #[test]
    fn a_mask_offset_inside_a_glyph_lands_on_the_cluster_it_belongs_to() {
        assert_eq!(content_offset_for_mask("abc", MASK_GLYPH.len() + 1), 1);
    }

    #[test]
    fn a_mask_offset_past_the_end_lands_at_the_end_of_the_text() {
        assert_eq!(content_offset_for_mask("ab", 99 * MASK_GLYPH.len()), 2);
    }
}
