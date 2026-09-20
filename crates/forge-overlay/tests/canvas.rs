#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::{
    ConfigSection, OverlayConfig, OverlayError, OverlayInstance, OverlayKindRegistry,
    PreviewCanvas, config_document, effective_overlay_config, register_builtin_kinds,
    validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::Variant;
use serde_json::Value;

const BUILTIN_IDS: &[&str] = &[
    "overlay.alert",
    "overlay.chat",
    "overlay.frame",
    "overlay.goal",
    "overlay.ticker",
];

const ALERT_KIND: &str = "overlay.alert";
const WIDTH_KEY: &str = "canvas_width";
const HEIGHT_KEY: &str = "canvas_height";

const FULL_HD: PreviewCanvas = PreviewCanvas {
    width: 1920,
    height: 1080,
};

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn field_key(field: &FormField) -> &'static str {
    match field {
        FormField::Text { key, .. }
        | FormField::TextArea { key, .. }
        | FormField::Code { key, .. }
        | FormField::Integer { key, .. }
        | FormField::Slider { key, .. }
        | FormField::Toggle { key, .. }
        | FormField::FilePicker { key, .. }
        | FormField::DateTime { key, .. }
        | FormField::Select { key, .. }
        | FormField::DynamicSelect { key, .. }
        | FormField::DependentSelect { key, .. }
        | FormField::Swatch { key, .. }
        | FormField::Optional { key, .. }
        | FormField::SubChain { key, .. }
        | FormField::CaseList { key, .. } => key,
    }
}

fn sides(width: Variant, height: Variant) -> OverlayConfig {
    OverlayConfig::from([
        (WIDTH_KEY.to_owned(), width),
        (HEIGHT_KEY.to_owned(), height),
    ])
}

fn document(kind_id: &str, stored: &OverlayConfig) -> Value {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("a registered builtin kind");
    let instance = OverlayInstance {
        id: "sub-alert-1".to_owned(),
        display_name: "Sub alert".to_owned(),
        kind_id: kind_id.to_owned(),
        config: stored.clone(),
        source_overrides: Vec::new(),
        credential: None,
    };

    serde_json::from_str(&config_document(&instance, descriptor).expect("the document builds"))
        .expect("the config document is valid JSON")
}

fn alert_canvas(stored: &OverlayConfig) -> PreviewCanvas {
    let reg = registry();
    let descriptor = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships in this build");
    descriptor
        .preview(&effective_overlay_config(descriptor, stored))
        .canvas
}

#[test]
fn an_overlay_stored_without_canvas_keys_draws_and_publishes_full_hd() {
    let reg = registry();

    for kind_id in BUILTIN_IDS {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");
        let effective = effective_overlay_config(descriptor, &OverlayConfig::new());

        assert_eq!(
            descriptor.preview(&effective).canvas,
            FULL_HD,
            "{kind_id} draws a record written before the canvas fields at the wrong size"
        );
        assert_eq!(
            (effective.get(WIDTH_KEY), effective.get(HEIGHT_KEY)),
            (Some(&Variant::Int(1920)), Some(&Variant::Int(1080))),
            "{kind_id} leaves an untouched record without a canvas to fall back on"
        );

        let doc = document(kind_id, &OverlayConfig::new());
        assert_eq!(
            (
                doc["config"][WIDTH_KEY].as_i64(),
                doc["config"][HEIGHT_KEY].as_i64(),
            ),
            (Some(1920), Some(1080)),
            "{kind_id} published a page config the drawn preview does not agree with"
        );
    }
}

#[test]
fn a_stored_canvas_size_replaces_full_hd_in_the_preview_and_on_the_page() {
    let stored = sides(Variant::Int(1280), Variant::Int(720));

    assert_eq!(
        alert_canvas(&stored),
        PreviewCanvas {
            width: 1280,
            height: 720,
        },
        "a size the user chose was not drawn"
    );

    let doc = document(ALERT_KIND, &stored);
    assert_eq!(
        (
            doc["config"][WIDTH_KEY].as_i64(),
            doc["config"][HEIGHT_KEY].as_i64(),
        ),
        (Some(1280), Some(720)),
        "a size the user chose never reached the page config"
    );
}

#[test]
fn every_builtin_offers_the_canvas_sides_in_its_style_section_right_after_position() {
    let reg = registry();

    for kind_id in BUILTIN_IDS {
        let offered: Vec<&str> = reg
            .get(kind_id)
            .expect("a registered builtin kind")
            .config_fields()
            .iter()
            .filter(|sectioned| sectioned.section == ConfigSection::Style)
            .map(|sectioned| field_key(&sectioned.field))
            .collect();

        assert_eq!(
            offered,
            ["accent", "font", "position", WIDTH_KEY, HEIGHT_KEY],
            "{kind_id} does not offer the canvas sides where the style section puts them"
        );
    }
}

#[test]
fn a_canvas_side_outside_the_supported_range_is_refused_before_it_is_stored() {
    let reg = registry();

    for kind_id in BUILTIN_IDS {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");

        for key in [WIDTH_KEY, HEIGHT_KEY] {
            for accepted in [160, 161, 1080, 7679, 7680] {
                let config = OverlayConfig::from([(key.to_owned(), Variant::Int(accepted))]);
                validate_overlay_config(descriptor, &config)
                    .unwrap_or_else(|e| panic!("{kind_id} refused {key} = {accepted}: {e}"));
            }

            for rejected in [159, 0, -1, 7681, i64::MIN, i64::MAX] {
                let config = OverlayConfig::from([(key.to_owned(), Variant::Int(rejected))]);
                let err = validate_overlay_config(descriptor, &config)
                    .expect_err("a canvas side outside the range must be refused");

                assert!(
                    matches!(&err, OverlayError::OutOfRange { key: k, .. } if k == key),
                    "{kind_id} answered {key} = {rejected} with {err:?}"
                );
            }
        }
    }
}

#[test]
fn the_drawn_preview_pulls_a_canvas_side_back_into_the_supported_range() {
    for (stored, expected) in [
        (159_i64, 160_u32),
        (160, 160),
        (1280, 1280),
        (7680, 7680),
        (7681, 7680),
        (0, 160),
        (-1, 160),
        (i64::MIN, 160),
        (i64::MAX, 7680),
    ] {
        let canvas = alert_canvas(&sides(Variant::Int(stored), Variant::Int(stored)));

        assert_eq!(
            canvas,
            PreviewCanvas {
                width: expected,
                height: expected,
            },
            "a stored canvas side of {stored} was drawn as {canvas:?}"
        );
    }
}

#[test]
fn a_canvas_side_stored_as_anything_but_a_whole_number_draws_at_full_hd() {
    for held in [
        Variant::Float(1280.5),
        Variant::String("1280".to_owned()),
        Variant::Bool(true),
        Variant::Array(vec![Variant::Int(1280)]),
    ] {
        assert_eq!(
            alert_canvas(&sides(held.clone(), held.clone())),
            FULL_HD,
            "a canvas side held as {held:?} was drawn as something other than the default"
        );
    }
}

#[test]
fn nothing_in_this_build_asks_a_stored_overlay_record_to_be_rewritten() {
    for kind_id in BUILTIN_IDS {
        let doc = document(kind_id, &OverlayConfig::new());

        assert_eq!(
            doc["configSchemaVersion"].as_u64(),
            Some(1),
            "{kind_id} bumped its config schema, which puts every stored record through a rewrite"
        );
        assert_eq!(
            doc["generatorVersion"].as_u64(),
            Some(1),
            "{kind_id} stamped a generator version that marks every materialized page stale"
        );
    }
}
