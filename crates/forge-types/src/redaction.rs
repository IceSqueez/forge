use std::fmt;

/// Every redacted rendering opens with this stamp, so a reader can tell a placeholder from a bug.
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
        write!(f, "<redacted len={}>", self.0)
    }
}
