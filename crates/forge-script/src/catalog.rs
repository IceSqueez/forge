#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDescriptor {
    pub name: &'static str,
    pub ty: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub namespace: Option<&'static str>,
    pub name: &'static str,
    pub params: &'static [ParamDescriptor],
    pub return_type: &'static str,
    pub doc: Option<&'static str>,
}

static CATALOG: &[MethodDescriptor] = &[
    MethodDescriptor {
        namespace: None,
        name: "log",
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
        params: &[],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("tts"),
        name: "clear",
        params: &[],
        return_type: "()",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("time"),
        name: "now",
        params: &[],
        return_type: "String",
        doc: None,
    },
    MethodDescriptor {
        namespace: Some("time"),
        name: "unix",
        params: &[],
        return_type: "Int",
        doc: None,
    },
];

pub fn catalog() -> &'static [MethodDescriptor] {
    CATALOG
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
}
