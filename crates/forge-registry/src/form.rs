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
    /// Stores a whole number; `unit` suffixes the value in the label.
    Slider {
        key: &'static str,
        label: &'static str,
        min: i64,
        max: i64,
        unit: &'static str,
    },
    Toggle {
        key: &'static str,
        label: &'static str,
    },
    FilePicker {
        key: &'static str,
        label: &'static str,
    },
    /// Free-text `%var%` field paired with a calendar+time picker; written as an RFC 3339 string.
    DateTime {
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
    /// Static choices naming palette colors, presented as swatches rather than a dropdown.
    Swatch {
        key: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    },
    Optional {
        key: &'static str,
        label: &'static str,
        inner: Box<FormField>,
    },
    /// Names a config key holding one nested sub-chain; the renderer offers a drill-in affordance rather than a text input.
    SubChain {
        key: &'static str,
        label: &'static str,
    },
    /// Names a config key holding an ordered list of labeled branches, each pairing a match value with a nested sub-chain.
    CaseList {
        key: &'static str,
        label: &'static str,
    },
}
