mod common;
use common::{
    ALERT_KIND, BLANK_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, LATEST_KIND, TICKER_KIND, registry,
    text, with_kind,
};

use forge_overlay::config::{
    DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, MARGIN_BOTTOM,
    MARGIN_LEFT, MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT, MARGIN_RIGHT, MARGIN_TOP, POSITION,
    TEXT_SIZE, TEXT_SIZE_MAX_PX, TEXT_SIZE_MIN_PX,
};
use forge_overlay::{
    ContentMargins, DesignSize, OverlayConfig, OverlayError, OverlayKindDescriptor, PreviewCanvas,
    PreviewElement, effective_overlay_config, element_sizing, validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::Variant;

const ALL_KINDS: [&str; 7] = [
    ALERT_KIND,
    BLANK_KIND,
    CHAT_KIND,
    FRAME_KIND,
    GOAL_KIND,
    LATEST_KIND,
    TICKER_KIND,
];

const BASE_TEXT_SIZE: &[(&str, Option<u32>)] = &[
    (ALERT_KIND, Some(20)),
    (BLANK_KIND, None),
    (CHAT_KIND, Some(13)),
    (FRAME_KIND, Some(13)),
    (GOAL_KIND, Some(14)),
    (LATEST_KIND, Some(20)),
    (TICKER_KIND, Some(19)),
];

const TEXT_SCALES: &[(&str, &[(u32, f32)])] = &[
    (ALERT_KIND, &[(20, 1.0), (40, 2.0), (10, 0.5)]),
    (CHAT_KIND, &[(13, 1.0), (26, 2.0)]),
    (FRAME_KIND, &[(13, 1.0), (26, 2.0)]),
    (GOAL_KIND, &[(14, 1.0), (28, 2.0), (7, 0.5)]),
    (LATEST_KIND, &[(20, 1.0), (40, 2.0), (10, 0.5)]),
    (TICKER_KIND, &[(19, 1.0), (38, 2.0)]),
];

fn one(key: &str, value: Variant) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), value)])
}

fn drawn(kind_id: &str, stored: &OverlayConfig) -> PreviewElement {
    with_kind(kind_id, |descriptor| {
        descriptor
            .preview(
                &effective_overlay_config(descriptor, stored),
                forge_types::NamesLanguage::En,
            )
            .element
    })
}

fn offers(descriptor: &dyn OverlayKindDescriptor, key: &str) -> bool {
    descriptor
        .config_fields()
        .iter()
        .any(|sectioned| sectioned.field.key() == key)
}

fn position_options(descriptor: &dyn OverlayKindDescriptor) -> Option<&'static [&'static str]> {
    descriptor
        .config_fields()
        .into_iter()
        .find_map(|sectioned| match sectioned.field {
            FormField::Select {
                key: POSITION,
                options,
                ..
            } => Some(options),
            _ => None,
        })
}

#[test]
fn each_kind_defaults_to_the_text_size_its_stylesheet_falls_back_to() {
    for (kind_id, base) in BASE_TEXT_SIZE {
        let Some(base) = base else {
            continue;
        };
        let fallback = format!("--text-scale: calc(var(--text-size, {base}) / {base});");

        with_kind(kind_id, |descriptor| {
            assert!(
                descriptor.page_assets().style.contains(&fallback),
                "{kind_id} does not state '{fallback}', so a page without a chosen size renders \
                 at another scale than it did before the field existed"
            );
            assert_eq!(
                descriptor.default_config().get(TEXT_SIZE),
                Some(&Variant::Int(i64::from(*base))),
                "{kind_id} opens the form on a text size other than the one its page falls back to"
            );
        });
    }
}

