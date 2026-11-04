#![expect(clippy::expect_used, reason = "test fixtures assert directly")]
mod common;
use common::{ALERT_KIND, CARD_KIND, js_block, js_function, registry, text, with_kind};

use forge_overlay::config::{DURATION, FONT, HEADLINE, ICON, IMAGE, SPEECH, SUBLINE, TEXT_COLOR};
use forge_overlay::motion::{EXIT, EXIT_MS};
use forge_overlay::{
    ConfigSection, OverlayConfig, OverlayKindDescriptor, effective_overlay_config,
};
use forge_registry::FormField;
use forge_types::NamesLanguage;

const SHOW_ENDING_KEYS: [&str; 3] = [DURATION, EXIT, EXIT_MS];

const LOOK_HELPERS: [&str; 5] = [
    "placeIcon",
    "paintSurface",
    "sizeCard",
    "placeImage",
    "playImage",
];

const LOOK_CONSTANTS: [&str; 2] = ["STAGE_SELECTOR", "SHOW_IMAGE_KEY"];

fn layout(descriptor: &dyn OverlayKindDescriptor) -> Vec<(ConfigSection, bool, &'static str)> {
    descriptor
        .config_fields()
        .iter()
        .flat_map(|sectioned| {
            let keys = match &sectioned.field {
                FormField::Optional { key, inner, .. } => vec![*key, inner.key()],
                field => vec![field.key()],
            };
            keys.into_iter()
                .map(|key| (sectioned.section, sectioned.delivered, key))
        })
        .collect()
}

fn one(key: &str, value: &str) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), text(value))])
}

fn opening(stored: &OverlayConfig) -> OverlayConfig {
    with_kind(CARD_KIND, |card| card.opening_content(stored))
        .expect("a card opens on the content its panel holds")
}

fn replaces(before: &OverlayConfig, after: &OverlayConfig) -> bool {
    with_kind(CARD_KIND, |card| card.panel_replaces_content(before, after))
}

fn behavior(kind_id: &str) -> &'static str {
    with_kind(kind_id, |descriptor| descriptor.page_assets().behavior)
}

fn card_script() -> &'static str {
    let composed = behavior(CARD_KIND);
    let own = composed
        .find("forge.ready(")
        .expect("the card script waits for its config");
    &composed[own..]
}

#[test]
fn the_card_offers_the_alert_form_minus_every_field_that_ends_a_show() {
    let alert: Vec<_> = with_kind(ALERT_KIND, layout)
        .into_iter()
        .filter(|(_, _, key)| !SHOW_ENDING_KEYS.contains(key))
        .collect();

    assert_eq!(with_kind(CARD_KIND, layout), alert);
}

#[test]
fn a_card_opens_on_the_words_its_panel_saved() {
    let mut stored = one(HEADLINE, "Be right back");
    stored.insert(SUBLINE.to_owned(), text("Grabbing tea"));

    let content = opening(&stored);

    assert_eq!(
        (content.get(HEADLINE), content.get(SUBLINE)),
        (Some(&text("Be right back")), Some(&text("Grabbing tea")))
    );
}

#[test]
fn a_card_opens_on_its_default_words_before_the_panel_saves_any() {
    let content = opening(&OverlayConfig::new());

    assert_eq!(
        (content.get(HEADLINE), content.get(SUBLINE)),
        (
            Some(&text("Welcome to the stream")),
            Some(&text("Say hi in chat"))
        )
    );
}

#[test]
fn a_card_opening_never_speaks_even_with_a_saved_speech_line() {
    let content = opening(&one(SPEECH, "Welcome everyone"));

    assert!(
        !content.contains_key(SPEECH),
        "a reconnecting page would read the speech line aloud again: {content:?}"
    );
}

#[test]
fn no_kind_but_the_card_opens_on_or_replaces_panel_content() {
    let edited = one(HEADLINE, "Edited");
    for descriptor in registry().all().filter(|kind| kind.id() != CARD_KIND) {
        let defaults = descriptor.default_config();

        assert!(
            descriptor.opening_content(&defaults).is_none(),
            "{} replays its panel text to a page nobody sent content to",
            descriptor.id()
        );
        assert!(
            !descriptor.panel_replaces_content(&defaults, &edited),
            "{} overwrites delivered content from its panel",
            descriptor.id()
        );
    }
}

#[test]
fn editing_the_cards_words_icon_or_image_replaces_what_it_shows() {
    for (key, value) in [
        (HEADLINE, "Starting soon"),
        (SUBLINE, "Grab a drink"),
        (ICON, "star"),
        (
            IMAGE,
            "sha256-0000000000000000000000000000000000000000000000000000000000000000",
        ),
    ] {
        assert!(
            replaces(&OverlayConfig::new(), &one(key, value)),
            "a new {key} left the old content on screen"
        );
    }
}

