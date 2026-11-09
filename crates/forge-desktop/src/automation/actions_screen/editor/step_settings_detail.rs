use std::collections::HashMap;

use forge_components::tr;
use forge_registry::{FormField, SubActionRunner};
use forge_types::{SubActionConfig, Variant};

use super::step_presentation::option_label_detail;
use crate::automation::amount_scale::AmountScale;
use crate::shared::config_field_label::form_field_label;
use crate::shared::config_form::{dependent_options_key, read_text};
use crate::shared::config_text::{editable_line, shown_one_based};
use crate::shared::option_choices::{ChoiceOption, names_integration_data};
use crate::shared::option_labels::option_label;

const SETTING_SEPARATOR: &str = " · ";
const LABEL_VALUE_SEPARATOR: &str = ": ";

pub(super) fn declared_settings_detail(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> Option<String> {
    let fields = runner.config_fields();
    if fields.is_empty() {
        return None;
    }
    let mut settings = runner.default_config();
    settings.extend(
        config
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    let kind_id = runner.id();
    let shown: Vec<String> = fields
        .iter()
        .filter_map(|spec| {
            let value = shown_setting(kind_id, spec, &settings, select_options)?;
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            let label = form_field_label(kind_id, &fields, spec.key())
                .unwrap_or_else(|| spec.key().to_owned());
            Some(format!("{label}{LABEL_VALUE_SEPARATOR}{value}"))
        })
        .collect();
    (!shown.is_empty()).then(|| shown.join(SETTING_SEPARATOR))
}

fn shown_setting(
    kind_id: &str,
    spec: &FormField,
    settings: &SubActionConfig,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> Option<String> {
    let key = spec.key();
    let stored = read_text(settings, key);
    match spec {
        FormField::Optional { .. }
        | FormField::Code { .. }
        | FormField::TextArea { .. }
        | FormField::SubChain { .. }
        | FormField::CaseList { .. } => None,
        FormField::Text { .. }
        | FormField::Integer { .. }
        | FormField::FilePicker { .. }
        | FormField::DateTime { .. } => Some(stored),
        FormField::OneBasedInteger { .. } => Some(shown_one_based(&stored)),
        FormField::Slider { unit, .. } => {
            (!stored.trim().is_empty()).then(|| format!("{stored}{unit}"))
        }
        FormField::Toggle { .. } => settings.get(key).map(toggle_text),
        FormField::Select { .. } | FormField::Swatch { .. } => {
            (!stored.is_empty()).then(|| option_label(kind_id, key, &stored))
        }
        FormField::DynamicSelect { options_key, .. } => {
            Some(picked_label(options_key, &stored, select_options))
        }
        FormField::DependentSelect {
            options_prefix,
            depends_on,
            ..
        } => {
            let options_key =
                dependent_options_key(options_prefix, &read_text(settings, depends_on));
            Some(option_label_detail(&options_key, &stored, select_options))
        }
        FormField::UnitAmount { .. } | FormField::Duration { .. } => {
            amount_text(spec, key, &stored, settings)
        }
    }
}

fn toggle_text(value: &Variant) -> String {
    match value {
        Variant::Bool(true) => tr!("action_editor_step_setting_on"),
        Variant::Bool(false) => tr!("action_editor_step_setting_off"),
        other => editable_line(other),
    }
}

fn picked_label(
    options_key: &str,
    stored: &str,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> String {
    if !names_integration_data(options_key) {
        return option_label_detail(options_key, stored, select_options);
    }
    select_options
        .get(options_key)
        .and_then(|options| options.iter().find(|option| option.value == stored))
        .map_or_else(|| stored.to_owned(), |option| option.label.clone())
}

fn amount_text(
    spec: &FormField,
    key: &str,
    stored: &str,
    settings: &SubActionConfig,
) -> Option<String> {
    if stored.trim().is_empty() {
        return None;
    }
    let scale = AmountScale::of(spec)?;
    let seed = scale.seed(key, settings);
    let unit = scale
        .unit_options()
        .into_iter()
        .find(|option| option.value == seed.unit)
        .map_or(seed.unit, |option| option.label);
    Some(format!("{} {unit}", seed.amount))
}
