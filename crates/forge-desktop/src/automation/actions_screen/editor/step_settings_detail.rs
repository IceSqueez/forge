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

#[cfg(test)]
mod tests {
    use forge_registry::{
        CodeKind, CodeLanguage, RunContext, StepCardDetail, StepCatalogEntry, SubActionCategory,
    };
    use forge_storage::Language;
    use forge_types::{ArgStack, SubActionStep, SubActionTelemetry};

    use super::*;
    use crate::automation::actions_screen::editor::step_presentation::step_card_detail;

    const KIND: &str = "midi.stub_step";
    const DIRECTION: &str = "direction";
    const GREETING: &str = "greeting";
    const LOUD: &str = "loud";

    #[derive(Default)]
    struct Declared {
        fields: Vec<FormField>,
        defaults: SubActionConfig,
        own_detail: Option<String>,
    }

    impl StepCatalogEntry for Declared {
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Logic
        }
        fn label(&self) -> &str {
            KIND
        }
        fn summary(&self) -> &str {
            KIND
        }
        fn search_text(&self) -> &str {
            KIND
        }
        fn icon_name(&self) -> &str {
            "bolt"
        }
        fn card_detail(&self, _config: &SubActionConfig) -> Option<StepCardDetail> {
            self.own_detail.clone().map(StepCardDetail::Text)
        }
    }

    #[async_trait::async_trait]
    impl SubActionRunner for Declared {
        fn id(&self) -> &str {
            KIND
        }

        fn default_config(&self) -> SubActionConfig {
            self.defaults.clone()
        }

        fn config_fields(&self) -> Vec<FormField> {
            self.fields.clone()
        }

        async fn execute(
            &self,
            _: &SubActionConfig,
            _: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            unreachable!("a card detail never runs a step")
        }
    }

    fn direction() -> FormField {
        FormField::Select {
            key: DIRECTION,
            label: "Way",
            options: &["input", "output"],
        }
    }

    fn text(key: &'static str, label: &'static str) -> FormField {
        FormField::Text {
            key,
            label,
            placeholder: "",
        }
    }

    fn config(pairs: &[(&str, Variant)]) -> SubActionConfig {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn string(text: &str) -> Variant {
        Variant::String(text.to_owned())
    }

    fn detail(runner: &Declared, stored: &SubActionConfig) -> Option<String> {
        crate::app::i18n::install_language(Language::En);
        declared_settings_detail(runner, stored, &HashMap::new())
    }

    #[test]
    fn the_card_names_each_required_setting_with_its_label_in_declared_order() {
        let runner = Declared {
            fields: vec![
                direction(),
                text(GREETING, "Greeting"),
                FormField::Toggle {
                    key: LOUD,
                    label: "Loud",
                },
            ],
            ..Declared::default()
        };
        let stored = config(&[
            (LOUD, Variant::Bool(true)),
            (GREETING, string("hi %user%")),
            (DIRECTION, string("input")),
        ]);

        assert_eq!(
            detail(&runner, &stored).as_deref(),
            Some("Direction: Input · Greeting: hi %user% · Loud: on"),
        );
    }

    #[test]
    fn a_setting_the_step_never_stored_shows_the_runners_default() {
        let runner = Declared {
            fields: vec![direction()],
            defaults: config(&[(DIRECTION, string("output"))]),
            ..Declared::default()
        };

        assert_eq!(
            detail(&runner, &SubActionConfig::new()).as_deref(),
            Some("Direction: Output"),
        );
    }

    #[test]
    fn optional_and_long_form_settings_stay_off_the_card() {
        let runner = Declared {
            fields: vec![
                FormField::Optional {
                    key: "extra",
                    label: "Extra",
                    inner: Box::new(text("extra", "Extra")),
                },
                FormField::TextArea {
                    key: "body",
                    label: "Body",
                },
                FormField::Code {
                    key: "script",
                    label: "Script",
                    language: CodeLanguage::Rhai,
                    kind: CodeKind::Script,
                },
                FormField::SubChain {
                    key: "then",
                    label: "Then",
                },
                FormField::CaseList {
                    key: "cases",
                    label: "Cases",
                },
                text(GREETING, "Greeting"),
            ],
            ..Declared::default()
        };
        let stored = config(&[
            ("extra", string("filled")),
            ("body", string("filled")),
            ("script", string("filled")),
            ("then", string("filled")),
            ("cases", string("filled")),
            (GREETING, string("hello")),
        ]);

        assert_eq!(detail(&runner, &stored).as_deref(), Some("Greeting: hello"));
    }

    #[test]
    fn a_blank_setting_is_left_off_the_card() {
        let runner = Declared {
            fields: vec![text("first", "First"), text(GREETING, "Greeting")],
            ..Declared::default()
        };
        let stored = config(&[("first", string(" \t ")), (GREETING, string("hello"))]);

        assert_eq!(detail(&runner, &stored).as_deref(), Some("Greeting: hello"));
    }

    #[test]
    fn a_step_with_nothing_left_to_show_has_no_detail() {
        for (label, runner) in [
            ("no declared fields", Declared::default()),
            (
                "only a blank setting",
                Declared {
                    fields: vec![text(GREETING, "Greeting")],
                    ..Declared::default()
                },
            ),
            (
                "only a long-form setting",
                Declared {
                    fields: vec![FormField::TextArea {
                        key: "body",
                        label: "Body",
                    }],
                    ..Declared::default()
                },
            ),
        ] {
            let stored = config(&[("body", string("filled"))]);

            assert_eq!(detail(&runner, &stored), None, "{label}");
        }
    }

    #[test]
    fn a_runners_own_card_detail_wins_over_its_declared_settings() {
        crate::app::i18n::install_language(Language::En);
        let runner = Declared {
            fields: vec![text(GREETING, "Greeting")],
            own_detail: Some("Runner wording".to_owned()),
            ..Declared::default()
        };
        let step = SubActionStep {
            kind_id: KIND.to_owned(),
            config: config(&[(GREETING, string("hello"))]),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        };

        assert_eq!(
            step_card_detail(Some(&runner), &step, &HashMap::new()).as_deref(),
            Some("Runner wording"),
        );
    }
}