#[test]
fn each_kind_preview_without_a_stored_size_draws_its_own_base_text_size() {
    for (kind_id, base) in BASE_TEXT_SIZE {
        let element = with_kind(kind_id, |descriptor| {
            descriptor
                .preview(&OverlayConfig::new(), forge_types::NamesLanguage::En)
                .element
        });

        assert_eq!(element.text_size, *base, "{kind_id}");
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
fn the_drawn_text_size_is_a_stored_pixel_pulled_into_range_or_the_kinds_base() {
    for (stored, expected) in [
        (Variant::Int(30), 30),
        (Variant::Int(TEXT_SIZE_MIN_PX), 8),
        (Variant::Int(TEXT_SIZE_MIN_PX - 1), 8),
        (Variant::Int(TEXT_SIZE_MIN_PX + 1), 9),
        (Variant::Int(TEXT_SIZE_MAX_PX - 1), 199),
        (Variant::Int(TEXT_SIZE_MAX_PX), 200),
        (Variant::Int(TEXT_SIZE_MAX_PX + 1), 200),
        (Variant::Int(0), 8),
        (Variant::Int(-1), 8),
        (Variant::Int(i64::MIN), 8),
        (Variant::Int(i64::MAX), 200),
        (Variant::Float(30.0), 20),
        (text("30"), 20),
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
fn every_kind_accepts_the_record_its_own_form_opens_on() {
    for kind_id in ALL_KINDS {
        let refusal = with_kind(kind_id, |descriptor| {
            validate_overlay_config(descriptor, &descriptor.default_config()).err()
        });

        assert!(
            refusal.is_none(),
            "{kind_id} refuses the record its own form opens on: {refusal:?}"
        );
    }
}

#[test]
fn an_untouched_record_previews_at_its_looks_design_size_and_margins() {
    for (kind_id, width, height, margins) in [
        (ALERT_KIND, 800, 600, ContentMargins::even(10)),
        (TICKER_KIND, 1920, 96, ContentMargins::NONE),
        (GOAL_KIND, 640, 160, ContentMargins::NONE),
        (LATEST_KIND, 600, 96, ContentMargins::NONE),
        (CHAT_KIND, 400, 600, ContentMargins::NONE),
        (FRAME_KIND, 1280, 720, ContentMargins::NONE),
    ] {
        let composition = with_kind(kind_id, |descriptor| {
            descriptor.preview(
                &effective_overlay_config(descriptor, &OverlayConfig::new()),
                forge_types::NamesLanguage::En,
            )
        });

        assert_eq!(
            (composition.canvas, composition.margins),
            (PreviewCanvas { width, height }, margins),
            "{kind_id} opens on a box other than the one its look is designed for"
        );
    }
}

#[test]
fn every_kind_accepts_the_design_size_and_margin_bounds_and_refuses_one_step_past_each() {
    for kind_id in ALL_KINDS {
        for (key, min, max) in [
            (DESIGN_WIDTH, DESIGN_SIZE_MIN_PX, DESIGN_SIZE_MAX_PX),
            (DESIGN_HEIGHT, DESIGN_SIZE_MIN_PX, DESIGN_SIZE_MAX_PX),
            (MARGIN_TOP, MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT),
            (MARGIN_RIGHT, MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT),
            (MARGIN_BOTTOM, MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT),
            (MARGIN_LEFT, MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT),
        ] {
            with_kind(kind_id, |descriptor| {
                for accepted in [min, min + 1, max - 1, max] {
                    validate_overlay_config(descriptor, &one(key, Variant::Int(accepted)))
                        .unwrap_or_else(|e| panic!("{kind_id} refused {key} = {accepted}: {e}"));
                }

                for rejected in [min - 1, max + 1, i64::MIN, i64::MAX] {
                    let err =
                        validate_overlay_config(descriptor, &one(key, Variant::Int(rejected)))
                            .expect_err("a value outside the supported range must be refused");

                    assert!(
                        matches!(&err, OverlayError::OutOfRange { key: k, .. } if k == key),
                        "{kind_id} answered {key} = {rejected} with {err:?}"
                    );
                }
            });
        }
    }
}

#[test]
fn a_design_size_or_margin_held_as_anything_but_a_whole_number_is_refused() {
    for key in [DESIGN_WIDTH, DESIGN_HEIGHT, MARGIN_TOP, MARGIN_LEFT] {
        for stored in [Variant::Float(10.0), text("10"), Variant::Bool(true)] {
            let err = with_kind(ALERT_KIND, |descriptor| {
                validate_overlay_config(descriptor, &one(key, stored.clone()))
                    .expect_err("a non-integer size must be refused")
            });

            assert!(
                matches!(&err, OverlayError::WrongType { key: k, .. } if k == key),
                "{key} held as {stored:?} produced {err:?}"
            );
        }
    }
}

#[test]
fn a_stored_margin_is_previewed_pulled_into_range() {
    for (stored, expected) in [
        (MARGIN_MIN_PERCENT - 1, 0),
        (MARGIN_MIN_PERCENT, 0),
        (MARGIN_MIN_PERCENT + 1, 1),
        (MARGIN_MAX_PERCENT - 1, 44),
        (MARGIN_MAX_PERCENT, 45),
        (MARGIN_MAX_PERCENT + 1, 45),
        (i64::MIN, 0),
        (i64::MAX, 45),
    ] {
        let margins = with_kind(ALERT_KIND, |descriptor| {
            descriptor
                .preview(
                    &effective_overlay_config(descriptor, &one(MARGIN_TOP, Variant::Int(stored))),
                    forge_types::NamesLanguage::En,
                )
                .margins
        });

        assert_eq!(
            margins.top, expected,
            "a stored top margin of {stored}% was previewed as {}%",
            margins.top
        );
    }
}

#[test]
fn a_stored_design_size_is_previewed_pulled_into_range() {
    for (stored, expected) in [
        (DESIGN_SIZE_MIN_PX - 1, 40),
        (DESIGN_SIZE_MIN_PX, 40),
        (DESIGN_SIZE_MAX_PX, 7680),
        (DESIGN_SIZE_MAX_PX + 1, 7680),
        (0, 40),
        (i64::MAX, 7680),
    ] {
        let canvas = with_kind(TICKER_KIND, |descriptor| {
            descriptor
                .preview(
                    &effective_overlay_config(descriptor, &one(DESIGN_WIDTH, Variant::Int(stored))),
                    forge_types::NamesLanguage::En,
                )
                .canvas
        });

        assert_eq!(
            canvas.width, expected,
            "a stored design width of {stored} was previewed as {}",
            canvas.width
        );
    }
}

#[test]
fn every_look_offers_the_source_box_and_position_only_where_the_box_does_not_place_it() {
    for kind_id in ALL_KINDS {
        with_kind(kind_id, |descriptor| {
            for kept in [
                DESIGN_WIDTH,
                DESIGN_HEIGHT,
                MARGIN_TOP,
                MARGIN_RIGHT,
                MARGIN_BOTTOM,
                MARGIN_LEFT,
            ] {
                assert!(offers(descriptor, kept), "{kind_id} does not offer {kept}");
            }
        });
    }

    for kind_id in [ALERT_KIND, TICKER_KIND, GOAL_KIND, LATEST_KIND] {
        with_kind(kind_id, |descriptor| {
            assert!(
                !offers(descriptor, POSITION),
                "{kind_id} still offers an in-page position the box replaced"
            );
            assert!(
                !descriptor.default_config().contains_key(POSITION),
                "{kind_id} still opens on an in-page position"
            );
        });
    }
}

#[test]
fn chat_and_frame_keep_position_as_a_top_or_bottom_edge_opening_on_the_bottom() {
    for kind_id in [CHAT_KIND, FRAME_KIND] {
        with_kind(kind_id, |descriptor| {
            assert_eq!(
                position_options(descriptor),
                Some(&["top", "bottom"][..]),
                "{kind_id} offers an edge other than top or bottom"
            );
            assert_eq!(
                descriptor.default_config().get(POSITION),
                Some(&text("bottom")),
                "{kind_id} opens on an edge other than the bottom"
            );
        });
    }
}

#[test]
fn a_design_size_written_into_a_config_reads_back_unchanged() {
    for size in [
        DesignSize {
            width: 40,
            height: 7680,
        },
        DesignSize {
            width: 1920,
            height: 96,
        },
    ] {
        let mut config = OverlayConfig::new();
        size.write_into(&mut config);

        assert_eq!(
            DesignSize::read(&config, DesignSize::BROWSER_SOURCE_DEFAULT),
            size
        );
    }
}

#[test]
fn margins_written_into_a_config_read_back_unchanged_side_by_side() {
    let margins = ContentMargins {
        top: 30,
        right: 15,
        bottom: 0,
        left: 45,
    };
    let mut config = OverlayConfig::new();
    margins.write_into(&mut config);

    assert_eq!(ContentMargins::read(&config, ContentMargins::NONE), margins);
}

#[test]
fn the_kind_list_names_every_registered_kind() {
    let reg = registry();
    let mut registered: Vec<&str> = reg.all().map(|descriptor| descriptor.id()).collect();
    registered.sort_unstable();
    let mut listed = ALL_KINDS.to_vec();
    listed.sort_unstable();
    assert_eq!(listed, registered);
}
