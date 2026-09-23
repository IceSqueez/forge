#![allow(clippy::unwrap_used, clippy::expect_used)]

use forge_overlay::config::{
    ACCENT, ANIMATION, DEFAULT_ICON, DURATION, FONT, HEADLINE, ICON, POSITION, SOUND, SUBLINE,
};
use forge_overlay::{
    GENERATED_MEDIA_DIRECTORY, ICON_FILE_FIELD, ICON_TINTABLE_FIELD, OverlayConfig,
    OverlayInstance, OverlayKindRegistry, OverlayMedia, ResolvedMedia, SampleContext,
    config_document, image_reference, register_builtin_kinds,
};
use forge_types::Variant;
use serde_json::{Value, json};

const ALERT_KIND: &str = "overlay.alert";

const PAGE_CREDENTIAL: &str = "7c1e4a90b2d84f36a5c0e9b71d3f8a24";
const BLOB_IDENTITY: &str =
    "sha256-1fe5a351bf0314c8a1840b023fd1e4cab3f0f123468940c241bd7bf20e989ab8";
const SVG: &str = "svg";

fn raw_document(config: OverlayConfig, credential: Option<&str>, media: OverlayMedia) -> String {
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
        credential: credential.map(str::to_owned),
        media,
        sample: SampleContext::neutral(),
    };

    config_document(&instance, descriptor).expect("the config document builds")
}

fn document(config: OverlayConfig) -> Value {
    serde_json::from_str(&raw_document(config, None, OverlayMedia::default()))
        .expect("the config document is valid JSON")
}

fn document_with_media(config: OverlayConfig, media: OverlayMedia) -> Value {
    serde_json::from_str(&raw_document(config, None, media))
        .expect("the config document is valid JSON")
}

fn icon_config(stored: &str) -> OverlayConfig {
    OverlayConfig::from([(ICON.to_owned(), Variant::String(stored.to_owned()))])
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
fn the_icon_reaches_the_page_as_a_file_and_a_fill_decision_rather_than_a_bare_string() {
    let media = OverlayMedia {
        resolved: vec![
            ResolvedMedia::new(ICON, BLOB_IDENTITY, SVG, b"<svg/>".to_vec())
                .expect("a content id is a token"),
        ],
        issues: Vec::new(),
    };
    let path = format!("{GENERATED_MEDIA_DIRECTORY}/{BLOB_IDENTITY}.{SVG}");

    for (stored, tintable, label) in [
        (DEFAULT_ICON.to_owned(), true, "a curated glyph"),
        (image_reference(BLOB_IDENTITY), false, "an imported image"),
    ] {
        let doc = document_with_media(icon_config(&stored), media.clone());

        assert_eq!(
            doc["config"][ICON],
            json!({ ICON_FILE_FIELD: path, ICON_TINTABLE_FIELD: tintable }),
            "{label} reached the page as something the script cannot place"
        );
    }
}

#[test]
fn an_icon_that_did_not_resolve_reaches_the_page_as_an_empty_file_rather_than_a_missing_key() {
    let doc = document(icon_config("no-such-glyph"));

    assert_eq!(
        doc["config"][ICON][ICON_FILE_FIELD].as_str(),
        Some(""),
        "the page reads config.icon.file unconditionally and got {:?}",
        doc["config"][ICON]
    );
}

/// The page authenticates its own socket with this value, so the slot is deliberate; what must
/// never happen is the credential appearing when the instance carries none, or leaking into the
/// `config` map every kind renders field by field.
#[test]
fn the_page_credential_is_a_top_level_slot_present_only_when_the_instance_holds_one() {
    let bound: Value = serde_json::from_str(&raw_document(
        OverlayConfig::new(),
        Some(PAGE_CREDENTIAL),
        OverlayMedia::default(),
    ))
    .expect("the config document is valid JSON");

    assert_eq!(
        bound["credential"].as_str(),
        Some(PAGE_CREDENTIAL),
        "the page cannot authenticate its socket without its own credential"
    );
    assert!(
        !bound["config"].to_string().contains(PAGE_CREDENTIAL),
        "the credential reached the config map, where kinds render every key they find"
    );

    let unbound = document(OverlayConfig::new());
    assert!(
        unbound.get("credential").is_none(),
        "an instance carrying no credential still emitted the slot as null"
    );
}
