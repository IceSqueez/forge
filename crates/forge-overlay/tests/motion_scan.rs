#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::{
    MotionIssue, OverlayKindDescriptor, OverlayKindRegistry, OverriddenSources, motion_issues,
    register_builtin_kinds,
};

const ALERT_KIND: &str = "overlay.alert";
const BLANK_KIND: &str = "overlay.blank";
const CHAT_KIND: &str = "overlay.chat";
const FRAME_KIND: &str = "overlay.frame";
const GOAL_KIND: &str = "overlay.goal";
const TICKER_KIND: &str = "overlay.ticker";

const BOUND_TEXT: &str = r#"<span data-bind="headline"></span>"#;

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn issues_of(kind_id: &str, sources: OverriddenSources<'_>) -> Vec<MotionIssue> {
    let reg = registry();
    let descriptor: &dyn OverlayKindDescriptor =
        reg.get(kind_id).expect("a registered builtin kind");
    motion_issues(descriptor, sources)
}

fn markup(markup: &str) -> OverriddenSources<'_> {
    OverriddenSources {
        markup: Some(markup),
        ..OverriddenSources::default()
    }
}

#[test]
fn every_stock_look_overridden_word_for_word_scans_clean() {
    let reg = registry();
    for descriptor in reg.all() {
        let assets = descriptor.page_assets();
        let sources = OverriddenSources {
            markup: Some(assets.markup),
            style: Some(assets.style),
            behavior: Some(assets.behavior),
        };

        assert_eq!(
            motion_issues(descriptor, sources),
            Vec::new(),
            "{} warns about motion on its own shipped files",
            descriptor.id()
        );
    }
}

#[test]
fn nothing_overridden_raises_no_motion_issue() {
    for kind_id in [ALERT_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, TICKER_KIND] {
        assert_eq!(
            issues_of(kind_id, OverriddenSources::default()),
            Vec::new(),
            "{kind_id} warns with no file overridden"
        );
    }
}

#[test]
fn overridden_markup_that_declares_the_reveal_target_in_any_attribute_form_is_accepted() {
    for declared in [
        r#"<div id="stage">"#,
        "<div id='stage'>",
        "<div id=stage>",
        "<div id=stage/>",
        r#"<div class="card" id="stage" hidden>"#,
        "<div\n  id=\"stage\">",
        "<div\tid=\"stage\">",
    ] {
        let page = format!("{declared}{BOUND_TEXT}</div>");

        assert_eq!(
            issues_of(ALERT_KIND, markup(&page)),
            Vec::new(),
            "{declared:?} declares the reveal target but was reported"
        );
    }
}

#[test]
fn overridden_markup_without_the_reveal_target_is_reported() {
    for missing in [
        r#"<div class="stage">"#,
        r#"<div data-id="stage">"#,
        r#"<div id="stage-old">"#,
        r#"<div id="backstage">"#,
        r#"<div id="">"#,
        "<div>stage</div><div>",
    ] {
        let page = format!("{missing}{BOUND_TEXT}</div>");

        assert_eq!(
            issues_of(ALERT_KIND, markup(&page)),
            vec![MotionIssue::RevealTargetMissing],
            "{missing:?} has no element the engine can reveal"
        );
    }
}

#[test]
fn chat_markup_is_checked_for_its_row_template_rather_than_the_stage() {
    for (page, expected) in [
        (
            r#"<div id="stage"><template id="row-template"></template></div>"#,
            Vec::new(),
        ),
        (
            r#"<div id="stage"><template id="row"></template></div>"#,
            vec![MotionIssue::RevealTargetMissing],
        ),
    ] {
        assert_eq!(
            issues_of(CHAT_KIND, markup(page)),
            expected,
            "chat scanned {page:?} against the wrong reveal target"
        );
    }
}

#[test]
fn markup_without_bound_text_is_reported_only_for_looks_that_play_a_text_effect() {
    let unbound = r#"<div id="stage"><span class="headline"></span></div>"#;
    for (kind_id, expected) in [
        (ALERT_KIND, vec![MotionIssue::TextTargetsMissing]),
        (TICKER_KIND, vec![MotionIssue::TextTargetsMissing]),
        (GOAL_KIND, vec![MotionIssue::TextTargetsMissing]),
        (FRAME_KIND, Vec::new()),
    ] {
        assert_eq!(
            issues_of(kind_id, markup(unbound)),
            expected,
            "{kind_id} reported text targets wrongly for markup without bound text"
        );
    }
}

#[test]
fn a_behaviour_script_that_never_reveals_through_the_runtime_is_reported() {
    for (behavior, expected) in [
        (
            "document.querySelector('#stage').classList.remove('hidden');",
            vec![MotionIssue::RevealCallMissing],
        ),
        (
            "forge.content(function () { forge.show(\"#stage\", 5000); });",
            Vec::new(),
        ),
    ] {
        let sources = OverriddenSources {
            behavior: Some(behavior),
            ..OverriddenSources::default()
        };

        assert_eq!(
            issues_of(ALERT_KIND, sources),
            expected,
            "behaviour {behavior:?} scanned wrongly"
        );
    }
}

#[test]
fn a_stylesheet_still_keyed_on_the_retired_animation_attribute_is_reported() {
    for (style, expected) in [
        (
            r#"body[data-animation="pop"] #stage.hidden { transform: scale(0.9); }"#,
            vec![MotionIssue::RetiredAnimationRules],
        ),
        ("#stage.hidden { opacity: 0; }", Vec::new()),
    ] {
        let sources = OverriddenSources {
            style: Some(style),
            ..OverriddenSources::default()
        };

        assert_eq!(
            issues_of(TICKER_KIND, sources),
            expected,
            "stylesheet {style:?} scanned wrongly"
        );
    }
}

#[test]
fn every_problem_in_one_override_set_is_reported_together() {
    let sources = OverriddenSources {
        markup: Some("<div><p>static</p></div>"),
        style: Some("body[data-animation] {}"),
        behavior: Some("show();"),
    };

    assert_eq!(
        issues_of(ALERT_KIND, sources),
        vec![
            MotionIssue::RevealTargetMissing,
            MotionIssue::TextTargetsMissing,
            MotionIssue::RevealCallMissing,
            MotionIssue::RetiredAnimationRules,
        ],
    );
}

#[test]
fn a_look_without_motion_never_warns_about_motion() {
    let sources = OverriddenSources {
        markup: Some("<p>no targets</p>"),
        style: Some("body[data-animation] {}"),
        behavior: Some("noop();"),
    };

    assert_eq!(issues_of(BLANK_KIND, sources), Vec::new());
}
