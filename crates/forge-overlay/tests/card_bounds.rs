#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{ALERT_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, TICKER_KIND, registry, with_kind};

use std::collections::BTreeSet;

use forge_overlay::config::{
    CARD_HEIGHT_LIMITED, CARD_MAX_HEIGHT, CARD_MAX_SIZE_MIN_PX, CARD_MAX_WIDTH, CARD_MIN_HEIGHT,
    CARD_MIN_WIDTH, CARD_SIZE_MAX_PX, CARD_WIDTH_LIMITED, TEXT_OVERFLOW, TEXT_OVERFLOW_ELLIPSIS,
    TEXT_OVERFLOW_WRAP,
};
use forge_overlay::{
    OverlayConfig, OverlayKindDescriptor, OverlayMedia, PreviewCard, PreviewComposition,
    PreviewTextOverflow, config_document, effective_overlay_config, validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::{NamesLanguage, Variant};
use serde_json::Value;

const UNBOUNDED: PreviewCard = PreviewCard {
    min_width: 0,
    min_height: 0,
    max_width: None,
    max_height: None,
    overflow: PreviewTextOverflow::Wrap,
};

fn stored(entries: &[(&str, Variant)]) -> OverlayConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn composition(kind_id: &str, config: OverlayConfig) -> PreviewComposition {
    with_kind(kind_id, |descriptor| {
        descriptor.preview(
            &effective_overlay_config(descriptor, &config),
            NamesLanguage::En,
        )
    })
}

fn card(config: OverlayConfig) -> PreviewCard {
    composition(ALERT_KIND, config)
        .card
        .expect("an alert draws a card")
}

fn both_limits(
    width_toggle: Variant,
    height_toggle: Variant,
    width: i64,
    height: i64,
) -> OverlayConfig {
    stored(&[
        (CARD_WIDTH_LIMITED, width_toggle),
        (CARD_HEIGHT_LIMITED, height_toggle),
        (CARD_MAX_WIDTH, Variant::Int(width)),
        (CARD_MAX_HEIGHT, Variant::Int(height)),
    ])
}

fn card_bounds_table(behavior: &str) -> &str {
    let opening = "var CARD_BOUNDS = [";
    let start = behavior
        .find(opening)
        .expect("the script declares its card bounds table")
        + opening.len();
    let rest = &behavior[start..];
    &rest[..rest.find("];").expect("the card bounds table is closed")]
}

fn quoted_after(source: &str, marker: &str) -> Vec<String> {
    source
        .match_indices(marker)
        .filter_map(|(at, _)| {
            let rest = &source[at + marker.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

fn offered_keys(descriptor: &dyn OverlayKindDescriptor) -> BTreeSet<&'static str> {
    descriptor
        .config_fields()
        .iter()
        .flat_map(|sectioned| match &sectioned.field {
            FormField::Optional { key, inner, .. } => vec![*key, inner.key()],
            field => vec![field.key()],
        })
        .collect()
}

#[test]
fn an_untouched_alert_previews_a_card_that_neither_grows_nor_caps_and_wraps_its_text() {
    assert_eq!(card(OverlayConfig::new()), UNBOUNDED);
}

#[test]
fn an_untouched_alert_hands_its_page_values_that_add_no_card_class_or_size() {
    let reg = registry();
    let descriptor = reg.get(ALERT_KIND).expect("the alert kind ships");
    let mut instance = common::sub_alert_instance(ALERT_KIND);
    instance.config = OverlayConfig::new();
    instance.media = OverlayMedia::default();
    let document = config_document(&instance, descriptor).expect("the config document builds");
    let config = serde_json::from_str::<Value>(&document).expect("valid JSON")["config"].clone();

    assert_eq!(
        [
            config[CARD_MIN_WIDTH].clone(),
            config[CARD_MIN_HEIGHT].clone(),
            config[CARD_WIDTH_LIMITED].clone(),
            config[CARD_HEIGHT_LIMITED].clone(),
            config[TEXT_OVERFLOW].clone(),
        ],
        [
            Value::from(0),
            Value::from(0),
            Value::Bool(false),
            Value::Bool(false),
            Value::from(TEXT_OVERFLOW_WRAP),
        ]
    );
}

#[test]
fn a_switched_off_limit_ignores_the_maximum_stored_behind_it() {
    for (label, toggle) in [
        ("off", Variant::Bool(false)),
        ("stored as text", Variant::String("true".to_owned())),
        ("stored as a number", Variant::Int(1)),
    ] {
        let card = card(both_limits(toggle.clone(), toggle, 500, 300));

        assert_eq!(
            (card.max_width, card.max_height),
            (None, None),
            "a limit {label}"
        );
    }
}

#[test]
fn a_switched_on_limit_caps_only_its_own_axis_at_its_own_maximum() {
    let width_only = card(both_limits(
        Variant::Bool(true),
        Variant::Bool(false),
        500,
        300,
    ));
    let height_only = card(both_limits(
        Variant::Bool(false),
        Variant::Bool(true),
        500,
        300,
    ));

    assert_eq!(
        [
            (width_only.max_width, width_only.max_height),
            (height_only.max_width, height_only.max_height),
        ],
        [(Some(500), None), (None, Some(300))]
    );
}

#[test]
fn a_switched_on_limit_without_a_usable_maximum_leaves_its_axis_uncapped() {
    for maximum in [None, Some(Variant::String("wide".to_owned()))] {
        let mut config = stored(&[(CARD_WIDTH_LIMITED, Variant::Bool(true))]);
        if let Some(value) = maximum.clone() {
            config.insert(CARD_MAX_WIDTH.to_owned(), value);
        }

        assert_eq!(card(config).max_width, None, "a maximum of {maximum:?}");
    }
}

#[test]
fn a_stored_card_size_outside_the_form_range_is_clamped_into_it() {
    for (minimum, maximum, expected_min, expected_max) in [
        (-1, CARD_MAX_SIZE_MIN_PX - 1, 0, CARD_MAX_SIZE_MIN_PX),
        (0, CARD_MAX_SIZE_MIN_PX, 0, CARD_MAX_SIZE_MIN_PX),
        (1, CARD_MAX_SIZE_MIN_PX + 1, 1, CARD_MAX_SIZE_MIN_PX + 1),
        (
            CARD_SIZE_MAX_PX,
            CARD_SIZE_MAX_PX,
            CARD_SIZE_MAX_PX,
            CARD_SIZE_MAX_PX,
        ),
        (
            CARD_SIZE_MAX_PX + 1,
            CARD_SIZE_MAX_PX + 1,
            CARD_SIZE_MAX_PX,
            CARD_SIZE_MAX_PX,
        ),
        (i64::MIN, i64::MAX, 0, CARD_SIZE_MAX_PX),
    ] {
        let mut config = both_limits(Variant::Bool(true), Variant::Bool(true), maximum, maximum);
        config.extend(stored(&[
            (CARD_MIN_WIDTH, Variant::Int(minimum)),
            (CARD_MIN_HEIGHT, Variant::Int(minimum)),
        ]));
        let card = card(config);
        let expected_min = u32::try_from(expected_min).expect("a pixel count");
        let expected_max = u32::try_from(expected_max).expect("a pixel count");

        assert_eq!(
            (
                card.min_width,
                card.min_height,
                card.max_width,
                card.max_height
            ),
            (
                expected_min,
                expected_min,
                Some(expected_max),
                Some(expected_max)
            ),
            "stored min {minimum}, max {maximum}"
        );
    }
}

#[test]
fn a_card_minimum_stored_with_the_wrong_type_lets_the_card_shrink_to_its_content() {
    let card = card(stored(&[
        (CARD_MIN_WIDTH, Variant::String("wide".to_owned())),
        (CARD_MIN_HEIGHT, Variant::Bool(true)),
    ]));

    assert_eq!((card.min_width, card.min_height), (0, 0));
}

#[test]
fn only_the_ellipsis_choice_cuts_the_text_and_anything_else_wraps_it() {
    for (value, expected) in [
        (
            Variant::String(TEXT_OVERFLOW_ELLIPSIS.to_owned()),
            PreviewTextOverflow::Ellipsis,
        ),
        (
            Variant::String(TEXT_OVERFLOW_WRAP.to_owned()),
            PreviewTextOverflow::Wrap,
        ),
        (
            Variant::String("ELLIPSIS".to_owned()),
            PreviewTextOverflow::Wrap,
        ),
        (
            Variant::String("clip".to_owned()),
            PreviewTextOverflow::Wrap,
        ),
        (Variant::String(String::new()), PreviewTextOverflow::Wrap),
        (Variant::Int(1), PreviewTextOverflow::Wrap),
    ] {
        assert_eq!(
            card(stored(&[(TEXT_OVERFLOW, value.clone())])).overflow,
            expected,
            "{value:?}"
        );
    }
}

#[test]
fn a_look_that_draws_no_alert_card_previews_no_card_even_with_card_keys_stored() {
    let bounded = stored(&[
        (CARD_MIN_WIDTH, Variant::Int(300)),
        (CARD_WIDTH_LIMITED, Variant::Bool(true)),
        (CARD_MAX_WIDTH, Variant::Int(500)),
        (
            TEXT_OVERFLOW,
            Variant::String(TEXT_OVERFLOW_ELLIPSIS.to_owned()),
        ),
    ]);

    for kind_id in [CHAT_KIND, FRAME_KIND, GOAL_KIND, TICKER_KIND] {
        assert_eq!(
            composition(kind_id, bounded.clone()).card,
            None,
            "{kind_id}"
        );
    }
}

#[test]
fn the_alert_form_accepts_the_card_ranges_it_offers_and_refuses_one_past_them() {
    let reg = registry();
    let descriptor = reg.get(ALERT_KIND).expect("the alert kind ships");

    for (config, accepted) in [
        (stored(&[(CARD_MIN_WIDTH, Variant::Int(0))]), true),
        (
            stored(&[(CARD_MIN_HEIGHT, Variant::Int(CARD_SIZE_MAX_PX))]),
            true,
        ),
        (stored(&[(CARD_MIN_WIDTH, Variant::Int(-1))]), false),
        (
            stored(&[(CARD_MIN_HEIGHT, Variant::Int(CARD_SIZE_MAX_PX + 1))]),
            false,
        ),
        (
            stored(&[(CARD_MAX_WIDTH, Variant::Int(CARD_MAX_SIZE_MIN_PX))]),
            true,
        ),
        (
            stored(&[(CARD_MAX_HEIGHT, Variant::Int(CARD_MAX_SIZE_MIN_PX - 1))]),
            false,
        ),
        (
            stored(&[(CARD_MAX_WIDTH, Variant::Int(CARD_SIZE_MAX_PX + 1))]),
            false,
        ),
        (
            stored(&[(CARD_WIDTH_LIMITED, Variant::String("yes".to_owned()))]),
            false,
        ),
        (
            stored(&[(TEXT_OVERFLOW, Variant::String("clip".to_owned()))]),
            false,
        ),
    ] {
        assert_eq!(
            validate_overlay_config(descriptor, &config).is_ok(),
            accepted,
            "{config:?}"
        );
    }
}

#[test]
fn the_alert_script_sizes_the_card_only_from_keys_its_form_offers() {
    with_kind(ALERT_KIND, |descriptor| {
        let behavior = card_bounds_table(descriptor.page_assets().behavior);
        let offered = offered_keys(descriptor);
        let read: Vec<String> = quoted_after(behavior, "key: \"")
            .into_iter()
            .chain(quoted_after(behavior, "limitedBy: \""))
            .collect();

        assert_eq!(
            read.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                CARD_MIN_WIDTH,
                CARD_MIN_HEIGHT,
                CARD_MAX_WIDTH,
                CARD_MAX_HEIGHT,
                CARD_WIDTH_LIMITED,
                CARD_HEIGHT_LIMITED,
            ])
        );
        for key in &read {
            assert!(
                offered.contains(key.as_str()),
                "the script sizes the card from {key}, which the form never offers"
            );
        }
    });
}

#[test]
fn the_alert_script_cuts_text_on_the_exact_choice_the_form_stores() {
    let behavior = with_kind(ALERT_KIND, |descriptor| descriptor.page_assets().behavior);

    assert!(
        behavior.contains(&format!(
            "var TEXT_OVERFLOW_ELLIPSIS = \"{TEXT_OVERFLOW_ELLIPSIS}\";"
        )),
        "the page compares text_overflow against a value the form never stores"
    );
}

#[test]
fn every_card_property_and_class_the_script_sets_has_a_stylesheet_rule_reading_it() {
    let (behavior, style) = with_kind(ALERT_KIND, |descriptor| {
        let assets = descriptor.page_assets();
        (assets.behavior, assets.style)
    });
    let bounds = card_bounds_table(behavior);
    let properties = quoted_after(bounds, "property: \"");
    let classes: Vec<String> = quoted_after(bounds, "cappedClass: \"")
        .into_iter()
        .chain(quoted_after(behavior, "TEXT_CUT_CLASS = \""))
        .collect();

    assert!(!properties.is_empty() && !classes.is_empty());
    for property in &properties {
        assert!(
            style.contains(&format!("var({property}")),
            "the script sets {property} and the stylesheet never reads it"
        );
    }
    for class in &classes {
        assert!(
            style.contains(&format!("#stage.{class} ")),
            "the script marks the card '{class}' and the stylesheet has no rule for it"
        );
    }
}
