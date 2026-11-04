#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{
    ALERT_KIND, BLANK_KIND, BLOB_IDENTITY, CHAT_KIND, FRAME_KIND, GOAL_KIND, LATEST_KIND,
    TICKER_KIND, registry, sub_alert_instance,
};

use forge_overlay::config::{
    ACCENT, BACKGROUND_COLOR, BACKGROUND_OPACITY, BORDER_COLOR, DEFAULT_BACKGROUND_COLOR,
    DEFAULT_BACKGROUND_OPACITY_PERCENT, DEFAULT_IMAGE_HEIGHT_PX, IMAGE, IMAGE_HEIGHT,
    IMAGE_HEIGHT_MAX_PX, IMAGE_HEIGHT_MIN_PX, IMAGE_POSITION, IMAGE_POSITION_ABOVE,
    IMAGE_POSITION_BELOW, IMAGE_POSITION_LEFT, IMAGE_POSITION_OPTIONS, IMAGE_POSITION_RIGHT,
    LABEL_OFFSET_MAX_PX, LABEL_OFFSET_MIN_PX, LABEL_OFFSET_X, LABEL_OFFSET_Y, LABEL_SIDE,
    LABEL_SIDE_CENTER, LABEL_SIDE_LEFT, LABEL_SIDE_RIGHT, MAIN_TEXT_COLOR, TEXT_COLOR,
};
use forge_overlay::metrics::{
    ALERT, BORDER_COLOR_PROPERTY, MAIN_TEXT_COLOR_PROPERTY, SURFACE_RGB, TEXT_COLOR_PROPERTY,
};
use forge_overlay::{
    GENERATED_MEDIA_DIRECTORY, IconValue, ImageReference, MediaSlot, OverlayConfig, OverlayError,
    OverlayMedia, PreviewAccent, PreviewImage, PreviewImageSide, PreviewLabel, PreviewLabelSide,
    PreviewSurface, RUNTIME_SOURCE, ResolvedMedia, config_document, effective_overlay_config,
    glyph_media, hex_color_channels, image_reference, image_references, media_slot,
    read_icon_value, validate_overlay_config,
};
use forge_types::{NamesLanguage, Variant};
use serde_json::Value;

const GIF: &str = "gif";
const WHITE: [u8; 3] = [255, 255, 255];
const TEXT_KINDS: [&str; 6] = [
    ALERT_KIND,
    CHAT_KIND,
    FRAME_KIND,
    GOAL_KIND,
    LATEST_KIND,
    TICKER_KIND,
];

fn text(key: &str, value: &str) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), Variant::String(value.to_owned()))])
}

fn number(key: &str, value: i64) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), Variant::Int(value))])
}

fn composition(kind_id: &str, stored: OverlayConfig) -> forge_overlay::PreviewComposition {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("the kind ships in this build");
    descriptor.preview(
        &effective_overlay_config(descriptor, &stored),
        NamesLanguage::En,
    )
}

fn accepts(kind_id: &str, config: &OverlayConfig) -> bool {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("the kind ships in this build");
    validate_overlay_config(descriptor, config).is_ok()
}

fn page_config(stored: OverlayConfig, media: OverlayMedia) -> Value {
    kind_page_config(ALERT_KIND, stored, media)
}

fn kind_page_config(kind_id: &str, stored: OverlayConfig, media: OverlayMedia) -> Value {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("the kind ships in this build");
    let mut instance = sub_alert_instance(kind_id);
    instance.config = stored;
    instance.media = media;
    let document = config_document(&instance, descriptor).expect("the config document builds");
    let parsed: Value = serde_json::from_str(&document).expect("the document is valid JSON");
    parsed["config"].clone()
}

#[test]
fn the_alert_defaults_paint_the_surface_the_stylesheet_always_drew() {
    let reg = registry();
    let assets = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships")
        .page_assets();
    let scale = ALERT.surface_alpha * 100.0;

    assert_eq!(
        hex_color_channels(DEFAULT_BACKGROUND_COLOR),
        Some(SURFACE_RGB)
    );
    assert!(
        (f32::from(u8::try_from(DEFAULT_BACKGROUND_OPACITY_PERCENT).expect("a percent")) - scale)
            .abs()
            < 0.001
    );
    assert!(
        assets.behavior.contains(&format!(
            "var FALLBACK_SURFACE = \"{DEFAULT_BACKGROUND_COLOR}\";"
        )),
        "a cleared color field would paint something other than the default"
    );
    assert!(assets.style.contains("background: rgba(20, 20, 28, 0.86);"));
}

