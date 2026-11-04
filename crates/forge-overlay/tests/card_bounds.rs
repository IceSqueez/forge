#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{ALERT_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, TICKER_KIND, registry, with_kind};

use std::collections::BTreeSet;

use forge_overlay::config::{
    CARD_MAX_HEIGHT, CARD_MAX_WIDTH, CARD_MIN_HEIGHT, CARD_MIN_WIDTH, CARD_SIZE_MAX_PX,
    TEXT_OVERFLOW, TEXT_OVERFLOW_ELLIPSIS, TEXT_OVERFLOW_WRAP,
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

fn maxima(width: i64, height: i64) -> OverlayConfig {
    stored(&[
        (CARD_MAX_WIDTH, Variant::Int(width)),
        (CARD_MAX_HEIGHT, Variant::Int(height)),
    ])
}

fn pixels(value: i64) -> u32 {
    u32::try_from(value).expect("a pixel count")
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

fn alert_assets() -> (&'static str, &'static str) {
    with_kind(ALERT_KIND, |descriptor| {
        let assets = descriptor.page_assets();
        (assets.behavior, assets.style)
    })
}

#[test]
fn an_untouched_alert_previews_a_card_that_neither_grows_nor_caps_and_wraps_its_text() {
    assert_eq!(card(OverlayConfig::new()), UNBOUNDED);
}

#[test]
fn an_untouched_alert_hands_its_page_zero_for_every_card_bound_and_wrapping_text() {
    let reg = registry();
    let descriptor = reg.get(ALERT_KIND).expect("the alert kind ships");
    let mut instance = common::sub_alert_instance(ALERT_KIND);
    instance.config = OverlayConfig::new();
    instance.media = OverlayMedia::default();
    let document = config_document(&instance, descriptor).expect("the config document builds");
    let config = serde_json::from_str::<Value>(&document).expect("valid JSON")["config"].clone();

    assert_eq!(
        [
            CARD_MIN_WIDTH,
            CARD_MIN_HEIGHT,
            CARD_MAX_WIDTH,
            CARD_MAX_HEIGHT,
            TEXT_OVERFLOW,
        ]
        .map(|key| config[key].clone()),
        [
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(0),
            Value::from(TEXT_OVERFLOW_WRAP),
        ]
    );
}

#[test]
fn a_positive_maximum_caps_only_its_own_axis_at_that_maximum() {
    let width_only = card(maxima(500, 0));
    let height_only = card(maxima(0, 300));

    assert_eq!(
        [
            (width_only.max_width, width_only.max_height),
            (height_only.max_width, height_only.max_height),
        ],
        [(Some(500), None), (None, Some(300))]
    );
}

#[test]
fn a_zero_absent_negative_or_mistyped_maximum_leaves_its_axis_uncapped() {
    for maximum in [
        None,
        Some(Variant::Int(0)),
        Some(Variant::Int(-1)),
        Some(Variant::Int(i64::MIN)),
        Some(Variant::String("wide".to_owned())),
        Some(Variant::Bool(true)),
    ] {
        let mut config = OverlayConfig::new();
        if let Some(value) = maximum.clone() {
            config.insert(CARD_MAX_WIDTH.to_owned(), value.clone());
            config.insert(CARD_MAX_HEIGHT.to_owned(), value);
        }
        let card = card(config);

        assert_eq!(
            (card.max_width, card.max_height),
            (None, None),
            "a maximum of {maximum:?}"
        );
    }
}

#[test]
fn a_stored_card_maximum_is_kept_from_one_pixel_and_clamped_above_the_form_range() {
    for (maximum, expected) in [
        (1, 1),
        (2, 2),
        (CARD_SIZE_MAX_PX - 1, CARD_SIZE_MAX_PX - 1),
        (CARD_SIZE_MAX_PX, CARD_SIZE_MAX_PX),
        (CARD_SIZE_MAX_PX + 1, CARD_SIZE_MAX_PX),
        (i64::MAX, CARD_SIZE_MAX_PX),
    ] {
        let card = card(maxima(maximum, maximum));

        assert_eq!(
            (card.max_width, card.max_height),
            (Some(pixels(expected)), Some(pixels(expected))),
            "stored max {maximum}"
        );
    }
}

#[test]
fn a_stored_card_minimum_outside_the_form_range_is_clamped_into_it() {
    for (minimum, expected) in [
        (i64::MIN, 0),
        (-1, 0),
        (0, 0),
        (1, 1),
        (CARD_SIZE_MAX_PX, CARD_SIZE_MAX_PX),
        (CARD_SIZE_MAX_PX + 1, CARD_SIZE_MAX_PX),
    ] {
        let card = card(stored(&[
            (CARD_MIN_WIDTH, Variant::Int(minimum)),
            (CARD_MIN_HEIGHT, Variant::Int(minimum)),
        ]));

        assert_eq!(
            (card.min_width, card.min_height),
            (pixels(expected), pixels(expected)),
            "stored min {minimum}"
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
fn the_alert_form_accepts_zero_to_the_card_range_on_every_bound_and_refuses_one_past_it() {
    let reg = registry();
    let descriptor = reg.get(ALERT_KIND).expect("the alert kind ships");

    for key in [
        CARD_MIN_WIDTH,
        CARD_MIN_HEIGHT,
        CARD_MAX_WIDTH,
        CARD_MAX_HEIGHT,
    ] {
        for (value, accepted) in [
            (-1, false),
            (0, true),
            (1, true),
            (CARD_SIZE_MAX_PX, true),
            (CARD_SIZE_MAX_PX + 1, false),
        ] {
            assert_eq!(
                validate_overlay_config(descriptor, &stored(&[(key, Variant::Int(value))])).is_ok(),
                accepted,
                "{key} = {value}"
            );
        }
    }
}

#[test]
fn the_alert_form_refuses_a_text_overflow_choice_it_never_offers() {
    let reg = registry();
    let descriptor = reg.get(ALERT_KIND).expect("the alert kind ships");
    let config = stored(&[(TEXT_OVERFLOW, Variant::String("clip".to_owned()))]);

    assert!(validate_overlay_config(descriptor, &config).is_err());
}

#[test]
fn the_alert_script_sizes_the_card_from_exactly_the_four_bounds_its_form_offers() {
    with_kind(ALERT_KIND, |descriptor| {
        let behavior = card_bounds_table(descriptor.page_assets().behavior);
        let offered = offered_keys(descriptor);
        let read: BTreeSet<String> = quoted_after(behavior, "key: \"")
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
    let (behavior, _) = alert_assets();

    assert!(
        behavior.contains(&format!(
            "var TEXT_OVERFLOW_ELLIPSIS = \"{TEXT_OVERFLOW_ELLIPSIS}\";"
        )),
        "the page compares text_overflow against a value the form never stores"
    );
}

#[test]
fn every_card_property_the_script_may_leave_unset_falls_back_to_no_bound_in_the_stylesheet() {
    let (behavior, style) = alert_assets();
    let properties = quoted_after(card_bounds_table(behavior), "property: \"");

    assert_eq!(properties.len(), 4, "one property per card bound");
    for property in &properties {
        let no_bound = if property.starts_with("--card-max-") {
            "100%"
        } else {
            "0px"
        };
        assert!(
            style.contains(&format!("var({property}, {no_bound})")),
            "the script leaves {property} unset for a zero bound and the stylesheet does not \
             fall back to {no_bound}"
        );
    }
}

#[test]
fn the_text_cut_class_the_script_sets_has_a_stylesheet_rule() {
    let (behavior, style) = alert_assets();
    let classes = quoted_after(behavior, "TEXT_CUT_CLASS = \"");

    assert_eq!(classes.len(), 1, "the script declares its text-cut class");
    for class in &classes {
        assert!(
            style.contains(&format!("#stage.{class} ")),
            "the script marks the card '{class}' and the stylesheet has no rule for it"
        );
    }
}
