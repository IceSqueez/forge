#![expect(clippy::panic, reason = "test fixtures assert directly")]
mod common;
use common::{
    ALERT_KIND, CARD_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, LATEST_KIND, TICKER_KIND, registry,
};

use std::collections::BTreeSet;

use forge_overlay::PreviewAccent;
use forge_overlay::config::{BORDER_COLOR, MAIN_TEXT_COLOR, TEXT_COLOR};
use forge_types::Variant;

use forge_overlay::metrics::{
    ALERT, AlertMetrics, BORDER_COLOR_PROPERTY, CHAT, ChatMetrics, FRAME, FrameMetrics, GOAL,
    GoalMetrics, LATEST, LatestMetrics, MAIN_TEXT_COLOR_PROPERTY, TEXT_COLOR_PROPERTY,
    TEXT_SCALE_PROPERTY, TICKER, TickerMetrics,
};

const SCALED_NUMBERS: &[(&str, &[f32])] = &[
    (ALERT_KIND, &[12.0, 13.0, 14.0, 16.0, 20.0, 22.0, 26.0]),
    (CARD_KIND, &[12.0, 13.0, 14.0, 16.0, 20.0, 22.0, 26.0]),
    (
        CHAT_KIND,
        &[
            1.0, 3.0, 4.0, 6.0, 6.0, 6.0, 6.0, 8.0, 10.0, 11.0, 11.0, 13.0, 13.0, 18.0, 18.0, 18.0,
            24.0,
        ],
    ),
    (
        FRAME_KIND,
        &[-13.0, -13.0, 3.0, 8.0, 11.0, 13.0, 14.0, 16.0],
    ),
    (GOAL_KIND, &[4.0, 8.0, 10.0, 12.0, 12.0, 14.0, 14.0, 18.0]),
    (
        LATEST_KIND,
        &[
            4.0, 10.0, 12.0, 13.0, 14.0, 14.0, 15.0, 20.0, 20.0, 26.0, 26.0,
        ],
    ),
    (
        TICKER_KIND,
        &[3.0, 6.0, 10.0, 12.0, 13.0, 14.0, 19.0, 22.0, 26.0],
    ),
];

const BREAKING_TEXT: &[(&str, &[&str])] = &[
    (ALERT_KIND, &[".body"]),
    (CARD_KIND, &[".body"]),
    (CHAT_KIND, &[".author", ".message"]),
    (GOAL_KIND, &[".label", ".figure"]),
];

const ELLIPSIZING_TEXT: &[(&str, &str)] = &[(LATEST_KIND, ".value"), (TICKER_KIND, ".body")];

const MAIN_INK: &[(&str, &str, &str)] = &[
    (ALERT_KIND, ".headline", "color"),
    (CHAT_KIND, ".message", "color"),
    (FRAME_KIND, ".headline", "color"),
    (GOAL_KIND, ".label", "color"),
    (LATEST_KIND, ".headline", "color"),
    (TICKER_KIND, ".headline", "color"),
];

const TEXT_INK: &[(&str, &str, &str)] = &[
    (ALERT_KIND, ".subline", "color"),
    (CHAT_KIND, ".badges", "color"),
    (FRAME_KIND, ".subline", "color"),
    (GOAL_KIND, ".figure", "color"),
    (LATEST_KIND, ".label", "color"),
    (TICKER_KIND, ".subline", "color"),
];

const BORDER_INK: &[(&str, &str, &str)] = &[
    (ALERT_KIND, "#stage", "border"),
    (ALERT_KIND, ".icon.tinted", "background-color"),
    (CHAT_KIND, ".event", "background"),
    (CHAT_KIND, ".row.event-row", "box-shadow"),
    (FRAME_KIND, "#stage", "border"),
    (FRAME_KIND, ".corner", "border"),
    (GOAL_KIND, "#stage", "border"),
    (GOAL_KIND, ".fill", "background"),
    (LATEST_KIND, "#stage", "border"),
    (LATEST_KIND, ".icon", "color"),
    (TICKER_KIND, "#stage", "border-top"),
    (TICKER_KIND, "#stage", "border-bottom"),
    (TICKER_KIND, ".mark", "background"),
];

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