#[test]
fn an_untouched_alert_hands_its_page_the_exact_color_the_stylesheet_draws() {
    let reg = registry();
    let assets = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships")
        .page_assets();

    let config = page_config(OverlayConfig::new(), OverlayMedia::default());
    let color = config[BACKGROUND_COLOR].as_str().expect("a color text");
    let percent = config[BACKGROUND_OPACITY].as_i64().expect("a percent");
    let [red, green, blue] = hex_color_channels(color).expect("a hex color");
    let painted = format!("background: rgba({red}, {green}, {blue}, 0.{percent});");

    assert!(assets.style.contains(&painted), "{painted}");
    let paint = common::js_function(assets.behavior, "paintSurface");
    assert!(assets.behavior.contains("var PERCENT = 100;"));
    assert!(paint.contains("opacity / PERCENT"));
    assert!(paint.contains("config.background_color"));
    assert!(paint.contains("config.background_opacity"));
}

#[test]
fn an_alert_without_an_image_keeps_the_row_the_stock_page_always_laid_out() {
    let reg = registry();
    let assets = reg
        .get(ALERT_KIND)
        .expect("the alert kind ships")
        .page_assets();

    assert!(
        !assets.markup.contains("picture"),
        "the image exists only when placed"
    );
    assert!(
        assets.style.contains(".message {\n  display: contents;\n}"),
        "the wrapper must not take part in the default row"
    );

    let config = page_config(OverlayConfig::new(), OverlayMedia::default());
    assert_eq!(config[IMAGE], Value::String(String::new()));
    assert_eq!(config[IMAGE_POSITION], IMAGE_POSITION_ABOVE);
    assert_eq!(config[IMAGE_HEIGHT], DEFAULT_IMAGE_HEIGHT_PX);
}

#[test]
fn a_color_is_exactly_a_hash_and_six_hex_digits() {
    for (value, expected) in [
        ("#000000", Some([0, 0, 0])),
        ("#ffFFff", Some([255, 255, 255])),
        ("#123456", Some([18, 52, 86])),
        ("#12345", None),
        ("#1234567", None),
        ("123456", None),
        ("#12345g", None),
        ("#+12345", None),
        ("#ééé", None),
        (" #123456", None),
        ("", None),
    ] {
        assert_eq!(hex_color_channels(value), expected, "{value:?}");
    }
}

#[test]
fn the_background_color_refuses_anything_a_page_could_not_paint() {
    for (value, accepted) in [
        ("", true),
        ("#123456", true),
        ("red", false),
        ("#12", false),
        ("url(http://x)", false),
    ] {
        assert_eq!(
            accepts(ALERT_KIND, &text(BACKGROUND_COLOR, value)),
            accepted,
            "{value:?}"
        );
    }
}

#[test]
fn the_image_field_takes_only_a_library_reference_never_a_path_or_a_glyph() {
    for (value, accepted) in [
        ("", true),
        (&image_reference(BLOB_IDENTITY)[..], true),
        ("heart", false),
        ("/etc/passwd", false),
        ("../secret.gif", false),
        ("image:", false),
        ("image:../../etc/passwd", false),
        ("image:/abs/file.gif", false),
        ("image:a/b", false),
        ("image:a\\b", false),
        ("image:%2e%2e", false),
        ("image:wave.gif", false),
        ("image:a b", false),
        ("image:abc\n", false),
        ("image:ABC", false),
        ("image:a_b", false),
        ("image:рисунок", false),
        ("IMAGE:abc", false),
        (" image:abc", false),
        ("clip:01j9p4s2m7q8v3x5y6z7a8b9c0", false),
    ] {
        let verdict = validate_overlay_config(
            registry().get(ALERT_KIND).expect("the alert kind ships"),
            &text(IMAGE, value),
        );
        assert_eq!(verdict.is_ok(), accepted, "{value:?}");
        if !accepted {
            assert!(
                matches!(&verdict, Err(OverlayError::UnknownChoice { key, .. }) if key == IMAGE),
                "{value:?} produced {verdict:?}"
            );
        }
    }
}

