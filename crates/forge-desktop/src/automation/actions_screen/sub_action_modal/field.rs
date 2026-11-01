use crate::shared::config_form::{ConfigField, choice_entries};
use crate::shared::option_choices::ChoiceOption;

pub(super) struct SelectEntries {
    pub(super) options: Vec<ChoiceOption>,
    pub(super) selected: String,
    pub(super) accepts_typed_value: bool,
}

pub(super) fn select_entries(fields: &[ConfigField], key: &str) -> Option<SelectEntries> {
    let (options, selected) = choice_entries(fields, key)?;
    let accepts_typed_value = fields.iter().any(|field| {
        matches!(
            field,
            ConfigField::Choice { key: k, options_key: Some(_), .. } if k == key
        )
    });
    Some(SelectEntries {
        options: options.to_vec(),
        selected: selected.to_owned(),
        accepts_typed_value,
    })
}
