#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use forge_overlay::{
    LatestBinding, OverlayConfig, OverlayKindDescriptor, OverlayKindRegistry, SampleContext,
    latest_binding, latest_content, register_builtin_kinds, sample_content,
};
use forge_types::{LatestScope, LatestValue, NOW_PLAYING_SLOT, Variant, latest_fields};
use time::OffsetDateTime;

const LATEST_KIND: &str = "overlay.latest";

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn config(pairs: &[(&str, &str)]) -> OverlayConfig {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), text(value)))
        .collect()
}

fn donation(fields: &[(&str, &str)]) -> LatestValue {
    LatestValue::new(
        "donatello",
        OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(),
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), text(value)))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn with_latest<R>(body: impl FnOnce(&dyn OverlayKindDescriptor) -> R) -> R {
    let reg = registry();
    body(reg.get(LATEST_KIND).expect("the latest look is registered"))
}

#[test]
fn a_filled_slot_renders_the_label_headline_and_subline_expanded_from_the_record() {
    let stored = config(&[
        ("label", "Top of %platform%"),
        ("headline", "%user_name%!"),
        ("subline", "%amount_formatted% via %platform%"),
        ("placeholder", "nothing yet"),
    ]);
    let value = donation(&[
        (latest_fields::USER_NAME, "Olena"),
        (latest_fields::AMOUNT_FORMATTED, "150 UAH"),
    ]);

    let content = with_latest(|look| latest_content(look, &stored, Some(&value)));

    assert_eq!(
        content,
        config(&[
            ("label", "Top of donatello"),
            ("headline", "Olena!"),
            ("subline", "150 UAH via donatello"),
        ])
    );
}

#[test]
fn an_empty_slot_renders_the_label_and_the_placeholder_and_no_value_lines() {
    let stored = config(&[
        ("label", "Last tip"),
        ("headline", "%user_name%"),
        ("placeholder", "Be the first"),
    ]);

    let content = with_latest(|look| latest_content(look, &stored, None));

    assert_eq!(
        content,
        config(&[("label", "Last tip"), ("placeholder", "Be the first")])
    );
}

#[test]
fn a_token_the_record_does_not_carry_is_left_verbatim() {
    let stored = config(&[("headline", "%user_name% %no_such_field%")]);
    let value = donation(&[(latest_fields::USER_NAME, "Olena")]);

    let content = with_latest(|look| latest_content(look, &stored, Some(&value)));

    assert_eq!(
        content.get("headline"),
        Some(&text("Olena %no_such_field%"))
    );
}

#[test]
fn a_donor_message_never_reaches_the_page_even_when_the_record_and_config_carry_one() {
    let stored = config(&[("message", "%message%")]);
    let value = donation(&[
        (latest_fields::USER_NAME, "Olena"),
        ("message", "a private note"),
    ]);

    let filled = with_latest(|look| latest_content(look, &stored, Some(&value)));
    let empty = with_latest(|look| latest_content(look, &stored, None));

    for content in [filled, empty] {
        assert!(
            !content.contains_key("message"),
            "the page was handed a message field: {content:?}"
        );
        assert!(
            content
                .values()
                .all(|held| !held.as_str().unwrap_or_default().contains("a private note")),
            "the donor message leaked into another field: {content:?}"
        );
    }
}

#[test]
fn only_the_latest_look_is_bound_to_a_slot() {
    let reg = registry();

    for descriptor in reg.all().filter(|look| look.id() != LATEST_KIND) {
        assert_eq!(
            latest_binding(descriptor, &OverlayConfig::new()),
            None,
            "{} claims a latest-value slot",
            descriptor.id()
        );
    }
}

#[test]
fn the_latest_look_binds_to_the_stored_slot_and_platform_over_its_defaults() {
    let stored = config(&[("slot", NOW_PLAYING_SLOT), ("platform", "spotify")]);

    let binding = with_latest(|look| latest_binding(look, &stored)).expect("a slot binding");

    assert_eq!(
        binding,
        LatestBinding {
            slot: NOW_PLAYING_SLOT.to_owned(),
            platform: "spotify".to_owned(),
        }
    );
    assert_eq!(binding.scope(), LatestScope::Platform("spotify"));
}

#[test]
fn the_sample_for_a_now_playing_slot_draws_a_track_not_a_donation() {
    let stored = config(&[
        ("slot", NOW_PLAYING_SLOT),
        ("headline", "%title%"),
        ("subline", "%artist%"),
    ]);

    let content = with_latest(|look| sample_content(look, &stored, &SampleContext::neutral()));

    assert_eq!(
        (content.get("headline"), content.get("subline")),
        (Some(&text("Midnight Drive")), Some(&text("Nova Lines")))
    );
}
