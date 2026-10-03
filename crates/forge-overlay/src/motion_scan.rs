use crate::descriptor::OverlayKindDescriptor;
use crate::page_contract::BIND_ATTRIBUTE;

const ID_ATTRIBUTE_NAME: &str = "id";
const ATTRIBUTE_ASSIGN: char = '=';
const REVEAL_CALL: &str = "forge.show(";
const RETIRED_ANIMATION_ATTRIBUTE: &str = "data-animation";
const ATTRIBUTE_QUOTES: [char; 2] = ['"', '\''];
const VALUE_TERMINATORS: [char; 4] = ['"', '\'', '>', '/'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionIssue {
    RevealTargetMissing,
    TextTargetsMissing,
    RevealCallMissing,
    RetiredAnimationRules,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverriddenSources<'a> {
    pub markup: Option<&'a str>,
    pub style: Option<&'a str>,
    pub behavior: Option<&'a str>,
}

pub fn motion_issues(
    descriptor: &dyn OverlayKindDescriptor,
    sources: OverriddenSources<'_>,
) -> Vec<MotionIssue> {
    let profile = descriptor.motion();
    if !profile.axes.any() {
        return Vec::new();
    }
    let mut issues = Vec::new();
    if let Some(markup) = sources.markup {
        if !declares_id(markup, profile.reveal_target_id) {
            issues.push(MotionIssue::RevealTargetMissing);
        }
        if profile.axes.text_effect && !markup.contains(BIND_ATTRIBUTE) {
            issues.push(MotionIssue::TextTargetsMissing);
        }
    }
    if sources
        .behavior
        .is_some_and(|behavior| !behavior.contains(REVEAL_CALL))
    {
        issues.push(MotionIssue::RevealCallMissing);
    }
    if sources
        .style
        .is_some_and(|style| style.contains(RETIRED_ANIMATION_ATTRIBUTE))
    {
        issues.push(MotionIssue::RetiredAnimationRules);
    }
    issues
}

fn declares_id(markup: &str, id: &str) -> bool {
    let lowered = markup.to_ascii_lowercase();
    lowered.match_indices(ID_ATTRIBUTE_NAME).any(|(at, _)| {
        let stands_alone = markup[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        stands_alone
            && assigned_value(&markup[at + ID_ATTRIBUTE_NAME.len()..])
                .is_some_and(|value| names_id(value, id))
    })
}

fn assigned_value(after_name: &str) -> Option<&str> {
    let value = after_name
        .trim_start()
        .strip_prefix(ATTRIBUTE_ASSIGN)?
        .trim_start();
    Some(value.strip_prefix(ATTRIBUTE_QUOTES).unwrap_or(value))
}

fn names_id(value: &str, id: &str) -> bool {
    value.strip_prefix(id).is_some_and(|rest| {
        rest.is_empty()
            || rest.starts_with(|c: char| c.is_whitespace() || VALUE_TERMINATORS.contains(&c))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::alert::AlertOverlayKind;
    use crate::kinds::blank::BlankOverlayKind;
    use crate::kinds::goal::GoalOverlayKind;

    #[test]
    fn id_attribute_is_recognised_in_every_legal_spelling_and_never_in_lookalikes() {
        for (markup, declared) in [
            (r#"<div id="stage">"#, true),
            (r#"<div ID="stage">"#, true),
            (r#"<div Id='stage'>"#, true),
            (r#"<div id = "stage">"#, true),
            ("<div id\n=\n'stage'>", true),
            (r#"<div class="a" id="stage" >"#, true),
            ("<div\n\tid=\"stage\">", true),
            ("<div id=stage>", true),
            (r#"<div id="stage"/>"#, true),
            (r#"<div data-id="stage">"#, false),
            (r#"<div sid="stage">"#, false),
            (r#"<div id="stages">"#, false),
            (r#"<div id="stage-2">"#, false),
            (r#"<div id="main">"#, false),
            (r#"<div class="id stage">"#, false),
            (r#"<div id:"stage">"#, false),
            ("", false),
        ] {
            assert_eq!(declares_id(markup, "stage"), declared, "{markup:?}");
        }
    }

    #[test]
    fn a_later_declaration_counts_after_an_earlier_lookalike() {
        assert!(declares_id(
            r#"<i data-id="stage"></i><div id="stage"></div>"#,
            "stage"
        ));
    }

    #[test]
    fn missing_reveal_target_is_reported_for_markup_without_the_stage_id() {
        let issues = motion_issues(
            &AlertOverlayKind,
            OverriddenSources {
                markup: Some(r#"<div data-bind="headline" id="main"></div>"#),
                ..Default::default()
            },
        );

        assert_eq!(issues, vec![MotionIssue::RevealTargetMissing]);
    }

    #[test]
    fn text_effect_without_any_bound_element_is_reported_only_for_kinds_with_the_text_axis() {
        let markup = Some(r#"<div id="stage"></div>"#);
        let sources = OverriddenSources {
            markup,
            ..Default::default()
        };

        assert_eq!(
            motion_issues(&GoalOverlayKind, sources),
            vec![MotionIssue::TextTargetsMissing]
        );
        let profile_without_text = motion_issues(&crate::kinds::frame::FrameOverlayKind, sources);
        assert!(profile_without_text.is_empty());
    }

    #[test]
    fn behavior_without_reveal_call_and_style_with_retired_rules_are_each_reported() {
        let issues = motion_issues(
            &AlertOverlayKind,
            OverriddenSources {
                markup: None,
                behavior: Some("render();"),
                style: Some("#stage { data-animation: x }"),
            },
        );

        assert_eq!(
            issues,
            vec![
                MotionIssue::RevealCallMissing,
                MotionIssue::RetiredAnimationRules
            ]
        );
    }

    #[test]
    fn compliant_overrides_and_non_overridden_files_report_nothing() {
        let clean = OverriddenSources {
            markup: Some(r#"<div id="stage" data-bind="x"></div>"#),
            behavior: Some("forge.show(render);"),
            style: Some("#stage {}"),
        };

        assert!(motion_issues(&AlertOverlayKind, clean).is_empty());
        assert!(motion_issues(&AlertOverlayKind, OverriddenSources::default()).is_empty());
    }

    #[test]
    fn a_kind_without_motion_never_reports_issues() {
        let issues = motion_issues(
            &BlankOverlayKind,
            OverriddenSources {
                markup: Some(""),
                behavior: Some(""),
                style: Some("data-animation"),
            },
        );

        assert!(issues.is_empty());
    }
}
