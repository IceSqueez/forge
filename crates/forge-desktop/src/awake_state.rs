use forge_awake::{Aspect, AspectState, AwakeStatus};
use forge_components::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwakeTone {
    Normal,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwakeIndicator {
    pub tone: AwakeTone,
    pub details: Vec<String>,
}

#[derive(Default)]
pub struct AwakeState {
    status: Option<AwakeStatus>,
}

impl AwakeState {
    pub fn new() -> Self {
        Self { status: None }
    }

    pub fn apply(&mut self, status: AwakeStatus) -> bool {
        if self.status.as_ref() == Some(&status) {
            return false;
        }
        self.status = Some(status);
        true
    }

    pub fn enabled(&self) -> Option<bool> {
        self.status.as_ref().map(|status| status.enabled)
    }

    pub fn indicator(&self) -> Option<AwakeIndicator> {
        let status = self.status.as_ref().filter(|status| status.enabled)?;
        let held: Vec<Aspect> = Aspect::ALL
            .into_iter()
            .filter(|aspect| *status.aspect(*aspect) == AspectState::Held)
            .collect();
        let refused: Vec<(Aspect, &str)> = Aspect::ALL
            .into_iter()
            .filter_map(|aspect| match status.aspect(aspect) {
                AspectState::Unavailable { reason } => Some((aspect, reason.as_str())),
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
            let reason = (*reason).to_owned();
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
