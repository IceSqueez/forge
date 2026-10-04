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
        namespace: Some("latest"),
        name: "get",
        params: &[ParamDescriptor {
            name: "slot",
            ty: "string",
        }],
        return_type: "Map",
        doc: Some("Most recent value across platforms; () when the slot is empty."),
    },
    MethodDescriptor {
        namespace: Some("latest"),
        name: "get",
        params: &[
            ParamDescriptor {
                name: "slot",
                ty: "string",
            },
            ParamDescriptor {
                name: "platform",
                ty: "string",
            },
        ],
        return_type: "Map",
        doc: Some("Value from one platform or service; () when it has none."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "after",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "seconds",
                ty: "int",
            },
        ],
        return_type: "Map",
        doc: Some("Runs the action by id or name later; returns #{id, due_at}."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "after",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "seconds",
                ty: "int",
            },
            ParamDescriptor {
                name: "options",
                ty: "Map",
            },
        ],
        return_type: "Map",
        doc: Some("Options: key, inherit_args, skip_if_late_minutes."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "at",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "when",
                ty: "string",
            },
        ],
        return_type: "Map",
        doc: Some("Runs the action at an RFC 3339 time; returns #{id, due_at}."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "at",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "when",
                ty: "string",
            },
            ParamDescriptor {
                name: "options",
                ty: "Map",
            },
        ],
        return_type: "Map",
        doc: Some("Options: key, inherit_args, skip_if_late_minutes."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "at",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "unix_seconds",
                ty: "int",
            },
        ],
        return_type: "Map",
        doc: Some("Runs the action at a unix time; returns #{id, due_at}."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "at",
        params: &[
            ParamDescriptor {
                name: "action",
                ty: "string",
            },
            ParamDescriptor {
                name: "unix_seconds",
                ty: "int",
            },
            ParamDescriptor {
                name: "options",
                ty: "Map",
            },
        ],
        return_type: "Map",
        doc: Some("Options: key, inherit_args, skip_if_late_minutes."),
    },
    MethodDescriptor {
        namespace: Some("schedule"),
        name: "cancel",
        params: &[ParamDescriptor {
            name: "key",
            ty: "string",
        }],
        return_type: "Bool",
        doc: Some("Cancels the pending run with this exact key; true if one was pending."),
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
