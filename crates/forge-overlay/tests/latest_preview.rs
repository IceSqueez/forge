#![allow(clippy::expect_used)]
mod common;
use common::{LATEST_KIND, registry};

use forge_overlay::config::{HEADLINE, SLOT};
use forge_overlay::{OverlayConfig, PreviewLineRole, PreviewMark, PreviewShape};
use forge_types::{LATEST_DONATION_SLOT, NOW_PLAYING_SLOT, Variant};

fn latest_config(overrides: &[(&str, &str)]) -> OverlayConfig {
    let reg = registry();
    let look = reg.get(LATEST_KIND).expect("the latest look");
    let mut config = look.default_config();
    for (key, value) in overrides {
        config.insert((*key).to_owned(), Variant::String((*value).to_owned()));
    }
    config
}

fn preview(overrides: &[(&str, &str)]) -> forge_overlay::PreviewComposition {
    let reg = registry();
    let look = reg.get(LATEST_KIND).expect("the latest look");
    look.preview(&latest_config(overrides))
}

#[test]
fn the_latest_preview_draws_a_card_with_label_headline_and_subline_expanded_from_the_sample() {
    let composition = preview(&[]);

    assert_eq!(composition.shape, PreviewShape::LatestCard);
    let lines: Vec<(PreviewLineRole, &str)> = composition
        .lines
        .iter()
        .map(|line| (line.role, line.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        vec![
            (PreviewLineRole::Label, "Last donation"),
            (PreviewLineRole::Headline, "PixelPal"),
            (PreviewLineRole::Subline, "100.00 UAH"),
        ]
    );
}

#[test]
fn a_different_headline_template_changes_the_previewed_headline() {
    let composition = preview(&[(HEADLINE, "%user_name% donated")]);

    let headline = composition
        .lines
        .iter()
        .find(|line| line.role == PreviewLineRole::Headline)
        .expect("a headline line");
    assert_eq!(headline.text, "PixelPal donated");
}

#[test]
fn the_preview_mark_follows_the_slot_and_is_absent_for_a_slot_without_one() {
    for (slot, expected) in [
        (LATEST_DONATION_SLOT, Some(PreviewMark::Donation)),
        (NOW_PLAYING_SLOT, Some(PreviewMark::NowPlaying)),
        ("unknown_slot", None),
    ] {
        assert_eq!(
            preview(&[(SLOT, slot)]).mark,
            expected,
            "mark for slot {slot:?}"
        );
    }
}
