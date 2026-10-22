use forge_components::tr_find;
use forge_registry::FormField;
use gpui::SharedString;

use crate::config_form::ConfigField;

const LABEL_PREFIX: &str = "field_label_";
const OPTIONAL_PREFIX: &str = "field_optional_";
const PLACEHOLDER_PREFIX: &str = "field_placeholder_";
const KIND_NAMESPACE_SEPARATOR: char = '.';

pub(crate) fn config_field_labels(kind_id: &str, spec: &FormField, out: &mut Vec<SharedString>) {
    out.push(field_label(kind_id, spec).into());
    if let FormField::Optional { inner, .. } = spec {
        config_field_labels(kind_id, inner, out);
    }
}

pub(crate) fn row_label(
    labels: &[SharedString],
    index: usize,
    field: &ConfigField,
) -> SharedString {
    labels
        .get(index)
        .cloned()
        .unwrap_or_else(|| SharedString::from(field.key().to_owned()))
}

pub(crate) fn form_field_label(kind_id: &str, fields: &[FormField], key: &str) -> Option<String> {
    fields
        .iter()
        .find_map(|spec| declaring_spec(spec, key))
        .map(|spec| short_label(&field_label(kind_id, spec)))
        .filter(|label| !label.is_empty())
}

pub(crate) fn field_label(kind_id: &str, spec: &FormField) -> String {
    let prefix = match spec {
        FormField::Optional { .. } => OPTIONAL_PREFIX,
        _ => LABEL_PREFIX,
    };
    scoped_copy(prefix, kind_id, spec.key()).unwrap_or_else(|| descriptor_label(spec).to_owned())
}

pub(crate) fn field_placeholder(kind_id: &str, key: &str, english: &str) -> String {
    if english.is_empty() {
        return String::new();
    }
    scoped_copy(PLACEHOLDER_PREFIX, kind_id, key).unwrap_or_else(|| english.to_owned())
}

fn scoped_copy(prefix: &str, kind_id: &str, key: &str) -> Option<String> {
    let namespace = kind_id
        .split_once(KIND_NAMESPACE_SEPARATOR)
        .map_or(kind_id, |(namespace, _)| namespace);
    [
        format!("{prefix}{kind_id}__{key}"),
        format!("{prefix}{namespace}__{key}"),
        format!("{prefix}{key}"),
    ]
    .iter()
    .find_map(|id| tr_find(id, None))
}

fn short_label(label: &str) -> String {
    label
        .split_once(" (")
        .map_or(label, |(name, _)| name)
        .trim()
        .to_owned()
}

fn declaring_spec<'a>(spec: &'a FormField, key: &str) -> Option<&'a FormField> {
    match spec {
        FormField::Optional { inner, .. } => {
            declaring_spec(inner, key).or_else(|| (spec.key() == key).then_some(spec))
        }
        FormField::UnitAmount { unit_key, .. } if *unit_key == key => Some(spec),
        _ => (spec.key() == key).then_some(spec),
    }
}

