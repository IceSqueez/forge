#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures assert directly"
)]
mod common;
use common::{
    ALERT_KIND, BLANK_KIND, CARD_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, LATEST_KIND, TICKER_KIND,
    text, with_kind,
};

use std::collections::BTreeSet;
use std::time::Duration;

use forge_overlay::config::DURATION;
use forge_overlay::motion::{
    DEFAULT_ENTRANCE_MS, DEFAULT_EXIT_MS, ENTRANCE, ENTRANCE_MS, ENTRANCE_OPTIONS, EXIT, EXIT_MS,
    EXIT_OPTIONS, INTENSITY, INTENSITY_OPTIONS, MOTION_MS_MAX, MOTION_MS_MIN, NO_MOTION,
    TEXT_EFFECT, TEXT_EFFECT_OPTIONS, TEXT_STAGGER_MS_MAX, TEXT_STAGGER_MS_MIN,
};
use forge_overlay::{
    MOTION_SOURCE, OverlayConfig, OverlayKindDescriptor, effective_overlay_config, exit_tail,
    show_timing,
};
use forge_registry::FormField;
use forge_types::Variant;

const EXITING_KINDS: [&str; 2] = [ALERT_KIND, TICKER_KIND];

fn int_of(config: &OverlayConfig, key: &str) -> Option<i64> {
    config.get(key).and_then(Variant::as_int)
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

fn tail_of(kind_id: &str, stored: &OverlayConfig) -> Duration {
    with_kind(kind_id, |descriptor| exit_tail(descriptor, stored))
}

fn exit_config(exit: &str, exit_ms: Option<Variant>) -> OverlayConfig {
    let mut config = OverlayConfig::from([(EXIT.to_owned(), text(exit))]);
    if let Some(ms) = exit_ms {
        config.insert(EXIT_MS.to_owned(), ms);
    }
    config
}

fn options(list: &[&str]) -> BTreeSet<String> {
    list.iter()
        .filter(|option| **option != NO_MOTION)
        .map(|option| (*option).to_owned())
        .collect()
}

fn engine_table(name: &str) -> &'static str {
    common::js_block(MOTION_SOURCE, &format!("var {name} = {{"))
}

fn engine_table_keys(name: &str) -> BTreeSet<String> {
    common::js_object_keys(MOTION_SOURCE, &format!("var {name} = {{"))
}

fn engine_string(name: &str) -> String {
    let opening = format!("var {name} = \"");
    let start = MOTION_SOURCE
        .find(&opening)
        .unwrap_or_else(|| panic!("the motion engine no longer declares '{opening}'"))
        + opening.len();
    let rest = &MOTION_SOURCE[start..];
    rest[..rest.find('"').unwrap()].to_owned()
}

fn engine_rect_styles() -> BTreeSet<String> {
    assert!(
        MOTION_SOURCE
            .contains("var RECT_STYLES = [FALLBACK_STYLE, POP, WIPE].concat(Object.keys(SLIDES));"),
        "the engine no longer builds its whole-rect styles from fade, pop, wipe and the slides"
    );
    let mut styles = engine_table_keys("SLIDES");
    for name in ["FALLBACK_STYLE", "POP", "WIPE"] {
        styles.insert(engine_string(name));
    }
    styles
}

fn engine_function(name: &str) -> &'static str {
    common::js_function(MOTION_SOURCE, name)
}

#[test]
fn each_look_offers_exactly_the_motion_axes_it_plays() {
    for (kind_id, entrance, text_effect, exit) in [
        (ALERT_KIND, true, true, true),
        (CARD_KIND, true, true, false),
        (TICKER_KIND, true, true, true),
        (GOAL_KIND, true, true, false),
        (LATEST_KIND, true, true, false),
        (CHAT_KIND, true, false, false),
        (FRAME_KIND, true, false, false),
        (BLANK_KIND, false, false, false),
    ] {
        let keys = with_kind(kind_id, offered_keys);

        assert_eq!(
            (
                keys.contains(ENTRANCE),
                keys.contains(TEXT_EFFECT),
                keys.contains(EXIT),
                keys.contains(INTENSITY),
            ),
            (entrance, text_effect, exit, entrance),
            "{kind_id} offers the wrong motion fields"
        );
    }
}

