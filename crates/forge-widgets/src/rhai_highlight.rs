#[derive(Debug, Clone, Default)]
pub struct RhaiHighlighterSettings {
    pub error_lines: Vec<usize>,
}

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

    #[test]
    fn highlighter_settings_default_has_empty_error_lines() {
        let s = RhaiHighlighterSettings::default();
        assert!(s.error_lines.is_empty());
    }

    #[test]
    fn highlighter_settings_error_lines_round_trip() {
        let s = RhaiHighlighterSettings {
            error_lines: vec![1, 5, 12],
        };
        assert_eq!(s.error_lines, [1, 5, 12]);
    }
}
