use forge_types::{ArgStack, TemplatePiece, TemplatePieces};

const QUOTE: char = '"';
const BACKSLASH: char = '\\';

#[derive(Default)]
struct JsonLexState {
    in_string: bool,
    escaped: bool,
}

impl JsonLexState {
    fn advance(&mut self, literal: &str) {
        for character in literal.chars() {
            if self.escaped {
                self.escaped = false;
            } else if self.in_string && character == BACKSLASH {
                self.escaped = true;
            } else if character == QUOTE {
                self.in_string = !self.in_string;
            }
        }
    }
}

pub(crate) fn interpolate_json(template: &str, args: &ArgStack) -> String {
    let mut result = String::with_capacity(template.len());
    let mut state = JsonLexState::default();
    for piece in TemplatePieces::new(template) {
        match piece {
            TemplatePiece::Literal(text) => {
                state.advance(text);
                result.push_str(text);
            }
            TemplatePiece::Reference { raw, .. } if state.in_string && state.escaped => {
                state.advance(raw);
                result.push_str(raw);
            }
            TemplatePiece::Reference { raw, .. } if state.in_string => {
                push_string_contents(&mut result, &args.interpolate(raw));
            }
            TemplatePiece::Reference { name, format, raw } => match args.resolve(name) {
                Some(value) if format.is_none() => {
                    result.push_str(&value.to_plain_json().to_string());
                }
                Some(_) => {
                    result.push(QUOTE);
                    push_string_contents(&mut result, &args.interpolate(raw));
                    result.push(QUOTE);
                }
                None => {
                    state.advance(raw);
                    result.push_str(raw);
                }
            },
        }
    }
    result
}

fn push_string_contents(result: &mut String, text: &str) {
    let quoted = serde_json::Value::String(text.to_owned()).to_string();
    result.push_str(&quoted[QUOTE.len_utf8()..quoted.len() - QUOTE.len_utf8()]);
}

#[cfg(test)]
mod tests {
    use forge_types::{ArgStack, Variant};

    use super::interpolate_json;

    #[test]
    fn an_unresolved_bare_reference_with_a_quote_keeps_string_tracking_in_step_with_the_output() {
        let args = ArgStack::new().set("msg".to_owned(), Variant::String("a\"b".to_owned()));

        assert_eq!(
            interpolate_json(r#"%ghost:"%%msg%""#, &args),
            r#"%ghost:"%a\"b""#
        );
    }
}
