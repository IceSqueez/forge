#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::config::{ELEMENT_HEIGHT, ELEMENT_WIDTH, ICON, TEXT_SIZE};
use forge_overlay::metrics::{ELEMENT_HEIGHT_PROPERTY, ELEMENT_WIDTH_PROPERTY};
use forge_overlay::{
    ConfigSection, OverlayConfig, OverlayError, OverlayInstance, OverlayKindRegistry, OverlayMedia,
    PreviewCanvas, PreviewElement, SampleContext, config_document, effective_overlay_config,
    element_sizing, register_builtin_kinds, style_guards, validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::Variant;
use serde_json::Value;

const ALERT_KIND: &str = "overlay.alert";
const AUDIO_KIND: &str = "overlay.audio";
const CHAT_KIND: &str = "overlay.chat";
const FRAME_KIND: &str = "overlay.frame";
const GOAL_KIND: &str = "overlay.goal";
const TICKER_KIND: &str = "overlay.ticker";

const STALE_WIDTH_KEY: &str = "canvas_width";
const STALE_HEIGHT_KEY: &str = "canvas_height";

const SIZE_MIN_PX: i64 = 40;
const WIDTH_MAX_PX: i64 = 1920;
const HEIGHT_MAX_PX: i64 = 1080;
const TEXT_MIN_PX: i64 = 8;
const TEXT_MAX_PX: i64 = 200;

const REFERENCE_CANVAS: PreviewCanvas = PreviewCanvas {
    width: 1920,
    height: 1080,
};

const STYLE_FIELDS: &[(&str, &[&str])] = &[
    (
        ALERT_KIND,
        &[
            "accent",
            "font",
            "position",
            ELEMENT_WIDTH,
            ELEMENT_HEIGHT,
            TEXT_SIZE,
            ICON,
        ],
    ),
    (AUDIO_KIND, &[]),
    (
        CHAT_KIND,
        &[
            "accent",
            "font",
            "position",
            ELEMENT_WIDTH,
            ELEMENT_HEIGHT,
            TEXT_SIZE,
        ],
    ),
    (FRAME_KIND, &["accent", "font", "position", TEXT_SIZE]),
    (
        GOAL_KIND,
        &[
            "accent",
            "font",
            "position",
            ELEMENT_WIDTH,
            ELEMENT_HEIGHT,
            TEXT_SIZE,
        ],
    ),
    (
        TICKER_KIND,
        &["accent", "font", "position", ELEMENT_HEIGHT, TEXT_SIZE],
    ),
];

const ELEMENT_EXTENTS: &[(&str, &[&str])] = &[
    (
        ALERT_KIND,
        &[
            "width: var(--element-width, auto);",
            "min-height: var(--element-height, auto);",
        ],
    ),
    (
        CHAT_KIND,
        &[
            "width: var(--element-width, 360px);",
            "max-height: var(--element-height, 100%);",
        ],
    ),
    (FRAME_KIND, &[]),
    (
        GOAL_KIND,
        &[
            "width: var(--element-width, 320px);",
            "min-height: var(--element-height, auto);",
        ],
    ),
    (TICKER_KIND, &["min-height: var(--element-height, auto);"]),
];

const CONFIG_SCHEMA_VERSIONS: &[(&str, u64)] = &[
    (ALERT_KIND, 2),
    (AUDIO_KIND, 1),
    (CHAT_KIND, 1),
    (FRAME_KIND, 1),
    (GOAL_KIND, 1),
    (TICKER_KIND, 1),
];

const BASE_TEXT_SIZE: &[(&str, Option<u32>)] = &[
    (ALERT_KIND, Some(20)),
    (AUDIO_KIND, None),
    (CHAT_KIND, Some(13)),
    (FRAME_KIND, Some(13)),
    (GOAL_KIND, Some(14)),
    (TICKER_KIND, Some(19)),
];

const TEXT_SCALES: &[(&str, &[(u32, f32)])] = &[
    (ALERT_KIND, &[(20, 1.0), (40, 2.0), (10, 0.5)]),
    (CHAT_KIND, &[(13, 1.0), (26, 2.0)]),
    (FRAME_KIND, &[(13, 1.0), (26, 2.0)]),
    (GOAL_KIND, &[(14, 1.0), (28, 2.0), (7, 0.5)]),
    (TICKER_KIND, &[(19, 1.0), (38, 2.0)]),
];

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

fn one(key: &str, value: Variant) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), value)])
}

