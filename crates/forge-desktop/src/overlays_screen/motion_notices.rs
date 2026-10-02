use std::sync::Arc;

use forge_components::tr;
use forge_overlay::motion::{ENTRANCE, EXIT, TEXT_EFFECT};
use forge_overlay::{
    BEHAVIOR_FILE, MARKUP_FILE, MotionIssue, OverlayKindRegistry, OverriddenSources, STYLE_FILE,
    motion_issues,
};
use forge_runtime::OverlayServiceHandle;
use forge_storage::{OverlayDefinition, OverlayId};
use gpui::Context;

use super::OverlaysView;
use super::property_panel::OverlayPropertyPanel;
use crate::async_bridge;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MotionNotices {
    pub(super) issues: Vec<MotionIssue>,
    pub(super) reveal_target: String,
}

impl MotionNotices {
    pub(super) fn notes_for(&self, key: &str) -> Vec<String> {
        self.issues
            .iter()
            .filter(|issue| affected_keys(**issue).contains(&key))
            .map(|issue| self.message(*issue))
            .collect()
    }

    fn message(&self, issue: MotionIssue) -> String {
        match issue {
            MotionIssue::RevealTargetMissing => tr!(
                "overlays_motion_reveal_target_missing",
                file = MARKUP_FILE,
                target = self.reveal_target.as_str()
            ),
            MotionIssue::TextTargetsMissing => {
                tr!("overlays_motion_text_targets_missing", file = MARKUP_FILE)
            }
            MotionIssue::RevealCallMissing => {
                tr!("overlays_motion_reveal_call_missing", file = BEHAVIOR_FILE)
            }
            MotionIssue::RetiredAnimationRules => {
                tr!("overlays_motion_retired_rules", file = STYLE_FILE)
            }
        }
    }
}

fn affected_keys(issue: MotionIssue) -> &'static [&'static str] {
    match issue {
        MotionIssue::RevealTargetMissing | MotionIssue::RevealCallMissing => {
            &[ENTRANCE, TEXT_EFFECT, EXIT]
        }
        MotionIssue::TextTargetsMissing => &[TEXT_EFFECT],
        MotionIssue::RetiredAnimationRules => &[ENTRANCE, EXIT],
    }
}

async fn read_override(
    service: &OverlayServiceHandle,
    id: &OverlayId,
    overrides: &[String],
    file: &str,
) -> Option<String> {
    if !overrides.iter().any(|held| held == file) {
        return None;
    }
    match service.read_source(id, file).await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(overlay = %id, file, %error, "overridden overlay file unreadable for the motion check");
            None
        }
    }
}

async fn scan_overrides(
    kinds: Arc<OverlayKindRegistry>,
    service: OverlayServiceHandle,
    definition: OverlayDefinition,
) -> MotionNotices {
    let Some(descriptor) = kinds.get(&definition.kind_id) else {
        return MotionNotices::default();
    };
    let overrides = &definition.source_overrides;
    let id = &definition.id;
    let markup = read_override(&service, id, overrides, MARKUP_FILE).await;
    let style = read_override(&service, id, overrides, STYLE_FILE).await;
    let behavior = read_override(&service, id, overrides, BEHAVIOR_FILE).await;
    let issues = motion_issues(
        descriptor,
        OverriddenSources {
            markup: markup.as_deref(),
            style: style.as_deref(),
            behavior: behavior.as_deref(),
        },
    );
    MotionNotices {
        issues,
        reveal_target: descriptor.motion().reveal_target_id.to_owned(),
    }
}

impl OverlaysView {
    pub(super) fn probe_motion_notices(
        &mut self,
        definition: &OverlayDefinition,
        cx: &mut Context<Self>,
    ) {
        let ticket = self.motion_probe_gen.next();
        if definition.source_overrides.is_empty() {
            self.apply_motion_notices(ticket, MotionNotices::default(), cx);
            return;
        }
        async_bridge::run_async(
            &self.rt_handle,
            scan_overrides(
                Arc::clone(&self.kinds),
                self.service.clone(),
                definition.clone(),
            ),
            move |this, notices, cx| this.apply_motion_notices(ticket, notices, cx),
            cx,
        );
    }

    fn apply_motion_notices(
        &mut self,
        ticket: u64,
        notices: MotionNotices,
        cx: &mut Context<Self>,
    ) {
        if !self.motion_probe_gen.is_current(ticket) {
            return;
        }
        if let Some(panel) = self.panel_view() {
            panel.update(cx, |panel, cx| panel.set_motion_notices(notices, cx));
        }
    }
}

impl OverlayPropertyPanel {
    pub(super) fn set_motion_notices(&mut self, notices: MotionNotices, cx: &mut Context<Self>) {
        if self.motion_notices == notices {
            return;
        }
        self.motion_notices = notices;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVEAL_TARGET: &str = "stage";

    fn notices(issues: &[MotionIssue]) -> MotionNotices {
        MotionNotices {
            issues: issues.to_vec(),
            reveal_target: REVEAL_TARGET.to_owned(),
        }
    }

    fn noted(notices: &MotionNotices) -> Vec<(&'static str, usize)> {
        [ENTRANCE, TEXT_EFFECT, EXIT, "intensity", "duration"]
            .into_iter()
            .map(|key| (key, notices.notes_for(key).len()))
            .collect()
    }

    #[test]
    fn no_issues_put_no_note_on_any_field() {
        assert_eq!(
            noted(&MotionNotices::default()),
            vec![
                (ENTRANCE, 0),
                (TEXT_EFFECT, 0),
                (EXIT, 0),
                ("intensity", 0),
                ("duration", 0)
            ]
        );
    }

    #[test]
    fn each_issue_notes_only_the_preset_fields_it_breaks() {
        for (issue, expected) in [
            (
                MotionIssue::RevealTargetMissing,
                vec![
                    (ENTRANCE, 1),
                    (TEXT_EFFECT, 1),
                    (EXIT, 1),
                    ("intensity", 0),
                    ("duration", 0),
                ],
            ),
            (
                MotionIssue::RevealCallMissing,
                vec![
                    (ENTRANCE, 1),
                    (TEXT_EFFECT, 1),
                    (EXIT, 1),
                    ("intensity", 0),
                    ("duration", 0),
                ],
            ),
            (
                MotionIssue::TextTargetsMissing,
                vec![
                    (ENTRANCE, 0),
                    (TEXT_EFFECT, 1),
                    (EXIT, 0),
                    ("intensity", 0),
                    ("duration", 0),
                ],
            ),
            (
                MotionIssue::RetiredAnimationRules,
                vec![
                    (ENTRANCE, 1),
                    (TEXT_EFFECT, 0),
                    (EXIT, 1),
                    ("intensity", 0),
                    ("duration", 0),
                ],
            ),
        ] {
            assert_eq!(noted(&notices(&[issue])), expected, "{issue:?}");
        }
    }

    #[test]
    fn issues_stack_on_a_field_they_share() {
        let both = notices(&[
            MotionIssue::TextTargetsMissing,
            MotionIssue::RevealCallMissing,
        ]);

        assert_eq!(both.notes_for(TEXT_EFFECT).len(), 2);
        assert_eq!(both.notes_for(ENTRANCE).len(), 1);
    }

    #[test]
    fn missing_target_note_names_the_element_the_user_must_add() {
        crate::i18n::install_language(forge_storage::Language::En);
        let note = notices(&[MotionIssue::RevealTargetMissing])
            .notes_for(ENTRANCE)
            .remove(0);

        assert!(note.contains(REVEAL_TARGET), "{note}");
        assert!(note.contains(MARKUP_FILE), "{note}");
    }
}
