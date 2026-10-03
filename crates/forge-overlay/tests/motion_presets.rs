#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::time::Duration;

use forge_overlay::config::{ANIMATION, ANIMATION_OPTIONS, DESIGN_WIDTH, DURATION};
use forge_overlay::motion::{
    DEFAULT_ENTRANCE_MS, DEFAULT_EXIT_MS, ENTRANCE, ENTRANCE_MS, ENTRANCE_OPTIONS, EXIT, EXIT_MS,
    EXIT_OPTIONS, INTENSITY, INTENSITY_OPTIONS, MOTION_MS_MAX, MOTION_MS_MIN, NO_MOTION,
    TEXT_EFFECT, TEXT_EFFECT_OPTIONS, TEXT_STAGGER_MS_MAX, TEXT_STAGGER_MS_MIN,
};
use forge_overlay::{
    MOTION_SOURCE, OverlayConfig, OverlayKindDescriptor, OverlayKindRegistry,
    effective_overlay_config, exit_tail, register_builtin_kinds, show_timing, upgrade_config,
    validate_overlay_config,
};
use forge_registry::FormField;
use forge_types::Variant;

const ALERT_KIND: &str = "overlay.alert";
const BLANK_KIND: &str = "overlay.blank";
const CHAT_KIND: &str = "overlay.chat";
const FRAME_KIND: &str = "overlay.frame";
const GOAL_KIND: &str = "overlay.goal";
const TICKER_KIND: &str = "overlay.ticker";
const LATEST_KIND: &str = "overlay.latest";

const MOVING_KINDS: [&str; 5] = [ALERT_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, TICKER_KIND];
const EXITING_KINDS: [&str; 2] = [ALERT_KIND, TICKER_KIND];

const PRE_BOX_ALERT_SCHEMA: u32 = 2;
const OLD_REVEAL_MS: i64 = 400;
const OLD_ROW_ENTER_MS: i64 = 200;

const LEGACY_MAPPING: [(&str, &str, &str, &str); 6] = [
    ("fade", "fade", "fade", "fade"),
    ("slide-up", "slide-up", "slide-down", "slide-up"),
    ("slide-down", "slide-down", "slide-up", "slide-down"),
    ("slide-left", "slide-left", "slide-right", "slide-left"),
    ("pop", "pop", "pop", "pop"),
    ("wipe-left", "wipe", "fade", "slide-left"),
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

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn str_of<'a>(config: &'a OverlayConfig, key: &str) -> Option<&'a str> {
    config.get(key).and_then(Variant::as_str)
}

fn int_of(config: &OverlayConfig, key: &str) -> Option<i64> {
    config.get(key).and_then(Variant::as_int)
}

fn before_motion(kind_id: &str, stored: &OverlayConfig) -> OverlayConfig {
    with_kind(kind_id, |descriptor| {
        upgrade_config(descriptor, descriptor.config_schema_version() - 1, stored)
            .expect("a record one schema behind is upgraded")
    })
}

fn with_animation(value: Variant) -> OverlayConfig {
    OverlayConfig::from([(ANIMATION.to_owned(), value)])
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
    let opening = format!("var {name} = {{");
    let start = MOTION_SOURCE
        .find(&opening)
        .unwrap_or_else(|| panic!("the motion engine no longer declares '{opening}'"))
        + opening.len();
    let rest = &MOTION_SOURCE[start..];
    &rest[..rest
        .find("\n  };")
        .unwrap_or_else(|| panic!("'{opening}' is never closed"))]
}

fn engine_table_keys(name: &str) -> BTreeSet<String> {
    engine_table(name)
        .lines()
        .filter(|line| line.starts_with("    ") && !line.starts_with("     "))
        .filter_map(|line| line.trim().split_once(':').map(|(key, _)| key))
        .map(|key| key.trim_matches('"').to_owned())
        .collect()
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
    let opening = format!("function {name}(");
    let start = MOTION_SOURCE
        .find(&opening)
        .unwrap_or_else(|| panic!("the motion engine no longer declares '{opening}'"));
    let rest = &MOTION_SOURCE[start..];
    &rest[..rest
        .find("\n  }")
        .unwrap_or_else(|| panic!("'{opening}' is never closed"))]
}

