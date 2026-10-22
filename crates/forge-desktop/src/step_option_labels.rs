use forge_components::{ArgsBuilder, tr_lookup};

const DURATION_SECONDS_KEY: &str = "duration_seconds";
const SECONDS_LABEL_KEY: &str = "step_option_seconds_value";

struct OptionGroup {
    keys: &'static [&'static str],
    options: &'static [(&'static str, &'static str)],
}

const GROUPS: &[OptionGroup] = &[
    OptionGroup {
        keys: &[
            "emote_only",
            "follower_mode",
            "slow_mode",
            "subscriber_mode",
            "unique_chat_mode",
            "is_enabled",
            "requires_user_input",
            "should_redemptions_skip_request_queue",
            "is_paused",
            "audio_enabled",
            "video_enabled",
            "overall_level",
            "aggression",
            "bullying",
            "disability",
            "misogyny",
            "race_ethnicity_or_religion",
            "sex_based_terms",
            "sexuality_sex_or_gender",
            "swearing",
        ],
        options: &[
            ("unchanged", "step_option_unchanged"),
            ("on", "step_option_on"),
            ("off", "step_option_off"),
            ("0", "step_option_level_0"),
            ("1", "step_option_level_1"),
            ("2", "step_option_level_2"),
            ("3", "step_option_level_3"),
            ("4", "step_option_level_4"),
        ],
    },
    OptionGroup {
        keys: &["status"],
        options: &[
            ("terminated", "step_option_poll_terminated"),
            ("archived", "step_option_poll_archived"),
        ],
    },
    OptionGroup {
        keys: &["fade_mode"],
        options: &[
            ("linear", "step_option_ease_linear"),
            ("easeIn", "step_option_ease_in"),
            ("easeOut", "step_option_ease_out"),
            ("easeBoth", "step_option_ease_both"),
            ("overshoot", "step_option_ease_overshoot"),
            ("zip", "step_option_ease_zip"),
        ],
    },
    OptionGroup {
        keys: &["vertex_pin_type"],
        options: &[
            ("Center", "step_option_pin_center"),
            ("Random", "step_option_pin_random"),
        ],
    },
    OptionGroup {
        keys: &["angle_relative_to", "size_relative_to"],
        options: &[
            ("RelativeToWorld", "step_option_rel_world"),
            (
                "RelativeToCurrentItemRotation",
                "step_option_rel_item_rotation",
            ),
            ("RelativeToModel", "step_option_rel_model"),
            ("RelativeToPinPosition", "step_option_rel_pin_position"),
            ("RelativeToCurrentItemSize", "step_option_rel_item_size"),
        ],
    },
    OptionGroup {
        keys: &["message_kind"],
        options: &[
            ("note_on", "step_option_midi_note_on"),
            ("note_off", "step_option_midi_note_off"),
            ("cc", "step_option_midi_cc"),
            ("raw", "step_option_midi_raw"),
        ],
    },
    OptionGroup {
        keys: &["direction"],
        options: &[
            ("input", "step_option_direction_input"),
            ("output", "step_option_direction_output"),
        ],
    },
    OptionGroup {
        keys: &["mark_as"],
        options: &[
            ("completed", "step_option_mark_completed"),
            ("failed", "step_option_mark_failed"),
        ],
    },
    OptionGroup {
        keys: &["level"],
        options: &[
            ("trace", "step_option_log_trace"),
            ("debug", "step_option_log_debug"),
            ("info", "step_option_log_info"),
            ("warn", "step_option_log_warn"),
            ("error", "step_option_log_error"),
        ],
    },
    OptionGroup {
        keys: &["mode"],
        options: &[
            ("chars", "step_option_len_chars"),
            ("bytes", "step_option_len_bytes"),
            ("both", "step_option_trim_both"),
            ("left", "step_option_trim_left"),
            ("right", "step_option_trim_right"),
            ("count", "step_option_loop_count"),
            ("foreach_array", "step_option_loop_foreach"),
            ("while", "step_option_loop_while"),
            ("overwrite", "step_option_write_overwrite"),
            ("append", "step_option_write_append"),
            ("create_new", "step_option_write_create_new"),
        ],
    },
    OptionGroup {
        keys: &["result_type"],
        options: &[
            ("auto", "step_option_result_auto"),
            ("int", "step_option_result_int"),
            ("float", "step_option_result_float"),
        ],
    },
    OptionGroup {
        keys: &["urgency"],
        options: &[
            ("low", "step_option_urgency_low"),
            ("normal", "step_option_urgency_normal"),
            ("critical", "step_option_urgency_critical"),
        ],
    },
    OptionGroup {
        keys: &["parse_response_as"],
        options: &[
            ("text", "step_option_parse_text"),
            ("json", "step_option_parse_json"),
            ("ignore", "step_option_parse_ignore"),
        ],
    },
    OptionGroup {
        keys: &["unit"],
        options: &[
            ("seconds", "step_option_unit_seconds"),
            ("minutes", "step_option_unit_minutes"),
            ("hours", "step_option_unit_hours"),
            ("days", "step_option_unit_days"),
            ("months", "step_option_unit_months"),
            ("years", "step_option_unit_years"),
        ],
    },
    OptionGroup {
        keys: &["encoding"],
        options: &[
            ("utf8", "step_option_enc_utf8"),
            ("latin1", "step_option_enc_latin1"),
            ("raw_base64", "step_option_enc_raw_base64"),
        ],
    },
    OptionGroup {
        keys: &["read_as"],
        options: &[
            ("Lines array", "step_option_read_lines"),
            ("Whole file", "step_option_read_whole"),
            ("JSON", "step_option_read_json"),
        ],
    },
    OptionGroup {
        keys: &["format"],
        options: &[
            ("iso8601", "step_option_fmt_iso8601"),
            ("unix_seconds", "step_option_fmt_unix_seconds"),
            ("unix_millis", "step_option_fmt_unix_millis"),
            ("custom", "step_option_fmt_custom"),
        ],
    },
    OptionGroup {
        keys: &["privacy_status"],
        options: &[
            ("public", "step_option_privacy_public"),
            ("unlisted", "step_option_privacy_unlisted"),
            ("private", "step_option_privacy_private"),
        ],
    },
    OptionGroup {
        keys: &["ban_type_filter"],
        options: &[
            ("any", "step_option_ban_any"),
            ("permanent", "step_option_ban_permanent"),
            ("temporary", "step_option_ban_temporary"),
        ],
    },
    OptionGroup {
        keys: &["status_filter"],
        options: &[
            ("any", "step_option_status_any"),
            ("fulfilled", "step_option_status_fulfilled"),
            ("canceled", "step_option_status_canceled"),
            ("approved", "step_option_status_approved"),
            ("denied", "step_option_status_denied"),
            ("expired", "step_option_status_expired"),
        ],
    },
    OptionGroup {
        keys: &["position"],
        options: &[
            ("top", "step_option_position_top"),
            ("bottom", "step_option_position_bottom"),
        ],
    },
];

