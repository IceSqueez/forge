use crate::rhai_highlight::{RhaiTokenKind, tokenize_line};

/// Keys are `"last_submodule::fn"` for sub-module calls; `"root_module::fn"` for top-level calls.
pub fn lookup_signature(qualified: &str) -> Option<&'static str> {
    match qualified {
        "forge::log" => Some("()"),
        "forge::warn" => Some("()"),
        "forge::sleep" => Some("()"),
        "globals::get" => Some("Variant"),
        "globals::set" => Some("()"),
        "globals::incr" => Some("Int"),
        "globals::del" => Some("Bool"),
        "chat::send" => Some("()"),
        "chat::reply" => Some("()"),
        "chat::whisper" => Some("()"),
        "time::now" => Some("String"),
        "time::unix" => Some("Int"),
        "tts::speak" => Some("()"),
        "tts::speak_as" => Some("()"),
        "tts::skip" => Some("()"),
        "tts::clear" => Some("()"),
        _ => None,
    }
}

/// Multi-line `let` patterns (assignment continues on next line) always return `None`.
pub fn scan_type_hint(line: &str) -> Option<(String, &'static str)> {
    let (tokens, _) = tokenize_line(line, false);

    let get = |i: usize| -> Option<(&std::ops::Range<usize>, &RhaiTokenKind)> {
        tokens.get(i).map(|(r, k)| (r, k))
    };

    let (r, k) = get(0)?;
    if *k != RhaiTokenKind::Keyword || &line[r.clone()] != "let" {
        return None;
    }

    let (ir, ik) = get(1)?;
    if *ik != RhaiTokenKind::Identifier {
        return None;
    }
    let ident = line[ir.clone()].to_owned();

    let (er, ek) = get(2)?;
    if *ek != RhaiTokenKind::Operator || &line[er.clone()] != "=" {
        return None;
    }

    let mut tok_pos = 3usize;
    let mut namespaces: Vec<&str> = Vec::new();

    loop {
        let (tr, tk) = get(tok_pos)?;
        tok_pos += 1;
        match tk {
            RhaiTokenKind::Namespace => {
                namespaces.push(&line[tr.clone()]);
                let (sr, sk) = get(tok_pos)?;
                tok_pos += 1;
                if *sk != RhaiTokenKind::Punctuation || &line[sr.clone()] != "::" {
                    return None;
                }
            }
            RhaiTokenKind::FunctionCall => {
                let fn_name = &line[tr.clone()];
                let qualified = match namespaces.len() {
                    0 => return None,
                    1 => format!("{}::{}", namespaces[0], fn_name),
                    _ => format!("{}::{}", namespaces[namespaces.len() - 1], fn_name),
                };
                return lookup_signature(&qualified).map(|ty| (ident, ty));
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_let_globals_get_returns_variant() {
        let result = scan_type_hint(r#"let x = forge::globals::get("counter");"#);
        assert_eq!(result, Some(("x".to_owned(), "Variant")));
    }

    #[test]
    fn scan_let_namespaced_chat_send_returns_unit() {
        let result = scan_type_hint(r#"let y = forge::chat::send("hello");"#);
        assert_eq!(result, Some(("y".to_owned(), "()")));
    }

    #[test]
    fn scan_unknown_fn_returns_none() {
        let result = scan_type_hint("let z = foo::bar(1);");
        assert_eq!(result, None);
    }

    #[test]
    fn scan_no_let_keyword_returns_none() {
        let result = scan_type_hint(r#"x = forge::globals::get("k");"#);
        assert_eq!(result, None);
    }

    #[test]
    fn scan_let_without_call_returns_none() {
        let result = scan_type_hint("let x = 42;");
        assert_eq!(result, None);
    }
}
