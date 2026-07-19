#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Rhai,
    Json,
}

#[derive(Debug, Clone)]
pub enum FormField {
    Text {
        key: &'static str,
        label: &'static str,
        placeholder: &'static str,
    },
    TextArea {
        key: &'static str,
        label: &'static str,
    },
    /// Multi-line editor with syntax highlighting for the named language.
    Code {
        key: &'static str,
        label: &'static str,
        language: CodeLanguage,
    },
    Integer {
        key: &'static str,
        label: &'static str,
        min: i64,
        max: i64,
    },
    Toggle {
        key: &'static str,
        label: &'static str,
    },
    FilePicker {
        key: &'static str,
        label: &'static str,
    },
    /// Static enum-like choices. Use `DynamicSelect` for runtime-supplied option lists.
    Select {
        key: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    },
    DynamicSelect {
        key: &'static str,
        label: &'static str,
        options_key: &'static str,
    },
    Optional {
        key: &'static str,
        label: &'static str,
        inner: Box<FormField>,
    },
    /// Names a config key holding exactly one nested sub-chain (the canonical
    /// `Array`-of-`Object` step form). The editor authors its steps in the
    /// recursive step-list surface, so the renderer offers a drill-in affordance
    /// rather than a text input.
    SubChain {
        key: &'static str,
        label: &'static str,
    },
    /// Names a config key holding an ordered list of labeled branches, each
    /// pairing a single-value match input with one nested sub-chain. The
    /// renderer offers per-row add/remove/reorder, a single-value match input,
    /// and a drill-in affordance into each branch.
    CaseList {
        key: &'static str,
        label: &'static str,
    },
}