pub(crate) fn step_option_label(key: &str, option: &str) -> Option<String> {
    if key == DURATION_SECONDS_KEY && option.parse::<u32>().is_ok() {
        let args = ArgsBuilder::new().set("seconds", option.to_owned()).build();
        return Some(tr_lookup(SECONDS_LABEL_KEY, Some(&args)));
    }
    GROUPS
        .iter()
        .filter(|group| group.keys.contains(&key))
        .flat_map(|group| group.options.iter())
        .find(|(wire, _)| *wire == option)
        .map(|(_, label_key)| tr_lookup(label_key, None))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_registry::FormField;
    use forge_storage::Language;

    use super::{GROUPS, step_option_label};
    use crate::i18n::install_language;
    use crate::integrations::tests::every_registry_entry;
    use crate::motion_labels::preset_label;

    const LANGUAGES: [Language; 2] = [Language::En, Language::Uk];
    const EN_WIRE_VALUES_DISPLAYED_AS_IS: &[(&str, &str)] = &[
        ("read_as", "Lines array"),
        ("read_as", "Whole file"),
        ("read_as", "JSON"),
    ];
    const UK_WIRE_VALUES_DISPLAYED_AS_IS: &[(&str, &str)] = &[("read_as", "JSON")];
    const SELECT_KEYS_WITH_UNTRANSLATED_OPTIONS: &[&str] = &["content_type"];

    fn displayed_as_is(language: Language) -> &'static [(&'static str, &'static str)] {
        match language {
            Language::En => EN_WIRE_VALUES_DISPLAYED_AS_IS,
            Language::Uk => UK_WIRE_VALUES_DISPLAYED_AS_IS,
        }
    }

    fn mapped_pairs() -> Vec<(&'static str, &'static str)> {
        GROUPS
            .iter()
            .flat_map(|group| {
                group
                    .keys
                    .iter()
                    .flat_map(|key| group.options.iter().map(move |(wire, _)| (*key, *wire)))
            })
            .collect()
    }

    fn collect_static_options(
        field: &FormField,
        owner: &str,
        out: &mut BTreeSet<(String, &'static str, &'static str)>,
    ) {
        match field {
            FormField::Select { key, options, .. } | FormField::Swatch { key, options, .. } => {
                for option in *options {
                    out.insert((owner.to_owned(), *key, *option));
                }
            }
            FormField::Optional { inner, .. } => collect_static_options(inner, owner, out),
            _ => {}
        }
    }

    #[test]
    fn every_mapped_option_has_a_translated_label_in_each_language() {
        for language in LANGUAGES {
            install_language(language);
            for (key, wire) in mapped_pairs() {
                let label = preset_label(key, wire);
                assert!(!label.is_empty(), "{language:?} {key}={wire}");
                assert!(
                    !label.starts_with("step_option_"),
                    "{language:?} {key}={wire}: untranslated {label}"
                );
                if !displayed_as_is(language).contains(&(key, wire)) {
                    assert_ne!(label, wire, "{language:?} {key}={wire}");
                }
            }
        }
    }

    #[test]
    fn options_that_pass_through_unchanged_are_declared_exceptions() {
        for language in LANGUAGES {
            install_language(language);
            let passthrough: Vec<_> = mapped_pairs()
                .into_iter()
                .filter(|(key, wire)| preset_label(key, wire) == *wire)
                .collect();
            assert_eq!(passthrough, displayed_as_is(language), "{language:?}");
        }
    }

    #[test]
    fn unknown_values_and_keys_fall_back_to_the_raw_value() {
        install_language(Language::En);
        for (key, option) in [
            ("mode", "teleport"),
            ("urgency", ""),
            ("position", "left"),
            ("status", "any"),
            ("no_such_field", "on"),
            ("mode", "BOTH"),
            ("duration_seconds", "soon"),
            ("duration_seconds", "-5"),
            ("duration_seconds", "4294967296"),
        ] {
            assert_eq!(step_option_label(key, option), None, "{key}={option}");
            assert_eq!(preset_label(key, option), option, "{key}={option}");
        }
    }

    #[test]
    fn options_of_one_field_never_share_a_label() {
        for language in LANGUAGES {
            install_language(language);
            for group in GROUPS {
                for key in group.keys {
                    let labels: Vec<String> = group
                        .options
                        .iter()
                        .map(|(wire, _)| preset_label(key, wire))
                        .collect();
                    let distinct: BTreeSet<&String> = labels.iter().collect();
                    assert_eq!(
                        distinct.len(),
                        labels.len(),
                        "{language:?} {key}: {labels:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_wire_value_is_not_labelled_under_a_field_that_does_not_declare_it() {
        install_language(Language::En);
        assert_ne!(preset_label("mode", "left"), "left");
        assert_eq!(preset_label("direction", "left"), "left");
        assert_eq!(preset_label("urgency", "both"), "both");
    }

    #[test]
    fn duration_seconds_accepts_unsigned_integers_up_to_the_u32_maximum() {
        for language in LANGUAGES {
            install_language(language);
            for option in ["0", "7", "4294967295"] {
                let label = step_option_label("duration_seconds", option).unwrap();
                assert!(label.contains(option), "{language:?}: {label}");
                assert!(!label.starts_with("step_option_"), "{language:?}: {label}");
            }
        }
    }

    #[test]
    fn ukrainian_labels_use_plain_hyphens_only() {
        install_language(Language::Uk);
        let mut labels: Vec<String> = mapped_pairs()
            .into_iter()
            .map(|(key, wire)| preset_label(key, wire))
            .collect();
        labels.push(step_option_label("duration_seconds", "30").unwrap());
        for label in labels {
            assert!(
                !label.contains(['\u{2014}', '\u{2013}', '\u{2012}', '\u{2015}']),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn every_static_select_option_in_the_registries_is_labelled_or_allow_listed() {
        let (triggers, sub_actions) = every_registry_entry().await;
        let mut options = BTreeSet::new();
        for descriptor in triggers.all() {
            for field in descriptor.config_fields() {
                collect_static_options(&field, descriptor.id(), &mut options);
            }
        }
        for runner in sub_actions.all() {
            for field in runner.config_fields() {
                collect_static_options(&field, runner.id(), &mut options);
            }
        }
        assert!(!options.is_empty());

        install_language(Language::En);
        let unlabelled: BTreeSet<_> = options
            .iter()
            .filter(|(_, key, option)| {
                !SELECT_KEYS_WITH_UNTRANSLATED_OPTIONS.contains(key)
                    && step_option_label(key, option).is_none()
                    && preset_label(key, option) == *option
            })
            .map(|(owner, key, option)| format!("{owner} {key}={option}"))
            .collect();
        assert!(unlabelled.is_empty(), "{unlabelled:#?}");
    }
}
