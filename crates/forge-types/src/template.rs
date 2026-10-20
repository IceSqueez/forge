pub fn is_variable_reference(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

pub fn variable_references(template: &str) -> impl Iterator<Item = &str> {
    TemplatePieces::new(template).filter_map(|piece| match piece {
        TemplatePiece::Reference { name, .. } => Some(name),
        TemplatePiece::Literal(_) => None,
    })
}

pub enum TemplatePiece<'a> {
    Literal(&'a str),
    Reference {
        name: &'a str,
        format: Option<&'a str>,
        raw: &'a str,
    },
}

fn split_reference(body: &str) -> Option<(&str, Option<&str>)> {
    if is_variable_reference(body) {
        return Some((body, None));
    }
    let (name, format) = body.split_once(FORMAT_SEPARATOR)?;
    is_variable_reference(name).then_some((name, Some(format)))
}

const FORMAT_SEPARATOR: char = ':';

pub struct TemplatePieces<'a> {
    rest: &'a str,
}

impl<'a> TemplatePieces<'a> {
    pub fn new(template: &'a str) -> Self {
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
        if let Some((name, format)) = split_reference(&after[..close]) {
            self.rest = &after[close + 1..];
            return Some(TemplatePiece::Reference {
                name,
                format,
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
            ("%t:HH:mm% for %user%", vec!["t", "user"]),
            ("%a.b:yyyy%", vec!["a.b"]),
        ] {
            assert_eq!(
                variable_references(template).collect::<Vec<_>>(),
                expected,
                "template {template:?}"
            );
        }
    }

    fn references(template: &str) -> Vec<(&str, Option<&str>, &str)> {
        TemplatePieces::new(template)
            .filter_map(|piece| match piece {
                TemplatePiece::Reference { name, format, raw } => Some((name, format, raw)),
                TemplatePiece::Literal(_) => None,
            })
            .collect()
    }

    #[test]
    fn template_pieces_split_a_formatted_token_at_its_first_colon() {
        for (template, expected) in [
            ("%t:HH:mm%", vec![("t", Some("HH:mm"), "%t:HH:mm%")]),
            ("%a.b:yyyy%", vec![("a.b", Some("yyyy"), "%a.b:yyyy%")]),
            ("%a:%", vec![("a", Some(""), "%a:%")]),
            ("%a:b%user%", vec![("a", Some("b"), "%a:b%")]),
            ("%user%", vec![("user", None, "%user%")]),
            (
                "at %t:H% by %user%",
                vec![("t", Some("H"), "%t:H%"), ("user", None, "%user%")],
            ),
        ] {
            assert_eq!(references(template), expected, "template {template:?}");
        }
    }

    #[test]
    fn template_pieces_keep_a_colon_body_literal_when_the_name_part_is_invalid() {
        for template in ["%:x%", "%a b:x%", "% a:x%", "%a$b:x%", "%:%"] {
            assert_eq!(references(template), vec![], "template {template:?}");
        }
    }

    #[test]
    fn template_pieces_reassemble_into_the_original_template() {
        for template in [
            "%t:HH:mm% done",
            "%a:b%user%",
            "%a b:x%user%",
            "%:x% 50% %a:%",
            "tail %open:HH",
        ] {
            let rebuilt: String = TemplatePieces::new(template)
                .map(|piece| match piece {
                    TemplatePiece::Literal(text) => text,
                    TemplatePiece::Reference { raw, .. } => raw,
                })
                .collect();
            assert_eq!(rebuilt, template);
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
