#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::metrics::{
    ALERT, AlertMetrics, CHAT, ChatMetrics, FRAME, FrameMetrics, GOAL, GoalMetrics,
    TEXT_SCALE_PROPERTY, TICKER, TickerMetrics,
};
use forge_overlay::{OverlayKindRegistry, element_sizing, register_builtin_kinds, style_guards};

const SCALED_NUMBERS: &[(&str, &[f32])] = &[
    ("overlay.alert", &[12.0, 13.0, 14.0, 16.0, 20.0, 22.0, 26.0]),
    (
        "overlay.chat",
        &[6.0, 6.0, 6.0, 8.0, 10.0, 11.0, 13.0, 13.0],
    ),
    ("overlay.frame", &[-13.0, -13.0, 3.0, 8.0, 11.0, 13.0, 14.0]),
    (
        "overlay.goal",
        &[4.0, 8.0, 10.0, 12.0, 12.0, 14.0, 14.0, 18.0],
    ),
    (
        "overlay.ticker",
        &[3.0, 6.0, 10.0, 12.0, 13.0, 14.0, 19.0, 22.0, 26.0],
    ),
];

const BREAKING_TEXT: &[(&str, &[&str])] = &[
    ("overlay.alert", &[".body"]),
    ("overlay.chat", &[".author", ".message"]),
    ("overlay.goal", &[".label", ".figure"]),
];

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn scaled_multiplicands(style: &str) -> Vec<f32> {
    let opening = "calc(";
    let suffix = format!("px * var({TEXT_SCALE_PROPERTY}))");
    let mut found: Vec<f32> = style
        .match_indices(opening)
        .filter_map(|(at, _)| {
            let rest = &style[at + opening.len()..];
            let end = rest.find(&suffix)?;
            rest[..end].parse::<f32>().ok()
        })
        .collect();
    found.sort_by(f32::total_cmp);
    found
}

fn rule_block(style: &str, selector: &str) -> String {
    let opening = format!("\n{selector} {{");
    let start = style
        .find(&opening)
        .unwrap_or_else(|| panic!("the stylesheet has no '{selector}' rule"))
        + opening.len();
    let rest = &style[start..];
    let end = rest
        .find('}')
        .unwrap_or_else(|| panic!("the '{selector}' rule is never closed"));
    rest[..end].to_owned()
}

fn stylesheet(kind_id: &str) -> &'static str {
    registry()
        .get(kind_id)
        .unwrap_or_else(|| panic!("{kind_id} has a style guard but no registered kind"))
        .page_assets()
        .style
}

#[test]
fn every_metric_a_guard_names_is_written_verbatim_in_its_kinds_stylesheet() {
    for guard in style_guards() {
        let style = stylesheet(guard.kind_id);

        assert!(
            !guard.declarations.is_empty(),
            "{} guards nothing, so its drawn preview can drift freely",
            guard.kind_id
        );
        for declaration in &guard.declarations {
            assert!(
                style.contains(declaration.as_str()),
                "{} draws '{declaration}' but its stylesheet never states it",
                guard.kind_id
            );
        }
    }
}

#[test]
fn a_metric_that_drifts_from_its_stylesheet_fails_its_guard() {
    for (kind_id, drifted) in [
        (
            "overlay.alert",
            AlertMetrics {
                gap: ALERT.gap + 0.5,
                ..ALERT
            }
            .declarations(),
        ),
        (
            "overlay.chat",
            ChatMetrics {
                row_radius: CHAT.row_radius + 0.5,
                ..CHAT
            }
            .declarations(),
        ),
        (
            "overlay.frame",
            FrameMetrics {
                inset: FRAME.inset + 0.5,
                ..FRAME
            }
            .declarations(),
        ),
        (
            "overlay.goal",
            GoalMetrics {
                track_height: GOAL.track_height + 0.5,
                ..GOAL
            }
            .declarations(),
        ),
        (
            "overlay.ticker",
            TickerMetrics {
                mark_width: TICKER.mark_width + 0.5,
                ..TICKER
            }
            .declarations(),
        ),
    ] {
        let style = stylesheet(kind_id);

        assert!(
            drifted
                .iter()
                .any(|declaration| !style.contains(declaration.as_str())),
            "{kind_id} accepts a metric its stylesheet does not state, so the guard proves nothing"
        );
    }
}

#[test]
fn every_number_a_page_scales_is_the_number_it_drew_before_a_text_size_could_be_chosen() {
    for (kind_id, expected) in SCALED_NUMBERS {
        assert_eq!(
            scaled_multiplicands(stylesheet(kind_id)),
            *expected,
            "{kind_id} scales a number it did not draw at, so an untouched page no longer renders \
             the way it did"
        );
    }
}

#[test]
fn a_kind_whose_element_takes_a_width_lets_an_unbreakable_word_break_inside_it() {
    let reg = registry();
    let widthful: BTreeSet<&str> = reg
        .all()
        .filter(|descriptor| {
            element_sizing(descriptor.id()).is_some_and(|sizing| sizing.width.is_some())
        })
        .map(|descriptor| descriptor.id())
        .collect();
    let breaking: BTreeSet<&str> = BREAKING_TEXT.iter().map(|(kind_id, _)| *kind_id).collect();

    assert_eq!(
        widthful, breaking,
        "a kind whose card the user can narrow has no rule keeping a long word inside it"
    );

    for (kind_id, selectors) in BREAKING_TEXT {
        let style = stylesheet(kind_id);

        for selector in *selectors {
            let block = rule_block(style, selector);

            for declaration in ["min-width: 0;", "overflow-wrap: anywhere;"] {
                assert!(
                    block.contains(declaration),
                    "{kind_id} {selector} lacks '{declaration}', so a long word pushes the card \
                     wider than the width the user chose"
                );
            }
            assert!(
                !block.contains("white-space: nowrap;"),
                "{kind_id} {selector} refuses to wrap, so nothing the break rules say can apply"
            );
        }
    }
}

#[test]
fn a_kind_is_guarded_against_style_drift_exactly_when_it_draws_a_page() {
    let reg = registry();
    let guarded: BTreeSet<&str> = style_guards().iter().map(|guard| guard.kind_id).collect();
    let drawing: BTreeSet<&str> = reg
        .all()
        .filter(|descriptor| descriptor.has_visual_page())
        .map(|descriptor| descriptor.id())
        .collect();

    assert_eq!(
        guarded, drawing,
        "a drawing kind with no guard drifts away from the size its preview draws, and a guard on \
         a kind that draws nothing pins geometry no page has"
    );
}