fn drawn(kind_id: &str, stored: &OverlayConfig) -> PreviewElement {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("a registered builtin kind");
    descriptor
        .preview(&effective_overlay_config(descriptor, stored))
        .element
}

fn side(element: PreviewElement, key: &str) -> Option<u32> {
    if key == ELEMENT_WIDTH {
        element.width
    } else {
        element.height
    }
}

fn stylesheet(kind_id: &str) -> &'static str {
    registry()
        .get(kind_id)
        .expect("a registered builtin kind")
        .page_assets()
        .style
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
        media: OverlayMedia::default(),
        sample: SampleContext::neutral(),
    };

    serde_json::from_str(&config_document(&instance, descriptor).expect("the document builds"))
        .expect("the config document is valid JSON")
}

#[test]
fn each_kind_offers_the_style_fields_its_page_can_spend_and_its_sizing_table_agrees() {
    let reg = registry();

    for (kind_id, expected) in STYLE_FIELDS {
        let offered: Vec<&str> = reg
            .get(kind_id)
            .expect("a registered builtin kind")
            .config_fields()
            .iter()
            .filter(|sectioned| sectioned.section == ConfigSection::Style)
            .map(|sectioned| field_key(&sectioned.field))
            .collect();

        assert_eq!(
            offered, *expected,
            "{kind_id} offers an appearance field its page cannot spend, or withholds one it can"
        );

        let sizing = element_sizing(kind_id);
        assert_eq!(
            sizing.is_some(),
            !expected.is_empty(),
            "{kind_id} disagrees with its sizing table about drawing a page at all"
        );
        if let Some(sizing) = sizing {
            assert_eq!(
                sizing.width.is_some(),
                expected.contains(&ELEMENT_WIDTH),
                "{kind_id} offers a width field its sizing table never resolves, or the reverse"
            );
            assert_eq!(
                sizing.height.is_some(),
                expected.contains(&ELEMENT_HEIGHT),
                "{kind_id} offers a height field its sizing table never resolves, or the reverse"
            );
        }
    }
}

#[test]
fn each_kind_binds_the_axes_it_offers_and_names_no_property_for_the_axes_it_does_not() {
    let guards = style_guards();

    for (kind_id, expected) in ELEMENT_EXTENTS {
        let guard = guards
            .iter()
            .find(|guard| guard.kind_id == *kind_id)
            .expect("a kind that draws a page is guarded");
        let extents: Vec<&str> = guard
            .declarations
            .iter()
            .map(String::as_str)
            .filter(|declaration| {
                declaration.contains(ELEMENT_WIDTH_PROPERTY)
                    || declaration.contains(ELEMENT_HEIGHT_PROPERTY)
            })
            .collect();

        assert_eq!(
            extents, *expected,
            "{kind_id} binds an axis to a bound or a fallback other than the one it draws"
        );

        let style = stylesheet(kind_id);
        for declaration in *expected {
            assert!(
                style.contains(declaration),
                "{kind_id} resolves '{declaration}' but its stylesheet never states it"
            );
        }
        for property in [ELEMENT_WIDTH_PROPERTY, ELEMENT_HEIGHT_PROPERTY] {
            if expected.iter().any(|d| d.contains(property)) {
                continue;
            }
            assert!(
                !style.contains(property),
                "{kind_id} spends {property} on an axis its form never offers"
            );
        }
    }
}