#[test]
fn the_exit_tail_is_the_chosen_exit_length_pulled_into_the_bounds_the_page_plays() {
    let max = MOTION_MS_MAX.unsigned_abs();
    let min = MOTION_MS_MIN.unsigned_abs();
    for (stored_ms, expected_ms) in [
        (Some(Variant::Int(1_200)), 1_200),
        (Some(Variant::Int(MOTION_MS_MIN)), min),
        (Some(Variant::Int(MOTION_MS_MIN - 1)), min),
        (Some(Variant::Int(MOTION_MS_MAX)), max),
        (Some(Variant::Int(MOTION_MS_MAX + 1)), max),
        (Some(Variant::Int(-5)), min),
        (Some(Variant::Int(i64::MAX)), max),
        (None, DEFAULT_EXIT_MS.unsigned_abs()),
    ] {
        let stored = exit_config("shatter", stored_ms.clone());

        assert_eq!(
            tail_of(ALERT_KIND, &stored),
            Duration::from_millis(expected_ms),
            "exit_ms {stored_ms:?} produced the wrong tail"
        );
    }
}

#[test]
fn an_exit_of_none_adds_no_tail() {
    let stored = exit_config(NO_MOTION, Some(Variant::Int(2_000)));

    assert_eq!(tail_of(ALERT_KIND, &stored), Duration::ZERO);
}

#[test]
fn a_look_without_an_exit_axis_adds_no_tail_whatever_its_record_holds() {
    for kind_id in [CARD_KIND, GOAL_KIND, CHAT_KIND, FRAME_KIND, BLANK_KIND] {
        let stored = exit_config("burst", Some(Variant::Int(2_000)));

        assert_eq!(
            tail_of(kind_id, &stored),
            Duration::ZERO,
            "{kind_id} holds a lane for an exit it never plays"
        );
    }
}

#[test]
fn a_shipped_transient_look_leaves_through_a_tail_by_default() {
    for kind_id in EXITING_KINDS {
        assert_eq!(
            tail_of(kind_id, &OverlayConfig::new()),
            Duration::from_millis(DEFAULT_EXIT_MS.unsigned_abs()),
            "{kind_id} does not schedule its default exit"
        );
    }
}

#[test]
fn a_show_is_timed_as_its_window_with_the_exit_tail_appended() {
    let mut stored = exit_config("dust", Some(Variant::Int(700)));
    stored.insert(DURATION.to_owned(), Variant::Int(3));

    let (configured, overridden) = with_kind(ALERT_KIND, |descriptor| {
        (
            show_timing(descriptor, &stored, None).expect("an alert is timed"),
            show_timing(descriptor, &stored, Some(2_000)).expect("an alert is timed"),
        )
    });

    assert_eq!(
        (
            configured.window,
            configured.total(),
            overridden.window,
            overridden.total(),
        ),
        (
            Duration::from_secs(3),
            Duration::from_millis(3_700),
            Duration::from_secs(2),
            Duration::from_millis(2_700),
        ),
        "the exit tail was folded into the window or not appended to it"
    );
}

#[test]
fn a_look_that_applies_content_on_arrival_is_not_timed_at_all() {
    for kind_id in [CARD_KIND, GOAL_KIND, CHAT_KIND, FRAME_KIND] {
        let timing = with_kind(kind_id, |descriptor| {
            show_timing(descriptor, &OverlayConfig::new(), Some(5_000))
        });

        assert!(timing.is_none(), "{kind_id} was given a show timing");
    }
}

#[test]
fn the_page_engine_and_forge_bound_motion_durations_identically() {
    for (constant, value) in [
        ("MOTION_MS_MIN", MOTION_MS_MIN),
        ("MOTION_MS_MAX", MOTION_MS_MAX),
        ("DEFAULT_ENTRANCE_MS", DEFAULT_ENTRANCE_MS),
        ("DEFAULT_EXIT_MS", DEFAULT_EXIT_MS),
        ("STAGGER_MS_MIN", TEXT_STAGGER_MS_MIN),
        ("STAGGER_MS_MAX", TEXT_STAGGER_MS_MAX),
    ] {
        let declaration = format!("var {constant} = {value};");

        assert!(
            MOTION_SOURCE.contains(&declaration),
            "the page does not state '{declaration}', so the page hard-stops an exit at a \
             different moment than forge releases the next show"
        );
    }
}