fn descriptor_label(spec: &FormField) -> &'static str {
    match spec {
        FormField::Text { label, .. }
        | FormField::TextArea { label, .. }
        | FormField::Code { label, .. }
        | FormField::Integer { label, .. }
        | FormField::Slider { label, .. }
        | FormField::Toggle { label, .. }
        | FormField::FilePicker { label, .. }
        | FormField::DateTime { label, .. }
        | FormField::Select { label, .. }
        | FormField::UnitAmount { label, .. }
        | FormField::Duration { label, .. }
        | FormField::DynamicSelect { label, .. }
        | FormField::DependentSelect { label, .. }
        | FormField::Swatch { label, .. }
        | FormField::Optional { label, .. }
        | FormField::SubChain { label, .. }
        | FormField::CaseList { label, .. } => label,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;
    use std::rc::Rc;

    use super::*;
    use fluent::{FluentBundle, FluentResource};
    use forge_components::install_bundle;
    use forge_storage::Language;

    use crate::i18n::install_language;
    use crate::integrations::tests::every_registry_entry;

    const EN_FTL: &str = include_str!("../locales/en/main.ftl");
    const UK_FTL: &str = include_str!("../locales/uk/main.ftl");
    const FIELD_KEY_PREFIX: &str = "field_";
    const INTENTIONAL_EN_REWORDINGS: &[(&str, &str, &str)] = &[
        (
            "core",
            "Base Datetime (or %var%)",
            "Base date-time (or %var%)",
        ),
        ("core", "Create Parent Directories", "Create parent folders"),
        (
            "core",
            "Directory Path (relative to the files folder)",
            "Folder path (relative to the files folder)",
        ),
        (
            "core",
            "End Index (-1 = to end, exclusive)",
            "End index (-1 = to the end, exclusive)",
        ),
        (
            "core",
            "From (datetime or %var%)",
            "From (date-time or %var%)",
        ),
        ("core", "Include Directories", "Include folders"),
        ("core", "Keep running action", "Keep the running action"),
        (
            "core",
            "Max Length (0 = unbounded)",
            "Max length (0 = unlimited)",
        ),
        ("core", "Search", "Search for"),
        (
            "core",
            "Source (datetime or %var%)",
            "Source (date-time or %var%)",
        ),
        ("core", "Source String", "Source text"),
        (
            "core",
            "Start Index (0-based, chars)",
            "Start index (0-based, characters)",
        ),
        ("core", "To (datetime or %var%)", "To (date-time or %var%)"),
        (
            "donation",
            "Donation service (leave empty to match any)",
            "Filter by donation service",
        ),
        ("midi", "Channel (leave empty for any)", "Filter by channel"),
        (
            "midi",
            "Controller (leave empty for any)",
            "Filter by controller",
        ),
        ("midi", "Device (leave empty for any)", "Filter by device"),
        (
            "midi",
            "Direction (leave empty for any)",
            "Filter by direction",
        ),
        ("midi", "Note (leave empty for any)", "Filter by note"),
        ("midi", "Program (leave empty for any)", "Filter by program"),
        (
            "obs",
            "Scene name (leave empty to match any)",
            "Filter by scene",
        ),
        (
            "obs",
            "Source name (leave empty to match any)",
            "Filter by source",
        ),
        (
            "timer",
            "Minimum chat messages since last fire",
            "Min chat messages",
        ),
        (
            "twitch",
            "Maximum bits (-1 = unlimited)",
            "Max bits (-1 = no limit)",
        ),
        ("twitch", "Minimum bits", "Min bits"),
        ("twitch", "Minimum donation (cents)", "Min donation (cents)"),
        ("twitch", "Minimum level (1-5)", "Min level (1-5)"),
        ("twitch", "Reward ID", "Reward"),
        (
            "twitch",
            "Reward ID (leave blank to match any reward)",
            "Reward (empty = any)",
        ),
        (
            "twitch",
            "Reward title (used only when Reward ID is blank; leave blank to match any)",
            "Reward title (if no reward chosen)",
        ),
    ];

    const PRECEDENCE_FTL: &str = "\
field_label_demo_step_alpha__k1 = Kind
field_label_demo__k1 = Namespace
field_label_k1 = Global
field_label_demo__k2 = Namespace
field_label_k2 = Global
field_label_k3 = Global
";

    fn install_ftl(source: &str) {
        let mut bundle = FluentBundle::new(vec!["en-US".parse().unwrap()]);
        bundle
            .add_resource(FluentResource::try_new(source.to_owned()).unwrap())
            .unwrap();
        install_bundle(Rc::new(bundle));
    }

    fn text(key: &'static str, label: &'static str) -> FormField {
        FormField::Text {
            key,
            label,
            placeholder: "",
        }
    }

    fn midi_channel_filter() -> FormField {
        FormField::Optional {
            key: "channel",
            label: "Channel (leave empty for any)",
            inner: Box::new(FormField::Integer {
                key: "channel",
                label: "Channel",
                min: 0,
                max: 15,
            }),
        }
    }

    fn flatten(spec: FormField, out: &mut Vec<FormField>) {
        if let FormField::Optional { inner, .. } = &spec {
            flatten((**inner).clone(), out);
        }
        out.push(spec);
    }

    struct RegistryField {
        kind_id: String,
        is_trigger: bool,
        spec: FormField,
    }

    impl RegistryField {
        fn namespace(&self) -> &str {
            self.kind_id.split('.').next().unwrap_or_default()
        }

        fn has_bundle_copy(&self) -> bool {
            scoped_copy(copy_prefix(&self.spec), &self.kind_id, self.spec.key()).is_some()
        }

        fn describe(&self) -> String {
            format!("{} {}", self.kind_id, self.spec.key())
        }
    }

    async fn every_registry_field() -> Vec<RegistryField> {
        let (triggers, sub_actions) = every_registry_entry().await;
        let mut owners: Vec<(String, bool, Vec<FormField>)> = triggers
            .all()
            .map(|descriptor| (descriptor.id().to_owned(), true, descriptor.config_fields()))
            .collect();
        owners.extend(
            sub_actions
                .all()
                .map(|runner| (runner.id().to_owned(), false, runner.config_fields())),
        );
        let mut fields = Vec::new();
        for (kind_id, is_trigger, specs) in owners {
            for spec in specs {
                let mut flat = Vec::new();
                flatten(spec, &mut flat);
                fields.extend(flat.into_iter().map(|spec| RegistryField {
                    kind_id: kind_id.clone(),
                    is_trigger,
                    spec,
                }));
            }
        }
        fields
    }

    fn copy_prefix(spec: &FormField) -> &'static str {
        match spec {
            FormField::Optional { .. } => OPTIONAL_PREFIX,
            _ => LABEL_PREFIX,
        }
    }

    fn field_entries(source: &str) -> Vec<(&str, &str)> {
        source
            .lines()
            .filter(|line| line.starts_with(FIELD_KEY_PREFIX))
            .filter_map(|line| line.split_once(" = "))
            .collect()
    }

    #[test]
    fn row_label_uses_the_label_at_the_row_index_or_falls_back_to_the_field_key() {
        let labels = vec![SharedString::from("Канал")];
        let field = ConfigField::Hint {
            key: "cases".to_owned(),
        };
        for (index, expected) in [(0, "Канал"), (1, "cases")] {
            assert_eq!(
                row_label(&labels, index, &field),
                SharedString::from(expected)
            );
        }
    }

    fn labelled_form() -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "scene",
                label: "Scene",
                placeholder: "",
            },
            FormField::Text {
                key: "from_user",
                label: "From user (login)",
                placeholder: "",
            },
            FormField::Optional {
                key: "only_live",
                label: "Only while live",
                inner: Box::new(FormField::Text {
                    key: "pattern",
                    label: "Match text",
                    placeholder: "",
                }),
            },
            FormField::UnitAmount {
                key: "amount",
                label: "Timeout length",
                unit_key: "amount_unit",
                bounds: forge_registry::UnitAmountBounds {
                    min: 1,
                    max_base_units: 60,
                    units: &[],
                },
            },
            FormField::Text {
                key: "hint_only",
                label: " (optional)",
                placeholder: "",
            },
        ]
    }

    #[test]
    fn form_field_label_names_any_declared_key_by_its_short_english_label() {
        install_language(Language::En);
        let fields = labelled_form();
        for (key, expected) in [
            ("scene", Some("Scene")),
            ("from_user", Some("From user")),
            ("only_live", Some("Only while live")),
            ("pattern", Some("Match text")),
            ("amount", Some("Timeout length")),
            ("amount_unit", Some("Timeout length")),
            ("hint_only", None),
            ("not_on_form", None),
        ] {
            assert_eq!(
                form_field_label("test.step", &fields, key).as_deref(),
                expected,
                "key {key:?}"
            );
        }
    }

    #[test]
    fn field_label_prefers_kind_then_namespace_then_global_then_the_descriptor_english() {
        install_ftl(PRECEDENCE_FTL);
        for (kind_id, key, expected) in [
            ("demo.step.alpha", "k1", "Kind"),
            ("demo.step.beta", "k1", "Namespace"),
            ("demo.step.alpha", "k2", "Namespace"),
            ("other.step", "k2", "Global"),
            ("demo.step.alpha", "k3", "Global"),
            ("demo.step.alpha", "k4", "English"),
            ("demo", "k1", "Namespace"),
        ] {
            assert_eq!(
                field_label(kind_id, &text(key, "English")),
                expected,
                "{kind_id} {key}"
            );
        }
    }

    #[test]
    fn real_catalog_kind_override_beats_the_namespace_default_and_globals_apply_anywhere() {
        install_language(Language::Uk);
        for (kind_id, key, expected) in [
            (
                "core.string.regex_match",
                "into_var",
                "Змінна для результату (bool)",
            ),
            ("core.math.evaluate", "into_var", "Змінна для результату"),
            ("kick.chat", "case_sensitive", "З урахуванням регістру"),
            ("kick.chat", "into_var", "English"),
        ] {
            assert_eq!(
                field_label(kind_id, &text(key, "English")),
                expected,
                "{kind_id} {key}"
            );
        }
    }

    #[test]
    fn optional_toggle_reads_its_own_copy_and_the_inner_field_keeps_the_value_label() {
        install_language(Language::En);
        let mut labels = Vec::new();
        config_field_labels("midi.note_on", &midi_channel_filter(), &mut labels);
        assert_eq!(
            labels,
            vec![
                SharedString::from("Filter by channel"),
                SharedString::from("Channel"),
            ]
        );
    }

    #[test]
    fn form_field_label_names_a_shared_optional_key_by_the_inner_value_field() {
        install_language(Language::En);
        assert_eq!(
            form_field_label("midi.note_on", &[midi_channel_filter()], "channel").as_deref(),
            Some("Channel")
        );
    }

    #[test]
    fn field_placeholder_keeps_an_empty_english_placeholder_empty_even_with_a_kind_entry() {
        install_language(Language::En);
        assert_eq!(field_placeholder("core.args.set", "value", ""), "");
    }

    #[test]
    fn field_placeholder_reads_the_kind_entry_and_falls_back_to_english() {
        install_language(Language::Uk);
        let localized = field_placeholder("core.args.set", "value", "hello");
        assert_ne!(localized, "hello");
        assert_eq!(field_placeholder("kick.chat", "value", "hello"), "hello");
    }

    #[test]
    fn field_copy_keys_are_identical_in_en_and_uk() {
        let keys = |source| -> BTreeSet<&str> {
            field_entries(source)
                .into_iter()
                .map(|(key, _)| key)
                .collect()
        };
        let en = keys(EN_FTL);
        let uk = keys(UK_FTL);
        assert!(!en.is_empty());
        assert_eq!(
            en.symmetric_difference(&uk).collect::<Vec<_>>(),
            Vec::<&&str>::new()
        );
    }

    #[test]
    fn field_copy_values_carry_no_em_or_en_dashes() {
        for source in [EN_FTL, UK_FTL] {
            for (key, value) in field_entries(source) {
                assert!(
                    !value.contains(['\u{2014}', '\u{2013}', '\u{2012}', '\u{2015}']),
                    "{key} = {value}"
                );
            }
        }
    }

    #[tokio::test]
    async fn every_registered_step_and_trigger_field_has_uk_copy() {
        let fields = every_registry_field().await;
        install_language(Language::Uk);
        let untranslated: Vec<String> = fields
            .iter()
            .filter(|field| !field.has_bundle_copy())
            .map(|field| {
                let kind = if field.is_trigger { "trigger" } else { "step" };
                format!("{kind} {}", field.describe())
            })
            .collect();
        assert!(
            untranslated.is_empty(),
            "{} fields without uk copy: {untranslated:#?}",
            untranslated.len()
        );
    }

    #[tokio::test]
    async fn every_registry_field_resolves_in_en_to_its_own_descriptor_label() {
        let fields = every_registry_field().await;
        install_language(Language::En);
        let mismatched: Vec<String> = fields
            .iter()
            .filter_map(|field| {
                let english = descriptor_label(&field.spec);
                let resolved = field_label(&field.kind_id, &field.spec);
                let reworded = INTENTIONAL_EN_REWORDINGS.contains(&(
                    field.namespace(),
                    english,
                    resolved.as_str(),
                ));
                (!reworded && resolved.to_lowercase() != english.to_lowercase())
                    .then(|| format!("{}: {english:?} -> {resolved:?}", field.describe()))
            })
            .collect();
        assert!(
            mismatched.is_empty(),
            "{} mismatches: {mismatched:#?}",
            mismatched.len()
        );
    }

    #[tokio::test]
    async fn every_registry_placeholder_resolves_in_en_to_its_own_descriptor_placeholder() {
        let fields = every_registry_field().await;
        install_language(Language::En);
        let mismatched: Vec<String> = fields
            .iter()
            .filter_map(|field| match &field.spec {
                FormField::Text { placeholder, .. } => {
                    let resolved = field_placeholder(&field.kind_id, field.spec.key(), placeholder);
                    (resolved != *placeholder)
                        .then(|| format!("{}: {placeholder:?} -> {resolved:?}", field.describe()))
                }
                _ => None,
            })
            .collect();
        assert!(mismatched.is_empty(), "{mismatched:#?}");
    }
}
