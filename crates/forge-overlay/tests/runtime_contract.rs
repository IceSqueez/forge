#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::config::{ACCENT_OPTIONS, ELEMENT_HEIGHT, ELEMENT_WIDTH, TEXT_SIZE};
use forge_overlay::metrics::{
    ELEMENT_HEIGHT_PROPERTY, ELEMENT_WIDTH_PROPERTY, TEXT_SCALE_PROPERTY, TEXT_SIZE_PROPERTY,
};
use forge_overlay::{
    OverlayKindRegistry, PREVIEW_PARAM, PREVIEW_VALUE, RUNTIME_SOURCE, SAMPLE_FILE,
    register_builtin_kinds,
};

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn function_body(name: &str) -> &'static str {
    let opening = format!("function {name}(");
    let start = RUNTIME_SOURCE
        .find(&opening)
        .unwrap_or_else(|| panic!("the runtime no longer declares '{opening}'"));
    let rest = &RUNTIME_SOURCE[start..];
    let end = rest
        .find("\n  }")
        .unwrap_or_else(|| panic!("'{opening}' is never closed"));
    &rest[..end]
}

fn at(body: &str, needle: &str) -> usize {
    body.find(needle)
        .unwrap_or_else(|| panic!("the runtime no longer contains '{needle}'"))
}

fn behind_the_preview_gate(body: &str, needle: &str) -> bool {
    let gate = at(body, "if (!previewing)");
    at(body, needle) > at(&body[gate..], "return;") + gate
}

fn block_after(source: &str, opening: &str) -> String {
    let start = source
        .find(opening)
        .unwrap_or_else(|| panic!("the runtime no longer declares '{opening}'"))
        + opening.len();
    let rest = &source[start..];
    let end = rest
        .find('}')
        .unwrap_or_else(|| panic!("'{opening}' is never closed"));
    rest[..end].to_owned()
}

fn keys_in(block: &str) -> BTreeSet<String> {
    block
        .lines()
        .filter_map(|line| line.split(':').next())
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .collect()
}

fn members_called_on(source: &str, receiver: &str) -> BTreeSet<String> {
    source
        .match_indices(receiver)
        .map(|(at, _)| {
            source[at + receiver.len()..]
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect()
        })
        .filter(|member: &String| !member.is_empty())
        .collect()
}

#[test]
fn the_runtime_carries_a_hex_value_for_every_accent_the_form_offers() {
    let mapped = keys_in(&block_after(RUNTIME_SOURCE, "var ACCENT_HEX = {"));
    let offered: BTreeSet<String> = ACCENT_OPTIONS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();

    assert_eq!(
        mapped, offered,
        "an accent the runtime cannot map falls back to mauve on every overlay that picks it"
    );
}

#[test]
fn every_helper_a_generated_page_calls_is_one_the_runtime_publishes() {
    let published = keys_in(&block_after(
        RUNTIME_SOURCE,
        "window.forge = Object.freeze({",
    ));

    for descriptor in registry().all() {
        for called in members_called_on(descriptor.page_assets().behavior, "forge.") {
            assert!(
                published.contains(&called),
                "{} calls forge.{called}, which the runtime never publishes",
                descriptor.id()
            );
        }
    }
}

#[test]
fn the_runtime_previews_a_page_on_the_flag_this_build_writes_into_its_url() {
    for declaration in [
        format!("var PREVIEW_PARAM = \"{PREVIEW_PARAM}\";"),
        format!("var PREVIEW_VALUE = \"{PREVIEW_VALUE}\";"),
    ] {
        assert!(
            RUNTIME_SOURCE.contains(&declaration),
            "the runtime does not state '{declaration}', so a preview link opens an ordinary page"
        );
    }
}

#[test]
fn the_runtime_fetches_the_sample_document_this_build_generates() {
    let declaration = format!("var SAMPLE_FILE = \"./{SAMPLE_FILE}\";");

    assert!(
        RUNTIME_SOURCE.contains(&declaration),
        "the runtime does not state '{declaration}', so a preview page fetches a file nothing writes"
    );
}

#[test]
fn the_runtime_publishes_the_size_properties_the_stylesheets_read_and_leaves_the_scale_to_them() {
    for declaration in [
        format!("var ELEMENT_WIDTH_PROPERTY = \"{ELEMENT_WIDTH_PROPERTY}\";"),
        format!("var ELEMENT_HEIGHT_PROPERTY = \"{ELEMENT_HEIGHT_PROPERTY}\";"),
        format!("var TEXT_SIZE_PROPERTY = \"{TEXT_SIZE_PROPERTY}\";"),
    ] {
        assert!(
            RUNTIME_SOURCE.contains(&declaration),
            "the runtime does not state '{declaration}', so a page is handed a property no \
             stylesheet spends"
        );
    }

    assert!(
        !RUNTIME_SOURCE.contains(&format!("\"{TEXT_SCALE_PROPERTY}\"")),
        "the runtime names {TEXT_SCALE_PROPERTY} as a value it can publish, which would scale \
         every kind by one ratio instead of by its own base"
    );
}

#[test]
fn a_size_reaches_the_page_in_the_unit_the_stylesheet_that_spends_it_expects() {
    for call in [
        format!("applySize(ELEMENT_WIDTH_PROPERTY, values.{ELEMENT_WIDTH}, PIXEL_UNIT);"),
        format!("applySize(ELEMENT_HEIGHT_PROPERTY, values.{ELEMENT_HEIGHT}, PIXEL_UNIT);"),
        format!("applySize(TEXT_SIZE_PROPERTY, values.{TEXT_SIZE}, NO_UNIT);"),
    ] {
        assert!(
            RUNTIME_SOURCE.contains(&call),
            "the runtime does not state '{call}', so a size arrives in a unit its stylesheet \
             cannot multiply"
        );
    }

    for unit in ["var PIXEL_UNIT = \"px\";", "var NO_UNIT = \"\";"] {
        assert!(
            RUNTIME_SOURCE.contains(unit),
            "the runtime does not state '{unit}'"
        );
    }
}

#[test]
fn a_size_the_record_leaves_out_is_taken_off_the_page_rather_than_published_as_zero() {
    let apply = function_body("applySize");

    assert!(
        apply.contains("value > 0"),
        "the runtime publishes a size of zero, which collapses the element the fallback would size"
    );
    assert!(
        apply.contains("removeProperty(property)"),
        "a size the record leaves out keeps whatever the property held, so the stylesheet fallback \
         never applies"
    );
}

#[test]
fn a_page_opened_without_the_preview_flag_never_delivers_the_sample_document() {
    let start = function_body("start");

    assert!(
        behind_the_preview_gate(start, "loadSample("),
        "a browser source on a live stream would deliver sample content to itself"
    );
}

#[test]
fn the_checkerboard_backdrop_is_painted_only_while_previewing() {
    let start = function_body("start");

    assert_eq!(
        RUNTIME_SOURCE.matches("paintCheckerboard();").count(),
        1,
        "the backdrop is painted from somewhere other than the preview branch"
    );
    assert!(
        behind_the_preview_gate(start, "paintCheckerboard();"),
        "a transparent browser source would be painted over with the preview backdrop"
    );
}

#[test]
fn a_transient_overlay_is_never_hidden_on_a_timer_while_previewing() {
    let show = function_body("show");

    assert!(
        at(show, "previewing ||") < at(show, "window.setTimeout("),
        "a previewed overlay disappears before it can be looked at"
    );
}
