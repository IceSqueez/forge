const OPEN: &str = "~q";
const CLOSE: char = '~';

/// Tags a payload with its sequence number so every effect it causes can be traced back to it.
pub fn marker(seq: u64) -> String {
    format!("{OPEN}{seq}{CLOSE}")
}

/// The first well-formed marker anywhere in `text`.
pub fn find_marker(text: &str) -> Option<u64> {
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let digits = after.len() - after.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits > 0 && after[digits..].starts_with(CLOSE) {
            return after[..digits].parse().ok();
        }
        rest = after;
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_embedded_anywhere_in_text_yields_its_sequence() {
        for seq in [0, 7, u64::MAX] {
            let text = format!("gg {} wp", marker(seq));
            assert_eq!(find_marker(&text), Some(seq), "{text}");
        }
    }

    #[test]
    fn malformed_markers_yield_nothing() {
        for text in [
            "",
            "~q~",
            "~q12",
            "~qx1~",
            "q12~",
            "~ q12~",
            "~q99999999999999999999~",
        ] {
            assert_eq!(find_marker(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_malformed_marker_does_not_hide_a_well_formed_one_after_it() {
        assert_eq!(find_marker("~q~q41~"), Some(41));
        assert_eq!(find_marker("~q1x ~q2~"), Some(2));
    }
}
