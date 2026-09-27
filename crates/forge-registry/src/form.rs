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
    DateTime {
        key: &'static str,
        label: &'static str,
    },
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
    DependentSelect {
        key: &'static str,
        label: &'static str,
        options_prefix: &'static str,
        depends_on: &'static str,
    },
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
    SubChain {
        key: &'static str,
        label: &'static str,
    },
    CaseList {
        key: &'static str,
        label: &'static str,
    },
}

impl FormField {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Text { key, .. }
            | Self::TextArea { key, .. }
            | Self::Code { key, .. }
            | Self::Integer { key, .. }
            | Self::Slider { key, .. }
            | Self::Toggle { key, .. }
            | Self::FilePicker { key, .. }
            | Self::DateTime { key, .. }
            | Self::Select { key, .. }
            | Self::DynamicSelect { key, .. }
            | Self::DependentSelect { key, .. }
            | Self::Swatch { key, .. }
            | Self::Optional { key, .. }
            | Self::SubChain { key, .. }
            | Self::CaseList { key, .. } => key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_the_config_key_each_field_shape_writes_to() {
        let cases = vec![
            (
                FormField::Text {
                    key: "alias_name",
                    label: "Alias name",
                    placeholder: "e.g. %user%",
                },
                "alias_name",
            ),
            (
                FormField::Code {
                    key: "body",
                    label: "Script",
                    language: CodeLanguage::Rhai,
                },
                "body",
            ),
            (
                FormField::Slider {
                    key: "volume_db",
                    label: "Volume",
                    min: -60,
                    max: 6,
                    unit: "dB",
                },
                "volume_db",
            ),
            (
                FormField::Select {
                    key: "mode",
                    label: "Mode",
                    options: &["first", "last"],
                },
                "mode",
            ),
            (
                FormField::DynamicSelect {
                    key: "engine_id",
                    label: "Engine ID",
                    options_key: "tts.engine_ids",
                },
                "engine_id",
            ),
            (
                FormField::DependentSelect {
                    key: "voice_id",
                    label: "Voice ID",
                    options_prefix: "tts.voices",
                    depends_on: "engine_id",
                },
                "voice_id",
            ),
            (
                FormField::Swatch {
                    key: "accent",
                    label: "Accent",
                    options: &["mauve"],
                },
                "accent",
            ),
            (
                FormField::Optional {
                    key: "use_voice",
                    label: "Override voice",
                    inner: Box::new(FormField::Text {
                        key: "voice_id",
                        label: "Voice ID",
                        placeholder: "",
                    }),
                },
                "use_voice",
            ),
        ];

        for (field, expected) in cases {
            assert_eq!(
                field.key(),
                expected,
                "{field:?} must name its own config key, not a sibling string field"
            );
        }
    }
}
