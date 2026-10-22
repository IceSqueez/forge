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
