use crate::descriptor::OverlayKindDescriptor;

const ID_ATTRIBUTE: &str = "id=";
const BIND_ATTRIBUTE: &str = "data-bind";
const REVEAL_CALL: &str = "forge.show(";
const RETIRED_ANIMATION_ATTRIBUTE: &str = "data-animation";
const ATTRIBUTE_QUOTES: [char; 2] = ['"', '\''];
const VALUE_TERMINATORS: [char; 5] = ['"', '\'', '>', '/', ' '];

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
    markup.match_indices(ID_ATTRIBUTE).any(|(at, _)| {
        let stands_alone = markup[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let value = markup[at + ID_ATTRIBUTE.len()..].trim_start_matches(ATTRIBUTE_QUOTES);
        stands_alone
            && value
                .strip_prefix(id)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(VALUE_TERMINATORS))
    })
}
