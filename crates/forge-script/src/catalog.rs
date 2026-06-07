use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Fn,
    Property,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDescriptor {
    pub name: &'static str,
    pub ty: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub kind: SymbolKind,
    pub params: &'static [ParamDescriptor],
    pub return_type: &'static str,
    pub doc: Option<&'static str>,
}

static CATALOG: &[MethodDescriptor] = &[
    MethodDescriptor {
        namespace: None,
        name: "log",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "msg",
            ty: "string",
        }],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: None,
        name: "warn",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "msg",
            ty: "string",
        }],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: None,
        name: "error",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "msg",
            ty: "string",
        }],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: None,
        name: "sleep",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "ms",
            ty: "int",
        }],
        return_type: "()",
        doc: Some("Clamped to the script's remaining wall-time budget."),
    },
    MethodDescriptor {
        namespace: Some("chat"),
        name: "send",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "text",
            ty: "string",
        }],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("chat"),
        name: "reply",
        kind: SymbolKind::Fn,
        params: &[
            ParamDescriptor {
                name: "to",
                ty: "string",
            },
            ParamDescriptor {
                name: "text",
                ty: "string",
            },
        ],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("chat"),
        name: "whisper",
        kind: SymbolKind::Fn,
        params: &[
            ParamDescriptor {
                name: "user",
                ty: "string",
            },
            ParamDescriptor {
                name: "text",
                ty: "string",
            },
        ],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("globals"),
        name: "get",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "key",
            ty: "string",
        }],
        return_type: "Variant",
        doc: Some("Returns () when the key is absent."),
    },
    MethodDescriptor {
        namespace: Some("globals"),
        name: "set",
        kind: SymbolKind::Fn,
        params: &[
            ParamDescriptor {
                name: "key",
                ty: "string",
            },
            ParamDescriptor {
                name: "value",
                ty: "Variant",
            },
            ParamDescriptor {
                name: "persisted",
                ty: "bool",
            },
        ],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("globals"),
        name: "incr",
        kind: SymbolKind::Fn,
        params: &[
            ParamDescriptor {
                name: "key",
                ty: "string",
            },
            ParamDescriptor {
                name: "amount",
                ty: "int",
            },
        ],
        return_type: "Int",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("globals"),
        name: "del",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "key",
            ty: "string",
        }],
        return_type: "Bool",
        doc: Some("Returns true if the key existed."),
    },
    MethodDescriptor {
        namespace: Some("tts"),
        name: "speak",
        kind: SymbolKind::Fn,
        params: &[ParamDescriptor {
            name: "text",
            ty: "string",
        }],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("tts"),
        name: "speak_as",
        kind: SymbolKind::Fn,
        params: &[
            ParamDescriptor {
                name: "voice_id",
                ty: "string",
            },
            ParamDescriptor {
                name: "text",
                ty: "string",
            },
        ],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("tts"),
        name: "skip",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("tts"),
        name: "clear",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("time"),
        name: "now",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "String",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("time"),
        name: "unix",
        kind: SymbolKind::Fn,
        params: &[],
        return_type: "Int",
        doc: None,
    },
];

pub fn catalog() -> &'static [MethodDescriptor] {
    CATALOG
}

/// Maps a pre-tokenized line position to the matching catalog entry.
///
/// Callers supply `(Range<usize>, SymbolToken)` pairs rather than rhai token types so
/// this function stays free of any `forge-widgets` dependency.
pub fn resolve_symbol_from_tokens(
    tokens: &[(Range<usize>, SymbolToken)],
    line_text: &str,
    col: usize,
) -> Option<&'static MethodDescriptor> {
    let cursor_idx = tokens.iter().position(|(r, _)| r.contains(&col))?;
    let (cursor_range, cursor_kind) = &tokens[cursor_idx];

    if !matches!(
        cursor_kind,
        SymbolToken::FunctionCall | SymbolToken::Identifier
    ) {
        return None;
    }

    let token_name = &line_text[cursor_range.clone()];

    let mut ns_chain: Vec<&str> = Vec::new();
    let mut i = cursor_idx;
    loop {
        if i == 0 {
            break;
        }
        i -= 1;
        match &tokens[i].1 {
            SymbolToken::Namespace => {
                ns_chain.push(&line_text[tokens[i].0.clone()]);
            }
            SymbolToken::Other => {}
            _ => break,
        }
    }
    ns_chain.reverse();

    let sub_chain: &[&str] = if ns_chain
        .first()
        .map(|s| *s == "forge" || *s == "sl")
        .unwrap_or(false)
    {
        &ns_chain[1..]
    } else {
        &ns_chain[..]
    };

    let namespace: Option<&str> = if sub_chain.is_empty() {
        None
    } else {
        Some(sub_chain[sub_chain.len() - 1])
    };

    catalog()
        .iter()
        .find(|d| d.namespace == namespace && d.name == token_name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolToken {
    Namespace,
    FunctionCall,
    Identifier,
    Other,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_globals_get() {
        assert!(
            catalog()
                .iter()
                .any(|d| d.namespace == Some("globals") && d.name == "get"),
            "catalog must contain globals::get"
        );
    }

    #[test]
    fn catalog_globals_get_signature_matches_register_fn() {
        let entry = catalog()
            .iter()
            .find(|d| d.namespace == Some("globals") && d.name == "get")
            .expect("globals::get must be in catalog");
        assert_eq!(entry.params.len(), 1);
        assert_eq!(entry.params[0].name, "key");
        assert_eq!(entry.params[0].ty, "string");
        assert_eq!(entry.return_type, "Variant");
    }

    #[test]
    fn catalog_all_kinds_are_fn_at_beta9() {
        for entry in catalog() {
            assert!(
                matches!(entry.kind, SymbolKind::Fn),
                "entry {}::{} must have kind Fn at beta-9",
                entry.namespace.unwrap_or("(root)"),
                entry.name
            );
        }
    }

    #[test]
    fn resolve_finds_globals_get_in_sl_namespace() {
        let line = "sl::globals::get";
        let tokens: Vec<(Range<usize>, SymbolToken)> = vec![
            (0..2, SymbolToken::Namespace),
            (2..4, SymbolToken::Other),
            (4..11, SymbolToken::Namespace),
            (11..13, SymbolToken::Other),
            (13..16, SymbolToken::FunctionCall),
        ];
        let result = resolve_symbol_from_tokens(&tokens, line, 14);
        let descriptor = result.expect("must find globals::get");
        assert_eq!(descriptor.namespace, Some("globals"));
        assert_eq!(descriptor.name, "get");
        assert_eq!(descriptor.return_type, "Variant");
    }

    #[test]
    fn resolve_returns_none_for_unknown_identifier() {
        let line = "unknown";
        let tokens: Vec<(Range<usize>, SymbolToken)> = vec![(0..7, SymbolToken::FunctionCall)];
        let result = resolve_symbol_from_tokens(&tokens, line, 3);
        assert!(result.is_none());
    }

    #[test]
    fn resolve_returns_none_outside_any_token() {
        let line = "     hello";
        let tokens: Vec<(Range<usize>, SymbolToken)> = vec![(5..10, SymbolToken::FunctionCall)];
        let result = resolve_symbol_from_tokens(&tokens, line, 2);
        assert!(result.is_none());
    }
}
