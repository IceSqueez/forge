use forge_platform_core::{
    OptionListId, QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue,
};
use forge_types::{MessageCode, UserText};

pub(crate) fn text_field(
    key: &str,
    label: MessageCode,
    placeholder: impl Into<UserText>,
) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Text,
        default: None,
        placeholder: Some(placeholder.into()),
        hint: None,
        required: false,
    }
}

pub(crate) fn multiline_field(
    key: &str,
    label: MessageCode,
    placeholder: impl Into<UserText>,
) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, placeholder)
    }
}

pub(crate) fn toggle_field(key: &str, label: MessageCode, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn int_field(
    key: &str,
    label: MessageCode,
    default: i64,
    min: i64,
    max: i64,
) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Int { min, max },
        default: Some(QuickActionFieldValue::Int(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn live_item_field(key: &str, label: MessageCode, list: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(OptionListId::new(
            list,
        ))),
        default: None,
        placeholder: None,
        hint: None,
        required: true,
    }
}

pub(crate) fn choice_field(
    key: &str,
    label: MessageCode,
    default: &str,
    options: &[(&str, MessageCode)],
) -> QuickActionField {
    let options = options
        .iter()
        .map(|(value, label)| QuickActionChoiceOption {
            value: (*value).to_owned(),
            label: label.clone().into(),
        })
        .collect();
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn choice_field_hint(
    key: &str,
    label: MessageCode,
    default: &str,
    options: &[(&str, MessageCode)],
    hint: impl Into<UserText>,
) -> QuickActionField {
    QuickActionField {
        hint: Some(hint.into()),
        ..choice_field(key, label, default, options)
    }
}
