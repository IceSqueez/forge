pub fn is_variable_reference(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Names of every `%name%` token in `template`, in order; a `%` that does not open a valid token is literal text.
pub fn variable_references(template: &str) -> impl Iterator<Item = &str> {
    TemplatePieces::new(template).filter_map(|piece| match piece {
        TemplatePiece::Reference { name, .. } => Some(name),
        TemplatePiece::Literal(_) => None,
    })
}

pub(crate) enum TemplatePiece<'a> {
    Literal(&'a str),
    Reference { name: &'a str, raw: &'a str },
}

pub(crate) struct TemplatePieces<'a> {
    rest: &'a str,
}

impl<'a> TemplatePieces<'a> {
    pub(crate) fn new(template: &'a str) -> Self {
        Self { rest: template }
    }
}

impl<'a> Iterator for TemplatePieces<'a> {
    type Item = TemplatePiece<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let rest = self.rest;
        if rest.is_empty() {
            return None;
        }
        let open = rest.find('%').unwrap_or(rest.len());
        if open > 0 {
            self.rest = &rest[open..];
            return Some(TemplatePiece::Literal(&rest[..open]));
        }
        let after = &rest[1..];
        let Some(close) = after.find('%') else {
            self.rest = "";
            return Some(TemplatePiece::Literal(rest));
        };
        let name = &after[..close];
        if is_variable_reference(name) {
            self.rest = &after[close + 1..];
            return Some(TemplatePiece::Reference {
                name,
                raw: &rest[..close + 2],
            });
        }
        self.rest = after;
        Some(TemplatePiece::Literal(&rest[..1]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_references_yields_only_names_of_valid_tokens_in_order() {
        for (template, expected) in [
            ("%user% gets 50% off", vec!["user"]),
            ("50%%user%", vec!["user"]),
            ("% user %", vec![]),
            ("Discount 50% for %user%!", vec!["user"]),
            ("%a%%b%", vec!["a", "b"]),
            ("%user% then %dangling", vec!["user"]),
            ("50% the prize %user%", vec!["user"]),
            ("%名前%", vec!["名前"]),
            ("100%", vec![]),
            ("%%", vec![]),
            ("", vec![]),
        ] {
            assert_eq!(
                variable_references(template).collect::<Vec<_>>(),
                expected,
                "template {template:?}"
            );
        }
    }

    #[test]
    fn is_variable_reference_accepts_letters_digits_underscore_dot_and_dash_only() {
        for name in [
            "user",
            "user.name",
            "a-b",
            "a_b",
            "1st",
            "名前",
            "користувач",
        ] {
            assert!(is_variable_reference(name), "expected accept for {name:?}");
        }
        for name in ["", "a b", " user", "a$b", "a!", "a/b"] {
            assert!(!is_variable_reference(name), "expected reject for {name:?}");
        }
    }
}