#[test]
fn every_old_animation_becomes_the_entrance_and_reversed_exit_that_moved_the_same_way() {
    for kind_id in EXITING_KINDS {
        for (legacy, entrance, exit, _) in LEGACY_MAPPING {
            let config = before_motion(kind_id, &with_animation(text(legacy)));

            assert_eq!(
                (
                    str_of(&config, ENTRANCE),
                    str_of(&config, TEXT_EFFECT),
                    str_of(&config, EXIT),
                    config.contains_key(ANIMATION),
                ),
                (Some(entrance), Some(NO_MOTION), Some(exit), false),
                "{kind_id} migrated '{legacy}' to the wrong motion"
            );
        }
    }
}

#[test]
fn an_old_chat_animation_becomes_the_row_entrance_its_stylesheet_played() {
    for (legacy, _, _, row_entrance) in LEGACY_MAPPING {
        let config = before_motion(CHAT_KIND, &with_animation(text(legacy)));

        assert_eq!(
            (
                str_of(&config, ENTRANCE),
                config.contains_key(EXIT),
                config.contains_key(TEXT_EFFECT),
                config.contains_key(ANIMATION),
            ),
            (Some(row_entrance), false, false, false),
            "chat migrated '{legacy}' to the wrong row motion"
        );
    }
}

#[test]
fn a_look_that_never_leaves_migrates_its_old_animation_to_an_entrance_and_gains_no_exit() {
    for (kind_id, has_text_effect) in [(GOAL_KIND, true), (FRAME_KIND, false)] {
        for (legacy, entrance, _, _) in LEGACY_MAPPING {
            let config = before_motion(kind_id, &with_animation(text(legacy)));

            assert_eq!(
                (
                    str_of(&config, ENTRANCE),
                    config.contains_key(TEXT_EFFECT),
                    config.contains_key(EXIT) || config.contains_key(EXIT_MS),
                ),
                (Some(entrance), has_text_effect, false),
                "{kind_id} migrated '{legacy}' to the wrong motion"
            );
        }
    }
}

#[test]
fn a_migrated_look_keeps_the_transition_length_its_old_stylesheet_played() {
    for (kind_id, entrance_ms, exit_ms) in [
        (ALERT_KIND, OLD_REVEAL_MS, Some(OLD_REVEAL_MS)),
        (TICKER_KIND, OLD_REVEAL_MS, Some(OLD_REVEAL_MS)),
        (GOAL_KIND, OLD_REVEAL_MS, None),
        (FRAME_KIND, OLD_REVEAL_MS, None),
        (CHAT_KIND, OLD_ROW_ENTER_MS, None),
    ] {
        let config = before_motion(kind_id, &with_animation(text("fade")));

        assert_eq!(
            (int_of(&config, ENTRANCE_MS), int_of(&config, EXIT_MS)),
            (Some(entrance_ms), exit_ms),
            "{kind_id} did not seed its durations from the transition it used to play"
        );
    }
}

#[test]
fn a_migrated_record_that_never_chose_an_animation_keeps_the_old_timing_under_the_look_default() {
    let config = before_motion(ALERT_KIND, &OverlayConfig::new());

    assert_eq!(
        (
            config.contains_key(ENTRANCE),
            config.contains_key(EXIT),
            int_of(&config, ENTRANCE_MS),
            int_of(&config, EXIT_MS),
        ),
        (false, false, Some(OLD_REVEAL_MS), Some(OLD_REVEAL_MS)),
        "an alert that relied on the default animation was given an explicit motion or new timing"
    );
}

#[test]
fn forge_holds_a_migrated_alert_for_the_old_transition_length_after_its_window() {
    let config = before_motion(ALERT_KIND, &with_animation(text("slide-up")));

    assert_eq!(
        tail_of(ALERT_KIND, &config),
        Duration::from_millis(OLD_REVEAL_MS.unsigned_abs()),
        "the exit tail forge schedules differs from the exit the migrated page plays"
    );
}

#[test]
fn an_old_animation_no_look_ever_offered_is_dropped_and_the_look_default_motion_applies() {
    for stored in [
        text("explode"),
        text(""),
        Variant::Int(3),
        Variant::Bool(true),
    ] {
        let config = before_motion(ALERT_KIND, &with_animation(stored.clone()));

        assert_eq!(
            (
                config.contains_key(ANIMATION),
                config.contains_key(ENTRANCE),
                config.contains_key(EXIT),
                int_of(&config, ENTRANCE_MS),
            ),
            (false, false, false, Some(OLD_REVEAL_MS)),
            "an unreadable old animation {stored:?} was carried over or mapped to a motion"
        );
    }
}

