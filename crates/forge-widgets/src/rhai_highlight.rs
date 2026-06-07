#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RhaiTokenKind {
    Keyword,
    Number,
    StringLit,
    TemplateLit,
    Comment,
    Namespace,
    FunctionCall,
    Identifier,
    Operator,
    Punctuation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kind_eq_reflexive() {
        assert_eq!(RhaiTokenKind::Keyword, RhaiTokenKind::Keyword);
        assert_eq!(RhaiTokenKind::Number, RhaiTokenKind::Number);
        assert_eq!(RhaiTokenKind::StringLit, RhaiTokenKind::StringLit);
        assert_eq!(RhaiTokenKind::TemplateLit, RhaiTokenKind::TemplateLit);
        assert_eq!(RhaiTokenKind::Comment, RhaiTokenKind::Comment);
        assert_eq!(RhaiTokenKind::Namespace, RhaiTokenKind::Namespace);
        assert_eq!(RhaiTokenKind::FunctionCall, RhaiTokenKind::FunctionCall);
        assert_eq!(RhaiTokenKind::Identifier, RhaiTokenKind::Identifier);
        assert_eq!(RhaiTokenKind::Operator, RhaiTokenKind::Operator);
        assert_eq!(RhaiTokenKind::Punctuation, RhaiTokenKind::Punctuation);
    }

    #[test]
    fn token_kind_ne_distinct() {
        assert_ne!(RhaiTokenKind::Keyword, RhaiTokenKind::Identifier);
        assert_ne!(RhaiTokenKind::Number, RhaiTokenKind::StringLit);
        assert_ne!(RhaiTokenKind::Comment, RhaiTokenKind::FunctionCall);
    }

    #[test]
    fn token_kind_clone_round_trip() {
        let original = RhaiTokenKind::Namespace;
        let cloned = original;
        assert_eq!(original, cloned);
    }
}