#[test]
fn every_preset_the_form_offers_is_one_the_page_engine_plays_rather_than_falling_back() {
    let rect = engine_rect_styles();
    let mut entrances = rect.clone();
    entrances.extend(engine_table_keys("PARTICLE_ENTRANCES"));
    let mut exits = rect;
    exits.extend(engine_table_keys("DESTRUCTIVE_EXITS"));

    for (field, offered, played) in [
        (ENTRANCE, options(ENTRANCE_OPTIONS), entrances),
        (EXIT, options(EXIT_OPTIONS), exits),
        (
            TEXT_EFFECT,
            options(TEXT_EFFECT_OPTIONS),
            engine_table_keys("TEXT_EFFECTS"),
        ),
        (
            INTENSITY,
            options(INTENSITY_OPTIONS),
            engine_table_keys("INTENSITIES"),
        ),
    ] {
        assert_eq!(
            offered, played,
            "the {field} choices offered and the ones the page plays differ, so a pick silently \
             plays the fallback"
        );
    }
}

#[test]
fn the_particle_entrances_are_exactly_the_offered_entrances_no_whole_rect_style_plays() {
    let rect = engine_rect_styles();
    let beyond_rect: BTreeSet<String> = options(ENTRANCE_OPTIONS)
        .into_iter()
        .filter(|entrance| !rect.contains(entrance))
        .collect();

    assert_eq!(
        engine_table_keys("PARTICLE_ENTRANCES"),
        beyond_rect,
        "a particle entrance either shadows a whole-rect style or is not offered by the form"
    );
}

#[test]
fn the_page_plan_keeps_a_particle_entrance_instead_of_falling_back() {
    let plan = engine_function("plan");
    let opening = "entrance: style(values.entrance, ";
    let start = plan
        .find(opening)
        .unwrap_or_else(|| panic!("the plan no longer reads the entrance through '{opening}'"))
        + opening.len();
    let predicate = &plan[start..start + plan[start..].find(')').unwrap()];

    assert!(
        engine_function(predicate).contains("owned(PARTICLE_ENTRANCES, value)"),
        "the plan checks the entrance with '{predicate}', which turns every particle entrance \
         into the fallback"
    );
}

#[test]
fn every_motion_key_the_page_reads_is_one_the_alert_document_carries() {
    let plan = engine_function("plan");
    let offered = with_kind(ALERT_KIND, |descriptor| {
        effective_overlay_config(descriptor, &OverlayConfig::new())
            .keys()
            .cloned()
            .chain(offered_keys(descriptor).into_iter().map(str::to_owned))
            .collect::<BTreeSet<String>>()
    });

    for (at, _) in plan.match_indices("values.") {
        let key: String = plan[at + "values.".len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        assert!(
            offered.contains(&key),
            "the page reads '{key}', which no alert field or default carries"
        );
    }
}

const MIN_STAGGER_MS: i64 = 40;
const MAX_STAGGER_MS: i64 = 80;

#[test]
fn the_agreed_motion_pacing_holds_three_intensities_and_a_quick_default_stagger() {
    let staggers: Vec<i64> = engine_table("TEXT_EFFECTS")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("staggerMs: "))
        .map(|value| value.trim_end_matches(',').parse().unwrap())
        .collect();

    assert_eq!(INTENSITY_OPTIONS.len(), 3);
    assert_eq!(staggers.len(), options(TEXT_EFFECT_OPTIONS).len());
    assert!(
        staggers
            .iter()
            .all(|ms| (MIN_STAGGER_MS..=MAX_STAGGER_MS).contains(ms)),
        "{staggers:?}"
    );
    assert!(MOTION_SOURCE.contains("var ENTRANCE_SHARE = 0.75;"));
}

#[test]
fn every_look_but_chat_enters_over_the_agreed_half_second_by_default() {
    for kind_id in [ALERT_KIND, CARD_KIND, TICKER_KIND, GOAL_KIND, FRAME_KIND] {
        let defaults = with_kind(kind_id, |descriptor| descriptor.default_config());

        assert_eq!(
            int_of(&defaults, ENTRANCE_MS),
            Some(500),
            "{kind_id} does not open on the agreed entrance length"
        );
    }
}
