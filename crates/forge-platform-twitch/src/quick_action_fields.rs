use forge_platform_core::{
    QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue,
};

pub(crate) fn text_field(key: &str, label: &str, placeholder: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Text,
        default: None,
        placeholder: Some(placeholder.to_owned()),
        hint: None,
        required: false,
    }
}

pub(crate) fn multiline_field(key: &str, label: &str, placeholder: &str) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, placeholder)
    }
}

pub(crate) fn toggle_field(key: &str, label: &str, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn int_field(
    key: &str,
    label: &str,
    default: i64,
    min: i64,
    max: i64,
) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Int { min, max },
        default: Some(QuickActionFieldValue::Int(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn choice_field(
    key: &str,
    label: &str,
    default: &str,
    options: &[(&str, &str)],
) -> QuickActionField {
    let options = options
        .iter()
        .map(|(value, label)| QuickActionChoiceOption {
            value: (*value).to_owned(),
            label: (*label).to_owned(),
        })
        .collect();
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn choice_field_hint(
    key: &str,
    label: &str,
    default: &str,
    options: &[(&str, &str)],
    hint: &str,
) -> QuickActionField {
    QuickActionField {
        hint: Some(hint.to_owned()),
        ..choice_field(key, label, default, options)
    }
}
