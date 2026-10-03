#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::config::{
    DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, ELEMENT_HEIGHT,
    ELEMENT_WIDTH, MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT,
    MARGIN_RIGHT, MARGIN_TOP, MIGRATION_ACKNOWLEDGED, POSITION, TEXT_SIZE, TEXT_SIZE_MAX_PX,
    TEXT_SIZE_MIN_PX,
};
use forge_overlay::{
    ContentMargins, DesignSize, OverlayConfig, OverlayError, OverlayKindDescriptor,
    OverlayKindRegistry, PreviewCanvas, PreviewElement, acknowledge_sizing_notice,
    effective_overlay_config, element_sizing, register_builtin_kinds, sizing_notice_pending,
    upgrade_config, validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::Variant;

const ALERT_KIND: &str = "overlay.alert";
const BLANK_KIND: &str = "overlay.blank";
const CHAT_KIND: &str = "overlay.chat";
const FRAME_KIND: &str = "overlay.frame";
const GOAL_KIND: &str = "overlay.goal";
const LATEST_KIND: &str = "overlay.latest";
const TICKER_KIND: &str = "overlay.ticker";

const ALL_KINDS: [&str; 7] = [
    ALERT_KIND,
    BLANK_KIND,
    CHAT_KIND,
    FRAME_KIND,
    GOAL_KIND,
    LATEST_KIND,
    TICKER_KIND,
];

const PRE_BOX_SCHEMA: &[(&str, u32)] = &[
    (ALERT_KIND, 2),
    (CHAT_KIND, 1),
    (FRAME_KIND, 1),
    (GOAL_KIND, 1),
    (TICKER_KIND, 1),
];

const VENDOR_KEY: &str = "vendor.future_key";

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

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn with_kind<T>(kind_id: &str, body: impl FnOnce(&dyn OverlayKindDescriptor) -> T) -> T {
    let reg = registry();
    body(reg.get(kind_id).expect("a registered builtin kind"))
}

fn one(key: &str, value: Variant) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), value)])
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn int(config: &OverlayConfig, key: &str) -> Option<i64> {
    config.get(key).and_then(Variant::as_int)
}

fn margins_of(config: &OverlayConfig) -> [Option<i64>; 4] {
    [MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT].map(|side| int(config, side))
}

fn drawn(kind_id: &str, stored: &OverlayConfig) -> PreviewElement {
    with_kind(kind_id, |descriptor| {
        descriptor
            .preview(&effective_overlay_config(descriptor, stored))
            .element
    })
}

