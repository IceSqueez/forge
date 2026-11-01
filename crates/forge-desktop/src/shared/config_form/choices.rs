use std::collections::HashMap;

use forge_components::{Picker, PickerEvent, PickerItem, PickerLabels, tr};
use forge_registry::FormField;
use forge_runtime::DONATION_PROVIDER_OPTIONS_KEY;
use gpui::{App, Context, Entity, SharedString, Subscription, Window, prelude::*};

use super::fold::read_text;
use super::{ConfigField, FieldConfig, collect_field_values};
use crate::integrations::donation_services::donation_provider_options;
use crate::shared::option_choices::{ChoiceOption, ChoiceOptions, names_integration_data};

pub(crate) fn choice_entries<'a>(
    fields: &'a [ConfigField],
    key: &str,
) -> Option<(&'a [ChoiceOption], &'a str)> {
    fields.iter().find_map(|field| match field {
        ConfigField::Choice {
            key: k,
            options,
            selected,
            ..
        } if k == key => Some((options.as_slice(), selected.as_str())),
        ConfigField::UnitAmount {
            unit_picker_key,
            units,
            unit,
            ..
        } if unit_picker_key == key => Some((units.as_slice(), unit.as_str())),
        _ => None,
    })
}

struct ChoicePopover {
    key: String,
    picker: Entity<Picker>,
    _sub: Subscription,
}

fn static_choice_options(options_key: &str) -> Option<Vec<ChoiceOption>> {
    (options_key == DONATION_PROVIDER_OPTIONS_KEY).then(donation_provider_options)
}

pub(super) fn has_options_provider(options_key: &str) -> bool {
    static_choice_options(options_key).is_some() || names_integration_data(options_key)
}

struct ChoiceOptionsKey {
    field_key: String,
    options_key: String,
}

fn choice_options_keys(specs: &[FormField]) -> Vec<ChoiceOptionsKey> {
    let mut out = Vec::new();
    for spec in specs {
        push_choice_options_key(spec, &mut out);
    }
    out
}

fn push_choice_options_key(spec: &FormField, out: &mut Vec<ChoiceOptionsKey>) {
    match spec {
        FormField::DynamicSelect {
            key, options_key, ..
        } if has_options_provider(options_key) => out.push(ChoiceOptionsKey {
            field_key: (*key).to_owned(),
            options_key: (*options_key).to_owned(),
        }),
        FormField::Optional { inner, .. } => push_choice_options_key(inner, out),
        _ => {}
    }
}

#[derive(Default)]
pub(crate) struct CollectionChoices {
    options_keys: Vec<ChoiceOptionsKey>,
    popover: Option<ChoicePopover>,
}

impl CollectionChoices {
    pub(crate) fn for_specs(specs: &[FormField]) -> Self {
        Self {
            options_keys: choice_options_keys(specs),
            popover: None,
        }
    }

    pub(crate) fn option_keys(&self) -> Vec<String> {
        self.options_keys
            .iter()
            .map(|keys| keys.options_key.clone())
            .filter(|key| names_integration_data(key))
            .collect()
    }

    pub(crate) fn refresh(
        &self,
        config_fields: &mut [ConfigField],
        options: &ChoiceOptions,
        blank: Option<&str>,
    ) {
        for field in config_fields {
            let ConfigField::Choice {
                key,
                options: offered,
                ..
            } = field
            else {
                continue;
            };
            let Some(choice_field) = self.options_keys.iter().find(|f| f.field_key == *key) else {
                continue;
            };
            let mut next: Vec<ChoiceOption> = blank
                .map(|label| ChoiceOption::new(String::new(), label))
                .into_iter()
                .collect();
            next.extend(
                options
                    .get(&choice_field.options_key)
                    .cloned()
                    .or_else(|| static_choice_options(&choice_field.options_key))
                    .unwrap_or_default(),
            );
            *offered = next;
        }
    }

    pub(crate) fn toggle<V: 'static>(
        &mut self,
        config_fields: &[ConfigField],
        key: String,
        on_event: fn(&mut V, Entity<Picker>, &PickerEvent, &mut Context<V>),
        window: &mut Window,
        cx: &mut Context<V>,
    ) {
        if self.popover.as_ref().is_some_and(|open| open.key == key) {
            self.popover = None;
            return;
        }
        self.popover = open_choice_popover(config_fields, key, on_event, window, cx);
    }

    pub(crate) fn close(&mut self) {
        self.popover = None;
    }

    pub(crate) fn take_open_key(&mut self) -> Option<String> {
        self.popover.take().map(|open| open.key)
    }

    pub(crate) fn active(&self) -> Option<(String, Entity<Picker>)> {
        self.popover
            .as_ref()
            .map(|open| (open.key.clone(), open.picker.clone()))
    }
}

fn open_choice_popover<V: 'static>(
    fields: &[ConfigField],
    key: String,
    on_event: fn(&mut V, Entity<Picker>, &PickerEvent, &mut Context<V>),
    window: &mut Window,
    cx: &mut Context<V>,
) -> Option<ChoicePopover> {
    let (options, selected) = choice_entries(fields, &key)?;
    let items: Vec<PickerItem> = options
        .iter()
        .map(|ChoiceOption { value, label }| PickerItem {
            id: SharedString::from(value.clone()),
            label: SharedString::from(label.clone()),
            sublabel: None,
            icon: None,
        })
        .collect();
    let labels = PickerLabels {
        placeholder: tr!("config_form_choice_search").into(),
        empty: tr!("config_form_choice_empty").into(),
        loading: tr!("widget_picker_loading").into(),
    };
    let current = Some(SharedString::from(selected.to_owned()));
    let picker = cx.new(|cx| {
        Picker::new(labels, items, cx)
            .with_current(current)
            .with_custom_entry(tr!("config_form_choice_custom").into())
    });
    let sub = cx.subscribe(&picker, on_event);
    picker.update(cx, |picker, cx| picker.focus(window, cx));
    Some(ChoicePopover {
        key,
        picker,
        _sub: sub,
    })
}

pub(crate) fn dependent_options(
    map: &HashMap<String, Vec<ChoiceOption>>,
    options_prefix: &str,
    sibling_value: &str,
) -> Vec<ChoiceOption> {
    if sibling_value.is_empty() {
        return Vec::new();
    }
    map.get(&dependent_options_key(options_prefix, sibling_value))
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn dependent_options_key(options_prefix: &str, sibling_value: &str) -> String {
    format!("{options_prefix}.{sibling_value}")
}

pub(crate) fn resolve_dependent_choices(
    fields: &mut [ConfigField],
    map: &HashMap<String, Vec<ChoiceOption>>,
    cx: &App,
) {
    let mut values = FieldConfig::new();
    collect_field_values(fields, &mut values, cx);
    for field in fields.iter_mut() {
        if let ConfigField::Choice {
            options,
            dependency: Some(dependency),
            ..
        } = field
        {
            *options = dependent_options(
                map,
                &dependency.options_prefix,
                &read_text(&values, &dependency.depends_on),
            );
        }
    }
}