#[test]
fn the_image_and_color_fields_refuse_a_value_that_is_not_text() {
    for key in [IMAGE, BACKGROUND_COLOR] {
        assert!(!accepts(ALERT_KIND, &number(key, 7)), "{key}");
    }
}

#[test]
fn a_kind_without_the_fields_ignores_their_keys() {
    assert!(accepts(TICKER_KIND, &text(BACKGROUND_COLOR, "not a color")));
    assert!(accepts(TICKER_KIND, &text(IMAGE, "../secret.gif")));
}

#[test]
fn the_numeric_controls_stop_at_their_bounds() {
    for (config, accepted) in [
        (number(BACKGROUND_OPACITY, 0), true),
        (number(BACKGROUND_OPACITY, 100), true),
        (number(BACKGROUND_OPACITY, -1), false),
        (number(BACKGROUND_OPACITY, 101), false),
        (number(IMAGE_HEIGHT, IMAGE_HEIGHT_MIN_PX), true),
        (number(IMAGE_HEIGHT, IMAGE_HEIGHT_MAX_PX), true),
        (number(IMAGE_HEIGHT, IMAGE_HEIGHT_MIN_PX - 1), false),
        (number(IMAGE_HEIGHT, IMAGE_HEIGHT_MAX_PX + 1), false),
    ] {
        assert_eq!(accepts(ALERT_KIND, &config), accepted, "{config:?}");
    }
    for (config, accepted) in [
        (number(LABEL_OFFSET_X, LABEL_OFFSET_MIN_PX), true),
        (number(LABEL_OFFSET_Y, LABEL_OFFSET_MAX_PX), true),
        (number(LABEL_OFFSET_X, LABEL_OFFSET_MIN_PX - 1), false),
        (number(LABEL_OFFSET_Y, LABEL_OFFSET_MAX_PX + 1), false),
    ] {
        assert_eq!(accepts(FRAME_KIND, &config), accepted, "{config:?}");
    }
    assert!(!accepts(FRAME_KIND, &text(LABEL_SIDE, "diagonal")));
    for position in IMAGE_POSITION_OPTIONS {
        assert!(accepts(ALERT_KIND, &text(IMAGE_POSITION, position)));
    }
    assert!(!accepts(ALERT_KIND, &text(IMAGE_POSITION, "diagonal")));
}

#[test]
fn the_image_key_is_a_media_slot_of_its_own_that_shares_the_image_references() {
    let reference = image_reference(BLOB_IDENTITY);
    let config = text(IMAGE, &reference);

    assert_eq!(media_slot(IMAGE), Some(MediaSlot::Image));
    assert_eq!(read_icon_value(&reference), IconValue::Image(BLOB_IDENTITY));
    assert_eq!(
        image_references(&config),
        vec![ImageReference {
            key: IMAGE.to_owned(),
            image: BLOB_IDENTITY.to_owned(),
        }],
        "export and retention would miss the blob this alert draws"
    );
}

#[test]
fn a_glyph_name_in_the_image_slot_is_never_resolved_as_a_glyph() {
    assert_eq!(glyph_media(&text(IMAGE, "heart")), OverlayMedia::default());
}

#[test]
fn the_page_gets_the_generated_file_and_never_the_stored_reference() {
    let reference = image_reference(BLOB_IDENTITY);
    let resolved = ResolvedMedia::new(IMAGE, BLOB_IDENTITY, GIF, b"GIF89a".to_vec())
        .expect("a content id is a token");

    let served = page_config(
        text(IMAGE, &reference),
        OverlayMedia {
            resolved: vec![resolved],
            issues: Vec::new(),
        },
    );
    assert_eq!(
        served[IMAGE],
        format!("{GENERATED_MEDIA_DIRECTORY}/{BLOB_IDENTITY}.{GIF}")
    );

    let unresolved = page_config(text(IMAGE, &reference), OverlayMedia::default());
    assert_eq!(
        unresolved[IMAGE], "",
        "an unresolved image must draw nothing"
    );
}