#[test]
fn each_kind_defaults_to_the_text_size_its_stylesheet_falls_back_to() {
    let reg = registry();

    for (kind_id, base) in BASE_TEXT_SIZE {
        let Some(base) = base else {
            continue;
        };
        let fallback = format!("--text-scale: calc(var(--text-size, {base}) / {base});");

        assert!(
            stylesheet(kind_id).contains(&fallback),
            "{kind_id} does not state '{fallback}', so a page without a chosen size renders at \
             another scale than it did before the field existed"
        );
        assert_eq!(
            reg.get(kind_id)
                .expect("a registered builtin kind")
                .default_config()
                .get(TEXT_SIZE),
            Some(&Variant::Int(i64::from(*base))),
            "{kind_id} opens the form on a text size other than the one its page falls back to"
        );
    }
}

#[test]
fn a_chosen_text_size_scales_a_page_by_its_ratio_to_the_kinds_base() {
    for (kind_id, rows) in TEXT_SCALES {
        let sizing = element_sizing(kind_id).expect("a kind that draws a page has a sizing table");

        for (chosen, expected) in *rows {
            assert_eq!(
                sizing.text_scale(*chosen),
                *expected,
                "{kind_id} scales a page set to {chosen}px by the wrong ratio"
            );
        }
    }
}

#[test]
fn an_element_nobody_sized_is_drawn_without_a_side_and_at_its_kinds_base_text_size() {
    for (kind_id, base) in BASE_TEXT_SIZE {
        let element = drawn(kind_id, &OverlayConfig::new());

        assert_eq!(
            (element.width, element.height),
            (None, None),
            "{kind_id} invents a size for an element the user left to size itself"
        );
        assert_eq!(
            element.text_size, *base,
            "{kind_id} draws an untouched record at a text size its page would not use"
        );
    }
}

#[test]
fn an_element_side_is_drawn_only_from_a_whole_number_on_an_axis_its_kind_offers() {
    for (kind_id, key, stored, expected) in [
        (ALERT_KIND, ELEMENT_WIDTH, Variant::Int(640), Some(640)),
        (ALERT_KIND, ELEMENT_HEIGHT, Variant::Int(200), Some(200)),
        (CHAT_KIND, ELEMENT_WIDTH, Variant::Int(500), Some(500)),
        (CHAT_KIND, ELEMENT_HEIGHT, Variant::Int(400), Some(400)),
        (GOAL_KIND, ELEMENT_WIDTH, Variant::Int(420), Some(420)),
        (TICKER_KIND, ELEMENT_HEIGHT, Variant::Int(90), Some(90)),
        (TICKER_KIND, ELEMENT_WIDTH, Variant::Int(500), None),
        (FRAME_KIND, ELEMENT_WIDTH, Variant::Int(500), None),
        (FRAME_KIND, ELEMENT_HEIGHT, Variant::Int(500), None),
        (AUDIO_KIND, ELEMENT_WIDTH, Variant::Int(500), None),
        (AUDIO_KIND, ELEMENT_HEIGHT, Variant::Int(500), None),
        (ALERT_KIND, ELEMENT_WIDTH, Variant::Float(640.0), None),
        (
            ALERT_KIND,
            ELEMENT_WIDTH,
            Variant::String("640".to_owned()),
            None,
        ),
        (ALERT_KIND, ELEMENT_HEIGHT, Variant::Bool(true), None),
        (
            ALERT_KIND,
            ELEMENT_HEIGHT,
            Variant::Array(vec![Variant::Int(200)]),
            None,
        ),
    ] {
        let element = drawn(kind_id, &one(key, stored.clone()));

        assert_eq!(
            side(element, key),
            expected,
            "{kind_id} drew {key} held as {stored:?} as {:?}",
            side(element, key)
        );
    }
}

