use crate::descriptor::OverlayKindDescriptor;

const ID_ATTRIBUTE_NAME: &str = "id";
const ATTRIBUTE_ASSIGN: char = '=';
const BIND_ATTRIBUTE: &str = "data-bind";
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