#[test]
fn the_editor_preview_follows_the_surface_choices() {
    let mut chosen = text(BACKGROUND_COLOR, "#123456");
    chosen.extend(number(BACKGROUND_OPACITY, 40));

    for (stored, expected) in [
        (
            OverlayConfig::new(),
            PreviewSurface {
                rgb: SURFACE_RGB,
                alpha: ALERT.surface_alpha,
            },
        ),
        (
            chosen,
            PreviewSurface {
                rgb: [18, 52, 86],
                alpha: 0.4,
            },
        ),
        (
            text(BACKGROUND_COLOR, ""),
            PreviewSurface {
                rgb: SURFACE_RGB,
                alpha: ALERT.surface_alpha,
            },
        ),
    ] {
        assert_eq!(composition(ALERT_KIND, stored).surface, Some(expected));
    }
    assert_eq!(composition(FRAME_KIND, OverlayConfig::new()).surface, None);
}

#[test]
fn the_editor_preview_stays_in_range_on_a_stored_value_validation_would_refuse() {
    for (stored, expected_alpha) in [
        (number(BACKGROUND_OPACITY, 150), 1.0),
        (number(BACKGROUND_OPACITY, -5), 0.0),
        (text(BACKGROUND_OPACITY, "half"), ALERT.surface_alpha),
    ] {
        let surface = composition(ALERT_KIND, stored)
            .surface
            .expect("an alert has a surface");
        assert_eq!(surface.alpha, expected_alpha);
    }

    let mut unknown_position = text(IMAGE, &image_reference(BLOB_IDENTITY));
    unknown_position.extend(text(IMAGE_POSITION, "diagonal"));
    assert_eq!(
        composition(ALERT_KIND, unknown_position)
            .image
            .map(|image| image.side),
        Some(PreviewImageSide::Above)
    );

    let mut wild = text(LABEL_SIDE, "diagonal");
    wild.extend(number(LABEL_OFFSET_X, LABEL_OFFSET_MAX_PX + 1));
    wild.extend(number(LABEL_OFFSET_Y, LABEL_OFFSET_MIN_PX - 1));
    assert_eq!(
        composition(FRAME_KIND, wild).label,
        Some(PreviewLabel {
            side: PreviewLabelSide::Left,
            offset_x: 4000,
            offset_y: -4000,
        })
    );
}

#[test]
fn the_editor_preview_places_a_chosen_image_and_reserves_nothing_without_one() {
    let reference = image_reference(BLOB_IDENTITY);
    assert_eq!(composition(ALERT_KIND, OverlayConfig::new()).image, None);
    assert_eq!(
        composition(ALERT_KIND, text(IMAGE, &reference)).image,
        Some(PreviewImage {
            side: PreviewImageSide::Above,
            height: 120,
        })
    );

    for (position, side) in [
        (IMAGE_POSITION_ABOVE, PreviewImageSide::Above),
        (IMAGE_POSITION_BELOW, PreviewImageSide::Below),
        (IMAGE_POSITION_LEFT, PreviewImageSide::Left),
        (IMAGE_POSITION_RIGHT, PreviewImageSide::Right),
    ] {
        let mut placed = text(IMAGE, &reference);
        placed.extend(text(IMAGE_POSITION, position));
        placed.extend(number(IMAGE_HEIGHT, 90));
        assert_eq!(
            composition(ALERT_KIND, placed).image,
            Some(PreviewImage { side, height: 90 }),
            "{position}"
        );
    }
}

#[test]
fn a_custom_color_is_accepted_for_both_inks_on_every_kind_and_a_named_one_stays_named() {
    for key in [TEXT_COLOR, BORDER_COLOR] {
        assert!(accepts(ALERT_KIND, &text(key, "#ff8800")), "{key}");
        assert!(accepts(FRAME_KIND, &text(key, "#FF8800")), "{key}");
        for refused in ["teal", "#ff88", "ff8800", "var(--x)", ""] {
            assert!(
                !accepts(ALERT_KIND, &text(key, refused)),
                "{key} {refused:?}"
            );
        }
    }
    let custom = composition(ALERT_KIND, text(BORDER_COLOR, "#ff8800"));
    assert_eq!(custom.border_color, PreviewAccent::Custom([255, 136, 0]));
    assert_eq!(
        composition(ALERT_KIND, text(TEXT_COLOR, "sky")).text_color,
        PreviewAccent::Sky
    );
    assert_eq!(PreviewAccent::Custom([0, 0, 0]).name(), None);
    assert_eq!(PreviewAccent::Sky.name(), Some("sky"));
}