#[test]
fn restyling_the_card_keeps_what_it_shows() {
    for (key, value) in [(TEXT_COLOR, "#ff8800"), (FONT, "Inter")] {
        assert!(
            !replaces(&OverlayConfig::new(), &one(key, value)),
            "changing {key} threw away the content on screen"
        );
    }
}

#[test]
fn saving_unchanged_words_keeps_what_the_card_shows() {
    let saved = one(HEADLINE, "Starting soon");

    assert!(!replaces(&saved, &saved.clone()));
}

#[test]
fn saving_a_word_at_its_default_value_is_no_content_change() {
    assert!(!replaces(
        &OverlayConfig::new(),
        &one(HEADLINE, "Welcome to the stream")
    ));
}

#[test]
fn the_card_preview_draws_its_picture_surface_and_card_box_like_the_alert() {
    let stored = one(IMAGE, "sha256-picture");
    let drawn = |kind_id| {
        with_kind(kind_id, |descriptor| {
            let shown = descriptor.preview(
                &effective_overlay_config(descriptor, &stored),
                NamesLanguage::En,
            );
            (shown.image, shown.surface, shown.card)
        })
    };

    let card = drawn(CARD_KIND);

    assert!(
        card.0.is_some() && card.1.is_some() && card.2.is_some(),
        "the card preview leaves out a part its page draws: {card:?}"
    );
    assert_eq!(card, drawn(ALERT_KIND));
}

#[test]
fn a_card_page_stays_blank_until_content_is_delivered() {
    let markup = with_kind(CARD_KIND, |card| card.page_assets().markup);
    let script = card_script();
    let ready = js_block(script, "forge.ready(function (config) {");
    let delivery = js_block(script, "forge.content(function (values) {");
    let before_delivery = ready.replace(delivery, "");

    assert!(
        markup.contains(r#"<div id="stage" class="hidden">"#),
        "the card is visible before anything was delivered"
    );
    assert!(
        !before_delivery.contains("present(") && !before_delivery.contains("forge.show("),
        "the card reveals itself on load instead of on a delivered frame"
    );
    assert!(
        delivery.contains("present("),
        "a delivered frame never reaches the card"
    );
}

#[test]
fn a_delivered_card_stays_on_screen_until_the_next_delivery() {
    let present = js_function(card_script(), "present");

    assert!(
        present.contains("forge.show(STAGE_SELECTOR);"),
        "the card reveals itself on a timer, so it leaves the screen on its own"
    );
}

#[test]
fn an_empty_delivery_hides_the_card_instead_of_showing_an_empty_box() {
    let present = js_function(card_script(), "present");

    assert!(
        present.contains(r#"querySelector(STAGE_SELECTOR).classList.add("hidden")"#),
        "a frame with no words, icon or image leaves an empty card on screen"
    );
}

#[test]
fn a_card_part_with_nothing_in_it_takes_no_room() {
    let present = js_function(card_script(), "present");
    let style = with_kind(CARD_KIND, |card| card.page_assets().style);

    assert!(
        present.contains("var worded = Boolean(headline) || Boolean(subline);")
            && present.contains("querySelector(BODY_SELECTOR).hidden = !worded;")
            && present.contains("querySelector(MESSAGE_SELECTOR).hidden = !worded && !iconed;"),
        "an empty text block or message row is kept in the layout"
    );
    assert!(
        style.contains("\n[hidden] {\n  display: none !important;\n}"),
        "a hidden part is still laid out by its own display rule and leaves a gap"
    );
}

#[test]
fn every_look_helper_a_card_kind_calls_is_declared_in_its_composed_behavior() {
    for kind_id in [ALERT_KIND, CARD_KIND] {
        let composed = behavior(kind_id);
        for helper in LOOK_HELPERS {
            assert!(
                composed.contains(&format!("function {helper}(")),
                "{kind_id} calls {helper} but its page never declares it"
            );
        }
        for constant in LOOK_CONSTANTS {
            assert!(
                composed.contains(&format!("var {constant} =")),
                "{kind_id} reads {constant} but its page never declares it"
            );
        }
    }
}

#[test]
fn the_alert_still_leaves_the_screen_after_its_window() {
    let alert = behavior(ALERT_KIND);

    assert!(
        alert.contains(
            "forge.show(STAGE_SELECTOR, durationMs || config.duration * 1000 || FALLBACK_MS);"
        ),
        "the alert lost its timed show and would stay on screen like a card"
    );
}
