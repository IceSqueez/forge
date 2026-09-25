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
