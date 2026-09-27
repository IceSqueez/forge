use std::fmt;

pub const STAMP: &str = "<redacted";

pub const MARKER: &str = "<redacted>";

pub struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MARKER)
    }
}

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

    #[test]
    fn placeholder_debug_shapes_are_pinned() {
        assert_eq!(MARKER, "<redacted>");
        assert_eq!(format!("{Redacted:?}"), "<redacted>");
        assert_eq!(
            format!("{:?}", RedactedText::new("abc")),
            "<redacted len=3>"
        );
    }

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
