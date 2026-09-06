use std::fmt;

/// The prefix every redacted rendering opens with; leak assertions and the bundle header match on this, not on `MARKER`.
pub const STAMP: &str = "<redacted";

/// The whole-value form, which withholds the size as well; length-bearing renderings only share `STAMP`.
pub const MARKER: &str = "<redacted>";

pub struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MARKER)
    }
}

/// Length is counted in characters, not bytes, so a multi-byte message does not read as inflated.
pub struct RedactedText(usize);

impl RedactedText {
    pub fn new(value: &str) -> Self {
        Self(value.chars().count())
    }
}

impl fmt::Debug for RedactedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{STAMP} len={}>", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Why: `<redacted` is the stamp every leak assertion in the workspace greps for and the stamp
    /// the log-bundle header names; both spellings are a cross-crate contract, not cosmetics.
    #[test]
    fn placeholder_debug_shapes_are_pinned() {
        assert_eq!(MARKER, "<redacted>");
        assert_eq!(format!("{Redacted:?}"), "<redacted>");
        assert_eq!(
            format!("{:?}", RedactedText::new("abc")),
            "<redacted len=3>"
        );
    }

    /// Why: the bundle header promises a reader that anything opening with `STAMP` was withheld
    /// on purpose. That promise only holds while every placeholder rendering starts with it, so a
    /// new placeholder shape that drifts off the stamp must fail here rather than in a bug report.
    #[test]
    fn every_placeholder_rendering_opens_with_the_shared_stamp() {
        for rendering in [
            MARKER.to_owned(),
            format!("{Redacted:?}"),
            format!("{:?}", RedactedText::new("")),
            format!("{:?}", RedactedText::new("Привіт")),
        ] {
            assert!(
                rendering.starts_with(STAMP),
                "{rendering:?} does not open with {STAMP:?}",
            );
        }
    }

    #[test]
    fn redacted_text_length_counts_characters_not_bytes() {
        for (input, chars) in [
            ("", 0),
            ("plain ascii", 11),
            ("Привіт", 6),
            ("🙂🙂", 2),
            ("e\u{0301}", 2),
        ] {
            assert_eq!(
                format!("{:?}", RedactedText::new(input)),
                format!("<redacted len={chars}>"),
                "input {input:?}"
            );
        }
    }
}