fn upgraded(kind_id: &str, stored: &OverlayConfig) -> OverlayConfig {
    let version = PRE_BOX_SCHEMA
        .iter()
        .find(|(kind, _)| *kind == kind_id)
        .map(|(_, version)| *version)
        .expect("a look that predates the box contract");
    with_kind(kind_id, |descriptor| {
        upgrade_config(descriptor, version, stored).expect("an older record is upgraded")
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
            descriptor.preview(&effective_overlay_config(descriptor, &OverlayConfig::new()))
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
                .preview(&effective_overlay_config(
                    descriptor,
                    &one(MARGIN_TOP, Variant::Int(stored)),
                ))
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
                .preview(&effective_overlay_config(
                    descriptor,
                    &one(DESIGN_WIDTH, Variant::Int(stored)),
                ))
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
fn the_box_contract_retires_element_size_everywhere_and_position_where_the_box_places_the_look() {
    for kind_id in ALL_KINDS {
        with_kind(kind_id, |descriptor| {
            for retired in [ELEMENT_WIDTH, ELEMENT_HEIGHT] {
                assert!(
                    !offers(descriptor, retired),
                    "{kind_id} still offers {retired}, which the source box replaced"
                );
            }
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
fn a_record_already_at_the_current_schema_is_left_alone() {
    for kind_id in ALL_KINDS {
        let untouched = with_kind(kind_id, |descriptor| {
            upgrade_config(
                descriptor,
                descriptor.config_schema_version(),
                &OverlayConfig::from([(ELEMENT_WIDTH.to_owned(), Variant::Int(640))]),
            )
        });

        assert!(
            untouched.is_none(),
            "{kind_id} rewrote a record that is already at its current schema: {untouched:?}"
        );
    }
}

#[test]
fn an_older_alert_without_element_size_takes_the_look_default_box_and_its_old_padding_as_percent() {
    let config = upgraded(ALERT_KIND, &OverlayConfig::new());

    assert_eq!(
        (int(&config, DESIGN_WIDTH), int(&config, DESIGN_HEIGHT)),
        (Some(800), Some(600)),
        "an alert sized by nobody did not open on its look default box"
    );
    assert_eq!(
        margins_of(&config),
        [Some(5), Some(4), Some(5), Some(4)],
        "32px of old padding over an 800x600 box is 5% vertically and 4% horizontally"
    );
}

#[test]
fn an_older_record_seeds_its_box_from_the_element_size_it_stored_and_drops_that_size() {
    let config = upgraded(
        ALERT_KIND,
        &OverlayConfig::from([
            (ELEMENT_WIDTH.to_owned(), Variant::Int(1000)),
            (ELEMENT_HEIGHT.to_owned(), Variant::Int(300)),
        ]),
    );

    assert_eq!(
        (int(&config, DESIGN_WIDTH), int(&config, DESIGN_HEIGHT)),
        (Some(1000), Some(300)),
        "the box ignored the element size the streamer had chosen"
    );
    assert_eq!(
        margins_of(&config),
        [Some(11), Some(3), Some(11), Some(3)],
        "the old padding was not re-expressed as a share of the seeded box"
    );
    assert!(
        !config.contains_key(ELEMENT_WIDTH) && !config.contains_key(ELEMENT_HEIGHT),
        "the retired element size survived the upgrade"
    );
}

#[test]
fn a_seeded_box_is_pulled_into_the_design_size_range() {
    for (stored, expected) in [
        (i64::MAX, DESIGN_SIZE_MAX_PX),
        (DESIGN_SIZE_MAX_PX + 1, DESIGN_SIZE_MAX_PX),
        (DESIGN_SIZE_MIN_PX - 1, DESIGN_SIZE_MIN_PX),
        (0, DESIGN_SIZE_MIN_PX),
    ] {
        let config = upgraded(GOAL_KIND, &one(ELEMENT_WIDTH, Variant::Int(stored)));

        assert_eq!(
            int(&config, DESIGN_WIDTH),
            Some(expected),
            "an element width of {stored} seeded a box outside the supported range"
        );
    }
}

#[test]
fn an_element_size_held_as_anything_but_a_whole_number_seeds_the_look_default() {
    for stored in [Variant::Float(500.0), text("500"), Variant::Bool(true)] {
        let config = upgraded(GOAL_KIND, &one(ELEMENT_WIDTH, stored.clone()));

        assert_eq!(
            int(&config, DESIGN_WIDTH),
            Some(640),
            "an element width held as {stored:?} seeded a box the look does not default to"
        );
    }
}

#[test]
fn a_huge_old_padding_share_is_pulled_down_to_the_margin_bound() {
    let config = upgraded(
        ALERT_KIND,
        &OverlayConfig::from([
            (ELEMENT_WIDTH.to_owned(), Variant::Int(DESIGN_SIZE_MIN_PX)),
            (ELEMENT_HEIGHT.to_owned(), Variant::Int(DESIGN_SIZE_MIN_PX)),
        ]),
    );

    assert_eq!(
        margins_of(&config),
        [Some(MARGIN_MAX_PERCENT); 4],
        "32px over a 40px box seeded margins past the supported bound"
    );
}

#[test]
fn each_look_reexpresses_its_own_old_padding_over_its_default_box() {
    for (kind_id, expected) in [
        (CHAT_KIND, [Some(4), Some(6), Some(4), Some(6)]),
        (GOAL_KIND, [Some(20), Some(5), Some(20), Some(5)]),
        (TICKER_KIND, [Some(0); 4]),
        (FRAME_KIND, [Some(0); 4]),
    ] {
        assert_eq!(
            margins_of(&upgraded(kind_id, &OverlayConfig::new())),
            expected,
            "{kind_id} re-expressed its old padding as the wrong share of its box"
        );
    }
}

#[test]
fn a_record_that_already_holds_a_box_or_margins_keeps_them_through_the_upgrade() {
    let stored = OverlayConfig::from([
        (DESIGN_WIDTH.to_owned(), Variant::Int(1280)),
        (DESIGN_HEIGHT.to_owned(), Variant::Int(720)),
        (ELEMENT_WIDTH.to_owned(), Variant::Int(400)),
        (MARGIN_TOP.to_owned(), Variant::Int(30)),
    ]);

    let config = upgraded(ALERT_KIND, &stored);

    assert_eq!(
        (int(&config, DESIGN_WIDTH), int(&config, DESIGN_HEIGHT)),
        (Some(1280), Some(720)),
        "a box the streamer set was overwritten by the retired element size"
    );
    assert_eq!(
        margins_of(&config),
        [Some(30), None, None, None],
        "a margin the streamer set was reseeded from the old padding"
    );
}

#[test]
fn an_upgrade_keeps_every_key_it_does_not_own() {
    let config = upgraded(
        CHAT_KIND,
        &OverlayConfig::from([
            (VENDOR_KEY.to_owned(), Variant::Bool(true)),
            (TEXT_SIZE.to_owned(), Variant::Int(18)),
        ]),
    );

    assert_eq!(config.get(VENDOR_KEY), Some(&Variant::Bool(true)));
    assert_eq!(config.get(TEXT_SIZE), Some(&Variant::Int(18)));
}

#[test]
fn an_upgrade_drops_position_where_the_box_places_the_look() {
    for kind_id in [ALERT_KIND, TICKER_KIND, GOAL_KIND] {
        let config = upgraded(kind_id, &one(POSITION, text("top")));

        assert!(
            !config.contains_key(POSITION),
            "{kind_id} kept an in-page position the box replaced"
        );
    }
}

#[test]
fn an_upgrade_folds_center_into_the_bottom_edge_and_keeps_an_offered_edge() {
    for kind_id in [CHAT_KIND, FRAME_KIND] {
        for (stored, expected) in [
            (Some(text("center")), Some(text("bottom"))),
            (Some(text("diagonal")), Some(text("bottom"))),
            (Some(Variant::Int(1)), Some(text("bottom"))),
            (Some(text("top")), Some(text("top"))),
            (Some(text("bottom")), Some(text("bottom"))),
            (None, None),
        ] {
            let before = stored
                .clone()
                .map_or_else(OverlayConfig::new, |value| one(POSITION, value));

            assert_eq!(
                upgraded(kind_id, &before).get(POSITION),
                expected.as_ref(),
                "{kind_id} upgraded a position of {stored:?} to the wrong edge"
            );
        }
    }
}

#[test]
fn an_upgraded_record_validates_against_its_current_form() {
    for (kind_id, _) in PRE_BOX_SCHEMA {
        let config = upgraded(
            kind_id,
            &OverlayConfig::from([
                (ELEMENT_WIDTH.to_owned(), Variant::Int(i64::MAX)),
                (ELEMENT_HEIGHT.to_owned(), Variant::Int(1)),
                (POSITION.to_owned(), text("center")),
            ]),
        );

        let refusal = with_kind(kind_id, |descriptor| {
            validate_overlay_config(descriptor, &config).err()
        });
        assert!(
            refusal.is_none(),
            "{kind_id} upgraded a record into one its own form refuses: {refusal:?}"
        );
    }
}

#[test]
fn upgrading_an_already_upgraded_record_again_changes_nothing() {
    for (kind_id, _) in PRE_BOX_SCHEMA {
        let stored = OverlayConfig::from([
            (ELEMENT_WIDTH.to_owned(), Variant::Int(900)),
            (POSITION.to_owned(), text("center")),
        ]);
        let once = upgraded(kind_id, &stored);

        assert_eq!(
            upgraded(kind_id, &once),
            once,
            "{kind_id} drifted when an interrupted upgrade ran a second time"
        );
    }
}

#[test]
fn every_upgraded_look_raises_the_sizing_notice() {
    for (kind_id, _) in PRE_BOX_SCHEMA {
        let config = upgraded(kind_id, &OverlayConfig::new());

        assert_eq!(
            config.get(MIGRATION_ACKNOWLEDGED),
            Some(&Variant::Bool(false)),
            "{kind_id} changed its sizing silently"
        );
        assert!(sizing_notice_pending(&config), "{kind_id}");
    }
}

#[test]
fn a_record_that_never_migrated_shows_no_sizing_notice() {
    for kind_id in ALL_KINDS {
        let defaults = with_kind(kind_id, |descriptor| descriptor.default_config());

        assert!(
            !sizing_notice_pending(&defaults),
            "{kind_id} opens a fresh overlay on a migration notice"
        );
    }
}

#[test]
fn acknowledging_the_notice_clears_it_for_good() {
    let mut config = upgraded(ALERT_KIND, &OverlayConfig::new());

    acknowledge_sizing_notice(&mut config);

    assert!(!sizing_notice_pending(&config));
    acknowledge_sizing_notice(&mut config);
    assert_eq!(
        config.get(MIGRATION_ACKNOWLEDGED),
        Some(&Variant::Bool(true)),
        "acknowledging twice did not leave the notice acknowledged"
    );
}

#[test]
fn acknowledging_a_record_that_never_migrated_records_nothing() {
    let mut config = OverlayConfig::new();

    acknowledge_sizing_notice(&mut config);

    assert!(
        !config.contains_key(MIGRATION_ACKNOWLEDGED),
        "an overlay that never migrated gained a migration marker"
    );
}

#[test]
fn a_notice_marker_of_the_wrong_type_is_not_a_pending_notice() {
    for stored in [Variant::Int(0), text("false"), Variant::Bool(true)] {
        assert!(
            !sizing_notice_pending(&one(MIGRATION_ACKNOWLEDGED, stored.clone())),
            "a marker held as {stored:?} raised the notice"
        );
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
fn blank_draws_nothing_on_stream_and_upgrades_without_a_sizing_notice() {
    let upgraded_blank = with_kind(BLANK_KIND, |descriptor| {
        upgrade_config(descriptor, 0, &OverlayConfig::new())
    });

    assert!(
        upgraded_blank.is_none_or(|config| !sizing_notice_pending(&config)),
        "blank draws nothing on stream yet raised a sizing notice"
    );
}
