use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageCode([u8; 2]);

impl LanguageCode {
    pub fn from_locale(locale: &str) -> Option<Self> {
        let primary = locale.split(['-', '_']).next()?.as_bytes();
        let [first, second] = primary else {
            return None;
        };
        if !first.is_ascii_alphabetic() || !second.is_ascii_alphabetic() {
            return None;
        }
        Some(Self([
            first.to_ascii_lowercase(),
            second.to_ascii_lowercase(),
        ]))
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [first, second] = self.0;
        write!(f, "{}{}", first as char, second as char)
    }
}

impl fmt::Debug for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}