#[test]
fn the_preview_paints_the_exact_channels_the_page_gets_for_a_stored_accent() {
    for (stored, channels) in [
        ("mauve", [0xcb, 0xa6, 0xf7]),
        ("sky", [0x89, 0xdc, 0xeb]),
        ("green", [0xa6, 0xe3, 0xa1]),
        ("peach", [0xfa, 0xb3, 0x87]),
        ("yellow", [0xf9, 0xe2, 0xaf]),
        ("red", [0xf3, 0x8b, 0xa8]),
        ("#102030", [0x10, 0x20, 0x30]),
        ("#ABCDEF", [0xab, 0xcd, 0xef]),
        ("flamingo", [0xcb, 0xa6, 0xf7]),
        ("", [0xcb, 0xa6, 0xf7]),
    ] {
        assert_eq!(
            PreviewAccent::from_stored(stored).channels(),
            channels,
            "{stored:?}"
        );
    }
}

#[test]
fn the_runtime_publishes_a_custom_color_verbatim_and_maps_a_named_one() {
    let palette_color: String = common::js_function(RUNTIME_SOURCE, "paletteColor")
        .split_whitespace()
        .collect();

    assert!(RUNTIME_SOURCE.contains("var CUSTOM_ACCENT = /^#[0-9a-f]{6}$/i;"));
    assert!(
        palette_color
            .contains("returnCUSTOM_ACCENT.test(value)?value:ACCENT_HEX[value]||fallback;")
    );
}

#[test]
fn the_runtime_sets_each_color_property_from_its_own_config_key_and_fallback() {
    let compact: String = common::js_function(RUNTIME_SOURCE, "applyAppearance")
        .split_whitespace()
        .collect();

    for (constant, property, key, fallback) in [
        (
            "MAIN_TEXT_COLOR_PROPERTY",
            MAIN_TEXT_COLOR_PROPERTY,
            MAIN_TEXT_COLOR,
            "FALLBACK_MAIN_TEXT_COLOR",
        ),
        (
            "TEXT_COLOR_PROPERTY",
            TEXT_COLOR_PROPERTY,
            TEXT_COLOR,
            "FALLBACK_ACCENT",
        ),
        (
            "BORDER_COLOR_PROPERTY",
            BORDER_COLOR_PROPERTY,
            BORDER_COLOR,
            "FALLBACK_ACCENT",
        ),
    ] {
        assert!(
            RUNTIME_SOURCE.contains(&format!("var {constant} = \"{property}\";")),
            "{constant} no longer names {property}"
        );
        assert!(
            compact.contains(&format!(
                "setProperty({constant},paletteColor(values.{key},{fallback})"
            )),
            "{property} is not fed from config key '{key}' with {fallback} behind it"
        );
    }
}

#[test]
fn a_page_whose_config_carries_no_main_text_color_paints_its_main_text_white() {
    let fallback = RUNTIME_SOURCE
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("var FALLBACK_MAIN_TEXT_COLOR = \"")
        })
        .and_then(|rest| rest.strip_suffix("\";"))
        .expect("the runtime declares a main text fallback");

    assert_eq!(hex_color_channels(fallback), Some(WHITE));
}

#[test]
fn an_overlay_saved_before_the_main_text_color_existed_keeps_white_main_text() {
    for kind_id in TEXT_KINDS {
        for legacy in [text(ACCENT, "#102030"), text(TEXT_COLOR, "sky")] {
            let page = kind_page_config(kind_id, legacy.clone(), OverlayMedia::default());
            let delivered = page[MAIN_TEXT_COLOR].as_str().unwrap_or_default();
            assert_eq!(
                hex_color_channels(delivered),
                Some(WHITE),
                "{kind_id} page gets {delivered:?} for {legacy:?}"
            );

            assert_eq!(
                composition(kind_id, legacy.clone()).main_text_color,
                PreviewAccent::Custom(WHITE),
                "{kind_id} preview for {legacy:?}"
            );
        }
    }
}

#[test]
fn the_main_text_color_takes_any_hex_color_but_no_palette_name() {
    for kind_id in TEXT_KINDS {
        for accepted in ["#ff8800", "#FF8800", "#000000"] {
            assert!(
                accepts(kind_id, &text(MAIN_TEXT_COLOR, accepted)),
                "{kind_id} {accepted:?}"
            );
        }
        for refused in ["sky", "mauve", "", "ff8800", "#ff88", "var(--x)"] {
            assert!(
                !accepts(kind_id, &text(MAIN_TEXT_COLOR, refused)),
                "{kind_id} {refused:?}"
            );
        }
    }
}

