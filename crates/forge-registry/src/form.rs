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
}