#[test]
fn a_record_that_already_names_its_motion_keeps_it_through_the_upgrade() {
    let stored = OverlayConfig::from([
        (ANIMATION.to_owned(), text("fade")),
        (ENTRANCE.to_owned(), text("pop")),
        (ENTRANCE_MS.to_owned(), Variant::Int(900)),
        (TEXT_EFFECT.to_owned(), text("wave")),
        (EXIT.to_owned(), text("burst")),
        (EXIT_MS.to_owned(), Variant::Int(1_500)),
    ]);

    let config = before_motion(ALERT_KIND, &stored);

    assert_eq!(
        (
            str_of(&config, ENTRANCE),
            int_of(&config, ENTRANCE_MS),
            str_of(&config, TEXT_EFFECT),
            str_of(&config, EXIT),
            int_of(&config, EXIT_MS),
        ),
        (
            Some("pop"),
            Some(900),
            Some("wave"),
            Some("burst"),
            Some(1_500)
        ),
        "the upgrade overwrote motion the record already chose"
    );
}

#[test]
fn every_migrated_record_is_one_its_own_look_accepts() {
    for kind_id in MOVING_KINDS {
        for legacy in ANIMATION_OPTIONS {
            let config = before_motion(kind_id, &with_animation(text(legacy)));

            let refusal = with_kind(kind_id, |descriptor| {
                validate_overlay_config(descriptor, &config).err()
            });

            assert!(
                refusal.is_none(),
                "{kind_id} migrated '{legacy}' into a record it refuses: {refusal:?}"
            );
        }
    }
}

#[test]
fn an_alert_from_before_the_box_contract_adopts_both_the_box_and_its_motion_in_one_upgrade() {
    let config = with_kind(ALERT_KIND, |descriptor| {
        upgrade_config(
            descriptor,
            PRE_BOX_ALERT_SCHEMA,
            &with_animation(text("pop")),
        )
        .expect("an alert from before the box contract is upgraded")
    });

    assert_eq!(
        (
            str_of(&config, ENTRANCE),
            str_of(&config, EXIT),
            config.contains_key(DESIGN_WIDTH),
            config.contains_key(ANIMATION),
        ),
        (Some("pop"), Some("pop"), true, false),
        "a two-step upgrade lost the old animation to the box step or skipped the box"
    );
}

#[test]
fn each_look_offers_exactly_the_motion_axes_it_plays_and_none_offers_the_old_animation() {
    for (kind_id, entrance, text_effect, exit) in [
        (ALERT_KIND, true, true, true),
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
                keys.contains(ANIMATION),
            ),
            (entrance, text_effect, exit, entrance, false),
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
    for kind_id in [GOAL_KIND, CHAT_KIND, FRAME_KIND, BLANK_KIND] {
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
    for kind_id in [GOAL_KIND, CHAT_KIND, FRAME_KIND] {
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
    let mut exits = rect.clone();
    exits.extend(engine_table_keys("DESTRUCTIVE_EXITS"));

    for (field, offered, played) in [
        (ENTRANCE, options(ENTRANCE_OPTIONS), rect),
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

#[test]
fn the_agreed_motion_pacing_holds_three_intensities_and_a_quick_default_stagger() {
    let staggers: Vec<i64> = engine_table("TEXT_EFFECTS")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("staggerMs: "))
        .map(|value| value.trim_end_matches(',').parse().unwrap())
        .collect();

    assert_eq!(
        (
            INTENSITY_OPTIONS.len(),
            staggers.len(),
            staggers.iter().all(|ms| (40..=80).contains(ms)),
            MOTION_SOURCE.contains("var ENTRANCE_SHARE = 0.75;"),
        ),
        (3, options(TEXT_EFFECT_OPTIONS).len(), true, true),
        "the pacing drifted from the agreed three intensities, 40-80 ms per text unit and an \
         entrance capped at three quarters of the window: staggers {staggers:?}"
    );
}

#[test]
fn every_look_but_chat_enters_over_the_agreed_half_second_by_default() {
    for kind_id in [ALERT_KIND, TICKER_KIND, GOAL_KIND, FRAME_KIND] {
        let defaults = with_kind(kind_id, |descriptor| descriptor.default_config());

        assert_eq!(
            int_of(&defaults, ENTRANCE_MS),
            Some(500),
            "{kind_id} does not open on the agreed entrance length"
        );
    }
}
