#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_overlay::config::{
    ACCENT, ANIMATION, DURATION, EVENT, FONT, HEADLINE, POSITION, SOUND, SUBLINE,
};
use forge_overlay::{
    OverlayConfig, OverlayInstance, OverlayKindRegistry, config_document, register_builtin_kinds,
};
use forge_types::Variant;
use serde_json::Value;

const ALERT_KIND: &str = "overlay.alert";

fn raw_document(config: OverlayConfig) -> String {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    let descriptor = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships in this build");
    let instance = OverlayInstance {
        id: "sub-alert-1".to_owned(),
        display_name: "Sub alert".to_owned(),
        kind_id: ALERT_KIND.to_owned(),
        config,
        source_overrides: Vec::new(),
    };

    config_document(&instance, descriptor).expect("the config document builds")
}

fn document(config: OverlayConfig) -> Value {
    serde_json::from_str(&raw_document(config)).expect("the config document is valid JSON")
}

#[test]
fn the_envelope_names_every_field_in_the_camel_case_the_page_reads() {
    let doc = document(OverlayConfig::new());

    let mut keys: Vec<&str> = doc
        .as_object()
        .expect("the document is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    assert_eq!(
        keys,
        [
            "config",
            "configSchemaVersion",
            "displayName",
            "documentVersion",
            "generatorVersion",
            "kindId",
            "overlayId",
        ]
    );
}

#[test]
fn config_keys_reach_the_page_exactly_as_they_are_stored() {
    let stored_by_a_newer_build = "legacy_sound_path";
    let doc = document(OverlayConfig::from([(
        stored_by_a_newer_build.to_owned(),
        Variant::String("fanfare.mp3".to_owned()),
    )]));

    let config = doc["config"].as_object().expect("config is an object");

    for key in [
        EVENT,
        HEADLINE,
        SUBLINE,
        ACCENT,
        FONT,
        POSITION,
        ANIMATION,
        DURATION,
        SOUND,
        stored_by_a_newer_build,
    ] {
        assert!(
            config.contains_key(key),
            "the page reads '{key}' by name and the document does not carry it"
        );
    }
    assert!(
        !config.contains_key("legacySoundPath"),
        "the envelope's camelCase rename must not reach the config keys themselves"
    );
}

#[test]
fn config_values_arrive_as_plain_json_rather_than_the_tagged_variant_form() {
    let doc = document(OverlayConfig::from([(
        "vendor.enabled".to_owned(),
        Variant::Bool(true),
    )]));
    let config = &doc["config"];

    assert!(
        config[DURATION].is_i64(),
        "a tagged variant would arrive as an object the page cannot multiply: {:?}",
        config[DURATION]
    );
    assert!(config[HEADLINE].is_string(), "{:?}", config[HEADLINE]);
    assert!(
        config["vendor.enabled"].is_boolean(),
        "{:?}",
        config["vendor.enabled"]
    );
}

#[test]
fn the_document_served_to_the_browser_carries_no_credential_slot() {
    let raw = raw_document(OverlayConfig::new()).to_lowercase();

    for probe in [
        "credential",
        "auth",
        "token",
        "secret",
        "bearer",
        "password",
    ] {
        assert!(
            !raw.contains(probe),
            "'{probe}' appears in a document written into a directory the static host serves"
        );
    }
}
