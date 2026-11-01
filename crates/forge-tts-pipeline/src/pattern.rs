#[derive(Debug, Clone)]
pub struct CompiledPattern(regex::Regex);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError(String);

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PatternError {}

impl CompiledPattern {
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        regex::Regex::new(pattern)
            .map(Self)
            .map_err(|error| PatternError(error.to_string()))
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }

    pub(crate) fn regex(&self) -> &regex::Regex {
        &self.0
    }
}
