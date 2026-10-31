use forge_awake::{Aspect, AspectState, AwakeStatus};
use forge_components::tr;
use forge_types::MessageRef;

use crate::user_text::resolve_message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AwakeTone {
    Normal,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AwakeIndicator {
    pub tone: AwakeTone,
    pub details: Vec<String>,
}

#[derive(Default)]
pub(crate) struct AwakeState {
    status: Option<AwakeStatus>,
}

impl AwakeState {
    pub(crate) fn apply(&mut self, status: AwakeStatus) -> bool {
        if self.status.as_ref() == Some(&status) {
            return false;
        }
        self.status = Some(status);
        true
    }

    pub(crate) fn enabled(&self) -> Option<bool> {
        self.status.as_ref().map(|status| status.enabled)
    }

    pub(crate) fn indicator(&self) -> Option<AwakeIndicator> {
        let status = self.status.as_ref().filter(|status| status.enabled)?;
        let held: Vec<Aspect> = Aspect::ALL
            .into_iter()
            .filter(|aspect| *status.aspect(*aspect) == AspectState::Held)
            .collect();
        let refused: Vec<(Aspect, &MessageRef)> = Aspect::ALL
            .into_iter()
            .filter_map(|aspect| match status.aspect(aspect) {
                AspectState::Unavailable { reason } => Some((aspect, reason)),
                _ => None,
            })
            .collect();

        let headline = match held.as_slice() {
            [] if refused.is_empty() => tr!("stay_awake_tooltip_starting"),
            [] => tr!("stay_awake_tooltip_inactive"),
            [Aspect::Display] => tr!("stay_awake_tooltip_display_only"),
            [Aspect::System] => tr!("stay_awake_tooltip_system_only"),
            _ => tr!("stay_awake_tooltip_both"),
        };
        let mut details = vec![headline];
        details.extend(refused.iter().map(|(aspect, reason)| {
            let reason = resolve_message(reason).inline();
            match aspect {
                Aspect::Display => tr!("stay_awake_tooltip_display_refused", reason = reason),
                Aspect::System => tr!("stay_awake_tooltip_system_refused", reason = reason),
            }
        }));
        let tone = if refused.is_empty() {
            AwakeTone::Normal
        } else {
            AwakeTone::Warning
        };
        Some(AwakeIndicator { tone, details })
    }
}

#[cfg(test)]
mod tests {
    use forge_types::UPSTREAM_FAILURE;

    use super::*;

    fn status(enabled: bool, display: AspectState, system: AspectState) -> AwakeStatus {
        AwakeStatus {
            enabled,
            display,
            system,
        }
    }

    fn refused() -> AspectState {
        AspectState::Unavailable {
            reason: MessageRef::new(UPSTREAM_FAILURE),
        }
    }

    fn indicator_of(status: AwakeStatus) -> Option<AwakeIndicator> {
        let mut state = AwakeState::default();
        state.apply(status);
        state.indicator()
    }

    #[test]
    fn apply_reports_a_change_only_when_the_status_differs() {
        let mut state = AwakeState::default();
        let held = status(true, AspectState::Held, AspectState::Held);

        assert!(state.apply(held.clone()));
        assert!(!state.apply(held));
        assert!(state.apply(status(false, AspectState::Off, AspectState::Off)));
    }

    #[test]
    fn no_indicator_shows_before_a_status_arrives_or_while_switched_off() {
        assert_eq!(AwakeState::default().indicator(), None);
        assert_eq!(
            indicator_of(status(false, AspectState::Held, AspectState::Held)),
            None
        );
    }

    #[test]
    fn a_refused_aspect_turns_the_indicator_to_a_warning_and_adds_one_detail_per_refusal() {
        for (display, system, tone, detail_count) in [
            (AspectState::Held, AspectState::Held, AwakeTone::Normal, 1),
            (
                AspectState::Pending,
                AspectState::Pending,
                AwakeTone::Normal,
                1,
            ),
            (AspectState::Held, refused(), AwakeTone::Warning, 2),
            (refused(), refused(), AwakeTone::Warning, 3),
        ] {
            let indicator = indicator_of(status(true, display, system)).expect("enabled");
            assert_eq!(indicator.tone, tone);
            assert_eq!(indicator.details.len(), detail_count);
        }
    }

    #[test]
    fn the_headline_names_exactly_the_aspects_being_held() {
        for (display, system, key) in [
            (
                AspectState::Held,
                AspectState::Held,
                "stay_awake_tooltip_both",
            ),
            (
                AspectState::Held,
                AspectState::Pending,
                "stay_awake_tooltip_display_only",
            ),
            (
                AspectState::Pending,
                AspectState::Held,
                "stay_awake_tooltip_system_only",
            ),
            (
                AspectState::Pending,
                AspectState::Pending,
                "stay_awake_tooltip_starting",
            ),
            (refused(), refused(), "stay_awake_tooltip_inactive"),
        ] {
            let indicator = indicator_of(status(true, display, system)).expect("enabled");
            assert_eq!(indicator.details[0], tr!(key));
        }
    }
}