#[test]
fn an_element_side_outside_its_range_is_drawn_at_the_nearest_size_the_page_allows() {
    for (key, stored, expected) in [
        (ELEMENT_WIDTH, SIZE_MIN_PX - 1, 40),
        (ELEMENT_WIDTH, SIZE_MIN_PX, 40),
        (ELEMENT_WIDTH, SIZE_MIN_PX + 1, 41),
        (ELEMENT_WIDTH, WIDTH_MAX_PX - 1, 1919),
        (ELEMENT_WIDTH, WIDTH_MAX_PX, 1920),
        (ELEMENT_WIDTH, WIDTH_MAX_PX + 1, 1920),
        (ELEMENT_WIDTH, 0, 40),
        (ELEMENT_WIDTH, -1, 40),
        (ELEMENT_WIDTH, i64::MIN, 40),
        (ELEMENT_WIDTH, i64::MAX, 1920),
        (ELEMENT_HEIGHT, SIZE_MIN_PX - 1, 40),
        (ELEMENT_HEIGHT, SIZE_MIN_PX, 40),
        (ELEMENT_HEIGHT, HEIGHT_MAX_PX - 1, 1079),
        (ELEMENT_HEIGHT, HEIGHT_MAX_PX, 1080),
        (ELEMENT_HEIGHT, HEIGHT_MAX_PX + 1, 1080),
        (ELEMENT_HEIGHT, 0, 40),
        (ELEMENT_HEIGHT, i64::MIN, 40),
        (ELEMENT_HEIGHT, i64::MAX, 1080),
    ] {
        let element = drawn(ALERT_KIND, &one(key, Variant::Int(stored)));

        assert_eq!(
            side(element, key),
            Some(expected),
            "a stored {key} of {stored} was drawn as {:?}",
            side(element, key)
        );
    }
}

#[test]
fn the_drawn_text_size_is_a_stored_pixel_pulled_into_range_or_the_kinds_base() {
    for (stored, expected) in [
        (Variant::Int(30), 30),
        (Variant::Int(TEXT_MIN_PX), 8),
        (Variant::Int(TEXT_MIN_PX - 1), 8),
        (Variant::Int(TEXT_MIN_PX + 1), 9),
        (Variant::Int(TEXT_MAX_PX - 1), 199),
        (Variant::Int(TEXT_MAX_PX), 200),
        (Variant::Int(TEXT_MAX_PX + 1), 200),
        (Variant::Int(0), 8),
        (Variant::Int(-1), 8),
        (Variant::Int(i64::MIN), 8),
        (Variant::Int(i64::MAX), 200),
        (Variant::Float(30.0), 20),
        (Variant::String("30".to_owned()), 20),
        (Variant::Bool(true), 20),
    ] {
        let element = drawn(ALERT_KIND, &one(TEXT_SIZE, stored.clone()));

        assert_eq!(
            element.text_size,
            Some(expected),
            "a text size held as {stored:?} was drawn as {:?}",
            element.text_size
        );
    }
}

