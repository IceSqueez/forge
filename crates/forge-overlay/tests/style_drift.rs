#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::metrics::{
    ALERT, AlertMetrics, CHAT, ChatMetrics, FRAME, FrameMetrics, GOAL, GoalMetrics, TICKER,
    TickerMetrics,
};
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds, style_guards};

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
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
fn every_kind_this_build_registers_is_guarded_against_style_drift() {
    let reg = registry();
    let guarded: BTreeSet<&str> = style_guards().iter().map(|guard| guard.kind_id).collect();
    let registered: BTreeSet<&str> = reg.all().map(|descriptor| descriptor.id()).collect();

    assert_eq!(
        guarded, registered,
        "a kind with no guard can drift away from the size its preview draws"
    );
}