#[test]
fn the_preview_paints_main_text_in_the_exact_channels_of_a_chosen_hex() {
    for kind_id in TEXT_KINDS {
        assert_eq!(
            composition(kind_id, text(MAIN_TEXT_COLOR, "#FF8800")).main_text_color,
            PreviewAccent::Custom([255, 136, 0]),
            "{kind_id}"
        );
    }
}

#[test]
fn a_kind_offers_the_main_text_color_exactly_when_it_draws_a_page() {
    let reg = registry();
    let mut blank_seen = false;
    for descriptor in reg.all() {
        blank_seen |= descriptor.id() == BLANK_KIND;
        let offers = descriptor
            .config_fields()
            .iter()
            .any(|sectioned| sectioned.field.key() == MAIN_TEXT_COLOR);

        assert_eq!(
            offers,
            descriptor.has_visual_page(),
            "{} disagrees about having main text to color",
            descriptor.id()
        );
    }
    assert!(blank_seen, "the blank look is not registered");
}

#[test]
fn the_frame_label_defaults_to_its_old_place_and_draws_above_the_frame() {
    assert_eq!(
        composition(FRAME_KIND, OverlayConfig::new()).label,
        Some(PreviewLabel {
            side: PreviewLabelSide::Left,
            offset_x: 0,
            offset_y: 0,
        })
    );
    let reg = registry();
    let assets = reg
        .get(FRAME_KIND)
        .expect("the frame kind ships")
        .page_assets();

    assert!(assets.style.contains("z-index: 1;\n  left: 22px;"));
    assert!(assets.style.contains("var(--label-x, 0px)"));
    assert!(assets.behavior.contains("label.dataset.side"));
}

#[test]
fn the_frame_page_receives_each_label_placement_key() {
    let untouched = kind_page_config(FRAME_KIND, OverlayConfig::new(), OverlayMedia::default());
    assert_eq!(untouched[LABEL_SIDE], LABEL_SIDE_LEFT);
    assert_eq!(untouched[LABEL_OFFSET_X], 0);
    assert_eq!(untouched[LABEL_OFFSET_Y], 0);

    let mut stored = text(LABEL_SIDE, LABEL_SIDE_RIGHT);
    stored.extend(number(LABEL_OFFSET_X, -30));
    stored.extend(number(LABEL_OFFSET_Y, 45));
    let moved = kind_page_config(FRAME_KIND, stored, OverlayMedia::default());
    assert_eq!(moved[LABEL_SIDE], LABEL_SIDE_RIGHT);
    assert_eq!(moved[LABEL_OFFSET_X], -30);
    assert_eq!(moved[LABEL_OFFSET_Y], 45);
}

#[test]
fn the_frame_page_reads_the_placement_keys_the_config_writes() {
    let reg = registry();
    let assets = reg
        .get(FRAME_KIND)
        .expect("the frame kind ships")
        .page_assets();
    let place = common::js_function(assets.behavior, "placeLabel");

    for key in [LABEL_SIDE, LABEL_OFFSET_X, LABEL_OFFSET_Y] {
        assert!(place.contains(&format!("config.{key}")), "{key}");
    }
    for side in [LABEL_SIDE_CENTER, LABEL_SIDE_RIGHT] {
        assert!(
            assets
                .style
                .contains(&format!(".corner[data-side=\"{side}\"]")),
            "{side} has no rule to move it"
        );
    }
}

#[test]
fn the_frame_label_moves_to_any_side_and_offset() {
    for (side, expected) in [
        (LABEL_SIDE_LEFT, PreviewLabelSide::Left),
        (LABEL_SIDE_CENTER, PreviewLabelSide::Center),
        (LABEL_SIDE_RIGHT, PreviewLabelSide::Right),
    ] {
        let mut stored = text(LABEL_SIDE, side);
        stored.extend(number(LABEL_OFFSET_X, -30));
        stored.extend(number(LABEL_OFFSET_Y, 45));

        assert_eq!(
            composition(FRAME_KIND, stored).label,
            Some(PreviewLabel {
                side: expected,
                offset_x: -30,
                offset_y: 45,
            })
        );
    }
    assert_eq!(composition(ALERT_KIND, OverlayConfig::new()).label, None);
}
