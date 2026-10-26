use forge_storage::TtsPipelineSettings;

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
