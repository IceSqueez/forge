use forge_storage::{SkipRegex, TtsPipelineSettings};

#[test]
fn settings_serialized_before_strip_emoji_existed_restore_with_it_off() {
    let with_flag = TtsPipelineSettings {
        strip_emoji: true,
        ..TtsPipelineSettings::default()
    };
    let mut serialized = serde_json::to_value(&with_flag).expect("serialize settings");
    let removed = serialized
        .as_object_mut()
        .expect("settings serialize as an object")
        .remove("strip_emoji");

    let restored: TtsPipelineSettings =
        serde_json::from_value(serialized).expect("an old backup still deserializes");

    assert!(removed.is_some());
    assert!(!restored.strip_emoji);
}

#[test]
fn strip_emoji_survives_a_serde_round_trip_in_both_states() {
    for flag in [false, true] {
        let settings = TtsPipelineSettings {
            strip_emoji: flag,
            ..TtsPipelineSettings::default()
        };

        let json = serde_json::to_string(&settings).expect("serialize");
        let restored: TtsPipelineSettings = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.strip_emoji, flag);
    }
}

#[test]
fn settings_serialized_before_the_row_switches_existed_restore_with_every_row_on() {
    let switched_off = TtsPipelineSettings {
        skip_prefix_enabled: false,
        output_max_duration_enabled: false,
        ..TtsPipelineSettings::default()
    };
    let mut serialized = serde_json::to_value(&switched_off).expect("serialize settings");
    let object = serialized
        .as_object_mut()
        .expect("settings serialize as an object");
    let removed = [
        object.remove("skip_prefix_enabled"),
        object.remove("output_max_duration_enabled"),
    ];

    let restored: TtsPipelineSettings =
        serde_json::from_value(serialized).expect("an old backup still deserializes");

    assert!(removed.iter().all(Option::is_some));
    assert!(restored.skip_prefix_enabled);
    assert!(restored.output_max_duration_enabled);
}

#[test]
fn a_skip_regex_list_reads_bare_strings_and_objects_in_their_stored_order() {
    let stored = serde_json::json!([
        "^bare$",
        { "pattern": "^off$", "enabled": false },
        { "pattern": "^unset$" },
        { "pattern": "^on$", "enabled": true },
    ]);

    let read: Vec<SkipRegex> = serde_json::from_value(stored).expect("both forms deserialize");

    assert_eq!(
        read,
        [
            SkipRegex::enabled("^bare$"),
            SkipRegex {
                pattern: "^off$".to_owned(),
                enabled: false,
            },
            SkipRegex::enabled("^unset$"),
            SkipRegex::enabled("^on$"),
        ]
    );
}

#[test]
fn a_skip_regex_is_written_as_an_object_that_keeps_its_switch() {
    let regex = SkipRegex {
        pattern: "^off$".to_owned(),
        enabled: false,
    };

    let written = serde_json::to_value(&regex).expect("serialize");

    assert_eq!(
        written,
        serde_json::json!({ "pattern": "^off$", "enabled": false })
    );
}

#[test]
fn a_skip_regex_in_neither_form_is_refused() {
    for malformed in [
        serde_json::json!(7),
        serde_json::json!({ "enabled": true }),
        serde_json::json!({ "pattern": 7 }),
        serde_json::json!(null),
    ] {
        let read = serde_json::from_value::<SkipRegex>(malformed.clone());
        assert!(read.is_err(), "{malformed}");
    }
}
