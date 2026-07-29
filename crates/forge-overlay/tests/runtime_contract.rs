#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::config::ACCENT_OPTIONS;
use forge_overlay::{OverlayKindRegistry, RUNTIME_SOURCE, register_builtin_kinds};

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
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

/// Drift tripwire: `tpl` is a hand port of `ArgStack::interpolate` and no test can execute the JS.
#[test]
fn the_template_expander_still_scans_by_hand_rather_than_by_regular_expression() {
    let body = &RUNTIME_SOURCE[RUNTIME_SOURCE
        .find("function tpl(")
        .expect("the runtime no longer declares tpl")..];

    for (marker, behaviour) in [
        (".trim()", "the token is trimmed before lookup"),
        (
            "\"%\" + name + \"%\"",
            "an unknown token is reprinted untrimmed",
        ),
        (
            "!closed",
            "an unterminated tail collapses to a lone percent",
        ),
    ] {
        assert!(
            body.contains(marker),
            "tpl no longer shows '{marker}' - re-verify {behaviour} against ArgStack::interpolate before updating this guard"
        );
    }
}
