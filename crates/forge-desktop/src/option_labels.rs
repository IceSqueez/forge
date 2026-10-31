use forge_components::{ArgsBuilder, ftl_id, tr_find, tr_lookup};

use crate::config_field_label::scoped_copy;

const OPTION_PREFIX: &str = "opt_";
const DURATION_SECONDS_KEY: &str = "duration_seconds";
const SECONDS_LABEL_KEY: &str = "opt_duration_seconds_value";

pub(crate) fn option_label(kind_id: &str, key: &str, option: &str) -> String {
    seconds_label(key, option)
        .or_else(|| scoped_copy(OPTION_PREFIX, kind_id, &field_option(key, option)))
        .unwrap_or_else(|| option.to_owned())
}

pub(crate) fn shared_option_label(key: &str, option: &str) -> String {
    let id = format!("{OPTION_PREFIX}{}", field_option(key, option));
    tr_find(&ftl_id(&id), None).unwrap_or_else(|| option.to_owned())
}

fn field_option(key: &str, option: &str) -> String {
    format!("{key}__{option}")
}

fn seconds_label(key: &str, option: &str) -> Option<String> {
    (key == DURATION_SECONDS_KEY && option.parse::<u32>().is_ok()).then(|| {
        let args = ArgsBuilder::new().set("seconds", option.to_owned()).build();
        tr_lookup(SECONDS_LABEL_KEY, Some(&args))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use forge_components::tr_find;
    use forge_overlay::config::SLOT;
    use forge_overlay::kinds::register_builtin_kinds;
    use forge_overlay::motion::{
        ENTRANCE, ENTRANCE_OPTIONS, EXIT, EXIT_OPTIONS, INTENSITY, INTENSITY_OPTIONS, TEXT_EFFECT,
        TEXT_EFFECT_OPTIONS, TEXT_UNIT, TEXT_UNIT_OPTIONS,
    };
    use forge_overlay::{OverlayKindRegistry, SLOT_OPTIONS};
    use forge_platform_core::SENDER_FIELD;
    use forge_registry::FormField;
    use forge_storage::Language;
    use forge_types::{LATEST_DONATION_SLOT, NOW_PLAYING_SLOT};

    use super::*;
    use crate::i18n::install_language;
    use crate::integrations::tests::every_registry_entry;

    const LANGUAGES: [Language; 2] = [Language::En, Language::Uk];
    const DASHES: [char; 4] = ['\u{2014}', '\u{2013}', '\u{2012}', '\u{2015}'];
    const SELECT_KEYS_WITH_UNTRANSLATED_OPTIONS: &[&str] = &["content_type", "font", "accent"];

    type Scoped = (String, &'static str, &'static str);
    type Shared = (&'static str, &'static str);

    #[derive(Default)]
    struct DeclaredOptions {
        scoped: BTreeSet<Scoped>,
        shared: BTreeSet<Shared>,
    }

    impl DeclaredOptions {
        fn collect(&mut self, field: &FormField, owner: &str) {
            match field {
                FormField::Select { key, options, .. } | FormField::Swatch { key, options, .. } => {
                    for option in *options {
                        self.scoped.insert((owner.to_owned(), *key, *option));
                    }
                }
                FormField::UnitAmount {
                    unit_key, bounds, ..
                } => {
                    for unit in bounds.units {
                        self.shared.insert((*unit_key, unit.value));
                    }
                }
                FormField::Optional { inner, .. } => self.collect(inner, owner),
                _ => {}
            }
        }

        fn add_shared(&mut self, key: &'static str, options: &'static [&'static str]) {
            self.shared
                .extend(options.iter().map(|option| (key, *option)));
        }
    }

    async fn declared_options() -> DeclaredOptions {
        let (triggers, sub_actions) = every_registry_entry().await;
        let mut declared = DeclaredOptions::default();
        for descriptor in triggers.all() {
            for field in descriptor.config_fields() {
                declared.collect(&field, descriptor.id());
            }
        }
        for runner in sub_actions.all() {
            for field in runner.config_fields() {
                declared.collect(&field, runner.id());
            }
        }
        let mut overlays = OverlayKindRegistry::new();
        register_builtin_kinds(&mut overlays).unwrap();
        for kind in overlays.all() {
            for sectioned in kind.config_fields() {
                declared.collect(&sectioned.field, kind.id());
            }
        }
        declared.add_shared(ENTRANCE, ENTRANCE_OPTIONS);
        declared.add_shared(EXIT, EXIT_OPTIONS);
        declared.add_shared(TEXT_EFFECT, TEXT_EFFECT_OPTIONS);
        declared.add_shared(TEXT_UNIT, TEXT_UNIT_OPTIONS);
        declared.add_shared(INTENSITY, INTENSITY_OPTIONS);
        declared.add_shared(SLOT, SLOT_OPTIONS);
        declared
            .scoped
            .retain(|(_, key, _)| !SELECT_KEYS_WITH_UNTRANSLATED_OPTIONS.contains(key));
        declared
    }

    fn has_scoped_key(kind: &str, key: &str, option: &str) -> bool {
        seconds_label(key, option).is_some()
            || scoped_copy(OPTION_PREFIX, kind, &field_option(key, option)).is_some()
    }

    fn has_shared_key(key: &str, option: &str) -> bool {
        let id = format!("{OPTION_PREFIX}{}", field_option(key, option));
        tr_find(&ftl_id(&id), None).is_some()
    }

    #[tokio::test]
    async fn every_declared_option_has_a_catalog_key_in_each_language() {
        let declared = declared_options().await;
        assert!(declared.shared.contains(&("delay_unit", "minutes")));
        assert!(
            declared
                .scoped
                .iter()
                .any(|(_, key, option)| *key == SENDER_FIELD && *option == "streamer")
        );

        for language in LANGUAGES {
            install_language(language);
            let scoped: Vec<String> = declared
                .scoped
                .iter()
                .filter(|(kind, key, option)| !has_scoped_key(kind, key, option))
                .map(|(kind, key, option)| format!("{kind} {key}={option}"))
                .collect();
            let shared: Vec<String> = declared
                .shared
                .iter()
                .filter(|(key, option)| !has_shared_key(key, option))
                .map(|(key, option)| format!("{key}={option}"))
                .collect();
            assert_eq!(
                (scoped, shared),
                (Vec::new(), Vec::new()),
                "{language:?} labels fall back to the raw wire value"
            );
        }
    }

    #[tokio::test]
    async fn options_of_one_field_never_share_a_label() {
        let declared = declared_options().await;
        for language in LANGUAGES {
            install_language(language);
            let mut fields: BTreeMap<(String, &str), Vec<String>> = BTreeMap::new();
            for (kind, key, option) in &declared.scoped {
                fields
                    .entry((kind.clone(), *key))
                    .or_default()
                    .push(option_label(kind, key, option));
            }
            for (key, option) in &declared.shared {
                fields
                    .entry((String::new(), *key))
                    .or_default()
                    .push(shared_option_label(key, option));
            }
            for (field, labels) in fields {
                let distinct: BTreeSet<&String> = labels.iter().collect();
                assert_eq!(
                    distinct.len(),
                    labels.len(),
                    "{language:?} {field:?}: {labels:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn ukrainian_labels_use_plain_hyphens_only() {
        let declared = declared_options().await;
        install_language(Language::Uk);
        let labels = declared
            .scoped
            .iter()
            .map(|(kind, key, option)| option_label(kind, key, option))
            .chain(
                declared
                    .shared
                    .iter()
                    .map(|(key, option)| shared_option_label(key, option)),
            );
        for label in labels {
            assert!(!label.contains(DASHES), "{label}");
        }
    }

    #[test]
    fn unknown_values_and_fields_fall_back_to_the_raw_value() {
        install_language(Language::En);
        for (kind, key, option) in [
            ("core.string.trim", "mode", "teleport"),
            ("core.string.trim", "mode", "BOTH"),
            ("core.string.trim", "urgency", ""),
            ("twitch.chat.send_announcement", "color", "red"),
            ("twitch.chat.send_announcement", "color", "Blue"),
            ("twitch.chat.send_message", SENDER_FIELD, "moderator"),
            ("twitch.chat.send_message", SENDER_FIELD, "Bot"),
            ("twitch.chat.send_message", "no_such_field", "on"),
            ("no.such.kind", "duration_seconds", "soon"),
            ("no.such.kind", "duration_seconds", "-5"),
            ("no.such.kind", "duration_seconds", "4294967296"),
        ] {
            assert_eq!(
                option_label(kind, key, option),
                option,
                "{kind} {key}={option}"
            );
        }
    }

    #[test]
    fn shared_labels_fall_back_to_the_raw_value() {
        install_language(Language::En);
        for (key, option) in [
            (SLOT, "written_by_a_newer_build"),
            ("platform", LATEST_DONATION_SLOT),
            (ENTRANCE, "teleport"),
            ("no_such_field", "on"),
        ] {
            assert_eq!(shared_option_label(key, option), option, "{key}={option}");
        }
    }

    #[test]
    fn scoped_labels_resolve_for_their_owner_and_never_leak_to_another_kind() {
        install_language(Language::En);
        for (kind, key, option, expected) in [
            ("twitch.chat.send_announcement", "color", "blue", "Blue"),
            ("twitch.chat.reply", "color", "blue", "blue"),
            ("core.time.now", "format", "iso8601", "ISO 8601"),
            ("core.file.write", "format", "iso8601", "iso8601"),
            ("core.time.add", "unit", "hours", "Hours"),
            ("twitch.poll.create", "unit", "hours", "hours"),
            ("midi.send", "direction", "input", "Input"),
            ("obs.send", "direction", "input", "input"),
            ("core.file.write", "mode", "append", "Append"),
            ("core.string.trim", "mode", "append", "append"),
            ("any.kind", "urgency", "low", "Low"),
        ] {
            assert_eq!(
                option_label(kind, key, option),
                expected,
                "{kind} {key}={option}"
            );
        }
    }

    #[test]
    fn wire_values_outside_the_identifier_alphabet_reach_their_keys() {
        install_language(Language::Uk);
        assert_eq!(
            (
                option_label("core.file.read", "read_as", "Lines array"),
                shared_option_label(ENTRANCE, "slide-up"),
            ),
            ("Масив рядків".to_owned(), "Виїзд знизу".to_owned())
        );
    }

    #[test]
    fn the_two_latest_slots_read_differently_in_each_language() {
        for language in LANGUAGES {
            install_language(language);
            let donation = shared_option_label(SLOT, LATEST_DONATION_SLOT);
            let playing = shared_option_label(SLOT, NOW_PLAYING_SLOT);
            assert!(
                donation != playing
                    && donation != LATEST_DONATION_SLOT
                    && playing != NOW_PLAYING_SLOT,
                "{language:?}: {donation} / {playing}"
            );
        }
    }

    #[test]
    fn duration_seconds_accepts_unsigned_integers_up_to_the_u32_maximum() {
        for (language, unit) in [(Language::En, "s"), (Language::Uk, "с")] {
            install_language(language);
            for option in ["0", "7", "4294967295"] {
                let label = option_label("any.kind", DURATION_SECONDS_KEY, option)
                    .replace(crate::test_support::FORMAT_ISOLATES, "");
                assert_eq!(label, format!("{option} {unit}"), "{language:?}");
            }
        }
    }
}