fn standalone_rule(style: &str, selector: &str) -> String {
    let opening = format!("{selector} {{");
    let start = style
        .match_indices(&opening)
        .map(|(at, _)| at)
        .find(|&at| at == 0 || style[..at].ends_with("\n\n"))
        .unwrap_or_else(|| panic!("the stylesheet has no standalone '{selector}' rule"))
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
    for guard in registry().style_guards() {
        let style = stylesheet(&guard.kind_id);

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
            ALERT_KIND,
            AlertMetrics {
                gap: ALERT.gap + 0.5,
                ..ALERT
            }
            .declarations(),
        ),
        (
            CHAT_KIND,
            ChatMetrics {
                row_radius: CHAT.row_radius + 0.5,
                ..CHAT
            }
            .declarations(),
        ),
        (
            FRAME_KIND,
            FrameMetrics {
                inset: FRAME.inset + 0.5,
                ..FRAME
            }
            .declarations(),
        ),
        (
            GOAL_KIND,
            GoalMetrics {
                track_height: GOAL.track_height + 0.5,
                ..GOAL
            }
            .declarations(),
        ),
        (
            LATEST_KIND,
            LatestMetrics {
                subline_margin: LATEST.subline_margin + 0.5,
                ..LATEST
            }
            .declarations(),
        ),
        (
            TICKER_KIND,
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
fn the_icon_box_spends_the_guarded_height_on_the_icon_itself() {
    let style = stylesheet(ALERT_KIND);
    let icon_height = ALERT
        .declarations()
        .into_iter()
        .find(|declaration| declaration.starts_with("height:"))
        .expect("the alert guard states the icon box height");

    let box_rule = rule_block(style, ".icon");
    assert!(
        box_rule.contains(&icon_height),
        "the icon box does not state '{icon_height}', so a chosen text size leaves it behind"
    );
    assert!(
        box_rule.contains("aspect-ratio: 1;"),
        "the icon box is free to stretch, so a wide glyph is drawn distorted"
    );
}

fn declaration_value(style: &str, selector: &str, property: &str) -> String {
    let block = rule_block(style, selector);
    let opening = format!("\n  {property}: ");
    let start = block
        .find(&opening)
        .unwrap_or_else(|| panic!("'{selector}' declares no '{property}'"))
        + opening.len();
    let rest = &block[start..];
    let end = rest
        .find(';')
        .unwrap_or_else(|| panic!("'{selector} {{ {property}' is never terminated"));
    rest[..end].to_owned()
}

fn assert_painted_with(table: &[(&str, &str, &str)], ink: &str, other: &str) {
    for (kind_id, selector, property) in table {
        let value = declaration_value(stylesheet(kind_id), selector, property);
        assert!(
            value.contains(&format!("var({ink})")) && !value.contains(&format!("var({other})")),
            "{kind_id} {selector} {property} is '{value}', not painted with {ink}"
        );
    }
}

#[test]
fn each_kind_paints_its_main_text_with_the_main_text_color() {
    assert_painted_with(MAIN_INK, MAIN_TEXT_COLOR_PROPERTY, TEXT_COLOR_PROPERTY);
}

#[test]
fn each_kind_paints_its_text_with_the_text_color() {
    assert_painted_with(TEXT_INK, TEXT_COLOR_PROPERTY, BORDER_COLOR_PROPERTY);
}

#[test]
fn each_kind_paints_its_edges_and_marks_with_the_border_color() {
    assert_painted_with(BORDER_INK, BORDER_COLOR_PROPERTY, TEXT_COLOR_PROPERTY);
}

#[test]
fn each_kind_page_starts_every_color_at_the_kinds_default_before_its_config_arrives() {
    let reg = registry();
    for kind_id in [
        ALERT_KIND,
        CARD_KIND,
        CHAT_KIND,
        FRAME_KIND,
        GOAL_KIND,
        LATEST_KIND,
        TICKER_KIND,
    ] {
        let descriptor = reg
            .get(kind_id)
            .unwrap_or_else(|| panic!("{kind_id} is registered"));
        let defaults = descriptor.default_config();
        let root = format!("\n{}", stylesheet(kind_id));

        for (key, property) in [
            (MAIN_TEXT_COLOR, MAIN_TEXT_COLOR_PROPERTY),
            (TEXT_COLOR, TEXT_COLOR_PROPERTY),
            (BORDER_COLOR, BORDER_COLOR_PROPERTY),
        ] {
            let stored = defaults
                .get(key)
                .and_then(Variant::as_str)
                .unwrap_or_default();
            let [red, green, blue] = PreviewAccent::from_stored(stored).channels();
            assert_eq!(
                declaration_value(&root, ":root", property),
                format!("#{red:02x}{green:02x}{blue:02x}"),
                "{kind_id} paints {property} before its config arrives with something other \
                 than its default {key}"
            );
        }
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
fn every_kind_whose_element_is_bound_to_the_box_width_keeps_its_text_inside_that_width() {
    let reg = registry();
    let widthful: BTreeSet<&str> = reg
        .all()
        .filter(|descriptor| {
            descriptor
                .sizing()
                .is_some_and(|sizing| sizing.width.is_some())
        })
        .map(|descriptor| descriptor.id())
        .collect();
    let contained: BTreeSet<&str> = BREAKING_TEXT
        .iter()
        .map(|(kind_id, _)| *kind_id)
        .chain(ELLIPSIZING_TEXT.iter().map(|(kind_id, _)| *kind_id))
        .collect();

    assert_eq!(
        widthful, contained,
        "a kind bound to the box width has no rule keeping a long word inside the box"
    );
}

#[test]
fn wrapping_text_lets_an_unbreakable_word_break_inside_the_box() {
    for (kind_id, selectors) in BREAKING_TEXT {
        let style = stylesheet(kind_id);

        for selector in *selectors {
            let block = rule_block(style, selector);

            for declaration in ["min-width: 0;", "overflow-wrap: anywhere;"] {
                assert!(
                    block.contains(declaration),
                    "{kind_id} {selector} lacks '{declaration}', so a long word pushes the card \
                     past the edge of the box"
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
fn single_line_text_ellipsizes_at_the_box_edge_instead_of_spilling() {
    for (kind_id, selector) in ELLIPSIZING_TEXT {
        let block = rule_block(stylesheet(kind_id), selector);

        for declaration in [
            "min-width: 0;",
            "overflow: hidden;",
            "text-overflow: ellipsis;",
        ] {
            assert!(
                block.contains(declaration),
                "{kind_id} {selector} lacks '{declaration}', so a long line runs past the box"
            );
        }
    }
}

#[test]
fn a_kind_is_guarded_against_style_drift_exactly_when_it_draws_a_page() {
    let reg = registry();
    let guards = reg.style_guards();
    let guarded: BTreeSet<&str> = guards.iter().map(|guard| guard.kind_id.as_str()).collect();
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

#[test]
fn every_stock_look_clips_at_the_box_and_keeps_its_margins_inside_it() {
    for descriptor in registry().all() {
        let kind_id = descriptor.id();
        let style = stylesheet(kind_id);
        let root = standalone_rule(style, "html,\nbody");

        for declaration in ["width: 100%;", "height: 100%;", "overflow: hidden;"] {
            assert!(
                root.contains(declaration),
                "{kind_id} root lacks '{declaration}', so a look can paint past the source box"
            );
        }
        assert!(
            standalone_rule(style, "body").contains("box-sizing: border-box;"),
            "{kind_id} adds its margins on top of the box instead of inside it"
        );
    }
}

#[test]
fn only_the_looks_that_keep_position_read_it_from_the_page() {
    for (kind_id, reads_position) in [
        (ALERT_KIND, false),
        (GOAL_KIND, false),
        (LATEST_KIND, false),
        (TICKER_KIND, false),
        (CHAT_KIND, true),
        (FRAME_KIND, true),
    ] {
        assert_eq!(
            stylesheet(kind_id).contains("data-position"),
            reads_position,
            "{kind_id} disagrees with the box contract about placing itself by position"
        );
    }
}

#[test]
fn every_ink_the_preview_paints_is_the_ink_its_page_rule_states() {
    let bright = "color: #fff;".to_owned();
    let main = format!("color: var({MAIN_TEXT_COLOR_PROPERTY});");
    let fixed_inks = [
        (CHAT_KIND, ".author", bright),
        (
            GOAL_KIND,
            ".track",
            "background: rgba(255, 255, 255, 0.14);".to_owned(),
        ),
        (
            GOAL_KIND,
            ".sep",
            "color: rgba(255, 255, 255, 0.5);".to_owned(),
        ),
        (
            LATEST_KIND,
            ".subline",
            "color: rgba(255, 255, 255, 0.7);".to_owned(),
        ),
    ];
    let main_inks = MAIN_INK
        .iter()
        .map(|(kind_id, selector, _)| (*kind_id, *selector, main.clone()));

    for (kind_id, selector, declaration) in fixed_inks.into_iter().chain(main_inks) {
        assert!(
            rule_block(stylesheet(kind_id), selector).contains(&declaration),
            "{kind_id} {selector} no longer states '{declaration}'"
        );
        let guards = registry().style_guards();
        assert!(
            guards
                .iter()
                .any(|guard| guard.kind_id == kind_id && guard.declarations.contains(&declaration)),
            "{kind_id} does not guard '{declaration}', so its preview ink can drift"
        );
    }
}