#[test]
fn a_size_outside_its_supported_range_is_refused_before_it_is_stored() {
    let reg = registry();

    for (kind_id, key, min, max) in [
        (ALERT_KIND, ELEMENT_WIDTH, SIZE_MIN_PX, WIDTH_MAX_PX),
        (ALERT_KIND, ELEMENT_HEIGHT, SIZE_MIN_PX, HEIGHT_MAX_PX),
        (CHAT_KIND, ELEMENT_WIDTH, SIZE_MIN_PX, WIDTH_MAX_PX),
        (CHAT_KIND, ELEMENT_HEIGHT, SIZE_MIN_PX, HEIGHT_MAX_PX),
        (GOAL_KIND, ELEMENT_WIDTH, SIZE_MIN_PX, WIDTH_MAX_PX),
        (GOAL_KIND, ELEMENT_HEIGHT, SIZE_MIN_PX, HEIGHT_MAX_PX),
        (TICKER_KIND, ELEMENT_HEIGHT, SIZE_MIN_PX, HEIGHT_MAX_PX),
        (ALERT_KIND, TEXT_SIZE, TEXT_MIN_PX, TEXT_MAX_PX),
        (CHAT_KIND, TEXT_SIZE, TEXT_MIN_PX, TEXT_MAX_PX),
        (FRAME_KIND, TEXT_SIZE, TEXT_MIN_PX, TEXT_MAX_PX),
        (GOAL_KIND, TEXT_SIZE, TEXT_MIN_PX, TEXT_MAX_PX),
        (TICKER_KIND, TEXT_SIZE, TEXT_MIN_PX, TEXT_MAX_PX),
    ] {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");

        for accepted in [min, min + 1, max - 1, max] {
            validate_overlay_config(descriptor, &one(key, Variant::Int(accepted)))
                .unwrap_or_else(|e| panic!("{kind_id} refused {key} = {accepted}: {e}"));
        }

        for rejected in [min - 1, 0, -1, max + 1, i64::MIN, i64::MAX] {
            let err = validate_overlay_config(descriptor, &one(key, Variant::Int(rejected)))
                .expect_err("a size outside the supported range must be refused");

            assert!(
                matches!(&err, OverlayError::OutOfRange { key: k, .. } if k == key),
                "{kind_id} answered {key} = {rejected} with {err:?}"
            );
        }
    }
}

#[test]
fn a_record_stored_before_these_fields_keeps_working_untouched() {
    let stale = OverlayConfig::from([
        (STALE_WIDTH_KEY.to_owned(), Variant::Int(1280)),
        (STALE_HEIGHT_KEY.to_owned(), Variant::Int(720)),
    ]);
    let reg = registry();

    for (kind_id, schema_version) in CONFIG_SCHEMA_VERSIONS {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");
        validate_overlay_config(descriptor, &stale)
            .unwrap_or_else(|e| panic!("{kind_id} refused a record an older build wrote: {e}"));

        let composition = descriptor.preview(&effective_overlay_config(descriptor, &stale));
        assert_eq!(
            composition.canvas, REFERENCE_CANVAS,
            "{kind_id} draws an older record on a canvas other than the browser source it fills"
        );
        assert_eq!(
            (composition.element.width, composition.element.height),
            (None, None),
            "{kind_id} sized its element from a key this build no longer means"
        );
        assert_eq!(
            document(kind_id, &stale)["configSchemaVersion"].as_u64(),
            Some(*schema_version),
            "{kind_id} moved its config schema without a decision to read stored records anew"
        );
    }
}

#[test]
fn every_kind_accepts_the_record_its_own_form_opens_on() {
    let reg = registry();

    for (kind_id, _) in CONFIG_SCHEMA_VERSIONS {
        let descriptor = reg.get(kind_id).expect("a registered builtin kind");

        let refusal = validate_overlay_config(descriptor, &descriptor.default_config()).err();

        assert!(
            refusal.is_none(),
            "{kind_id} refuses the record its own form opens on: {refusal:?}"
        );
    }
}

#[test]
fn the_page_config_carries_a_size_the_user_set_and_leaves_out_one_they_did_not() {
    let doc = document(ALERT_KIND, &one(ELEMENT_WIDTH, Variant::Int(640)));
    let config = doc["config"].as_object().expect("config is an object");

    assert_eq!(
        config.get(ELEMENT_WIDTH).and_then(Value::as_i64),
        Some(640),
        "the page cannot size itself from {:?}",
        config.get(ELEMENT_WIDTH)
    );
    assert!(
        config.get(ELEMENT_HEIGHT).is_none(),
        "an element side the user left empty reached the page as {:?} rather than not at all",
        config.get(ELEMENT_HEIGHT)
    );
    assert_eq!(
        config.get(TEXT_SIZE).and_then(Value::as_i64),
        Some(20),
        "the page cannot scale itself from {:?}",
        config.get(TEXT_SIZE)
    );
}
