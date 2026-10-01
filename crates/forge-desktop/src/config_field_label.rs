use forge_components::tr;
use forge_registry::FormField;
use gpui::SharedString;

use crate::config_form::ConfigField;

pub(crate) fn config_field_labels(spec: &FormField, out: &mut Vec<SharedString>) {
    out.push(localized_label(descriptor_label(spec)).into());
    if let FormField::Optional { inner, .. } = spec {
        config_field_labels(inner, out);
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
        | FormField::DynamicSelect { label, .. }
        | FormField::DependentSelect { label, .. }
        | FormField::Swatch { label, .. }
        | FormField::Optional { label, .. }
        | FormField::SubChain { label, .. }
        | FormField::CaseList { label, .. } => label,
    }
}

fn localized_label(english: &'static str) -> String {
    match english {
        "Ban type" => tr!("config_field_label_ban_type"),
        "Case sensitive" => tr!("config_field_label_case_sensitive"),
        "Channel" => tr!("config_field_label_channel"),
        "Channel (leave empty for any)" => tr!("config_field_label_channel_filter"),
        "Command phrase" => tr!("config_field_label_command_phrase"),
        "Controller" => tr!("config_field_label_controller"),
        "Controller (leave empty for any)" => tr!("config_field_label_controller_filter"),
        "Decision status" => tr!("config_field_label_decision_status"),
        "Device" => tr!("config_field_label_device"),
        "Device (leave empty for any)" => tr!("config_field_label_device_filter"),
        "Direction" => tr!("config_field_label_direction"),
        "Direction (leave empty for any)" => tr!("config_field_label_direction_filter"),
        "Event Name" => tr!("config_field_label_event_name"),
        "From user (login)" => tr!("config_field_label_from_user"),
        "Guest state (empty = any)" => tr!("config_field_label_guest_state"),
        "Hotkey combo" => tr!("config_field_label_hotkey_combo"),
        "Interval (minutes)" => tr!("config_field_label_interval_minutes"),
        "Match text" => tr!("config_field_label_match_text"),
        "Maximum bits (-1 = unlimited)" => tr!("config_field_label_max_bits"),
        "Minimum bits" => tr!("config_field_label_min_bits"),
        "Minimum chat messages since last fire" => tr!("config_field_label_min_chat_messages"),
        "Minimum donation (cents)" => tr!("config_field_label_min_donation_cents"),
        "Minimum level (1-5)" => tr!("config_field_label_min_level"),
        "Note" => tr!("config_field_label_note"),
        "Note (leave empty for any)" => tr!("config_field_label_note_filter"),
        "Only while live" => tr!("config_field_label_only_while_live"),
        "Program" => tr!("config_field_label_program"),
        "Program (leave empty for any)" => tr!("config_field_label_program_filter"),
        "Redemption status" => tr!("config_field_label_redemption_status"),
        "Reward ID (leave blank to match any reward)" => tr!("config_field_label_reward_id"),
        "Reward title (used only when Reward ID is blank; leave blank to match any)" => {
            tr!("config_field_label_reward_title")
        }
        "Scene" => tr!("config_field_label_scene"),
        "Scene name (leave empty to match any)" => tr!("config_field_label_scene_filter"),
        "Source name" => tr!("config_field_label_source_name"),
        "Source name (leave empty to match any)" => tr!("config_field_label_source_filter"),
        "Use regex" => tr!("config_field_label_use_regex"),
        other => other.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_registry::TriggerRegistry;
    use forge_storage::Language;

    use crate::i18n::install_language;

    const MISSING_KEY_PREFIX: &str = "config_field_label_";

    fn full_trigger_registry() -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        forge_runtime::register_core_triggers(&mut registry).unwrap();
        forge_platform_twitch::register_twitch_triggers(&mut registry).unwrap();
        forge_obs::register_obs_triggers(&mut registry).unwrap();
        forge_vtube::register_vtube_triggers(&mut registry).unwrap();
        forge_midi::register_midi_triggers(&mut registry).unwrap();
        forge_hotkey::register_hotkey_triggers(&mut registry).unwrap();
        forge_platform_youtube::register_youtube_triggers(&mut registry).unwrap();
        forge_platform_kick::register_kick_triggers(&mut registry).unwrap();
        registry
    }

    fn registry_labels(spec: &FormField, out: &mut Vec<(String, &'static str)>) {
        out.push((spec.key().to_owned(), descriptor_label(spec)));
        if let FormField::Optional { inner, .. } = spec {
            registry_labels(inner, out);
        }
    }

    fn every_trigger_label() -> Vec<(String, &'static str)> {
        let registry = full_trigger_registry();
        let mut labels = Vec::new();
        for descriptor in registry.all() {
            for spec in descriptor.config_fields() {
                let mut found = Vec::new();
                registry_labels(&spec, &mut found);
                labels.extend(
                    found
                        .into_iter()
                        .map(|(key, label)| (format!("{}.{key}", descriptor.id()), label)),
                );
            }
        }
        labels
    }

    #[test]
    fn every_trigger_config_label_has_a_translation_in_both_locales() {
        let labels = every_trigger_label();
        assert!(!labels.is_empty());
        for lang in [Language::En, Language::Uk] {
            install_language(lang);
            for (field, english) in &labels {
                let localized = localized_label(english);
                assert!(
                    !localized.starts_with(MISSING_KEY_PREFIX),
                    "{lang:?}: {field} label {english:?} maps to a key missing from the bundle: {localized}"
                );
            }
        }
    }

    #[test]
    fn every_trigger_config_label_is_known_to_the_mapper() {
        install_language(Language::Uk);
        for (field, english) in every_trigger_label() {
            assert_ne!(
                localized_label(english),
                english,
                "{field} label {english:?} has no translation mapping and renders in English under uk"
            );
        }
    }

    #[test]
    fn optional_wrapper_yields_its_own_label_followed_by_the_inner_field_label() {
        install_language(Language::En);
        let spec = FormField::Optional {
            key: "only_live",
            label: "Only while live",
            inner: Box::new(FormField::Text {
                key: "pattern",
                label: "Match text",
                placeholder: "",
            }),
        };
        let mut labels = Vec::new();
        config_field_labels(&spec, &mut labels);
        assert_eq!(
            labels,
            vec![
                SharedString::from(tr!("config_field_label_only_while_live")),
                SharedString::from(tr!("config_field_label_match_text")),
            ]
        );
    }

    #[test]
    fn label_without_a_mapping_is_shown_verbatim() {
        install_language(Language::Uk);
        assert_eq!(localized_label("Brand new label"), "Brand new label");
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
}
