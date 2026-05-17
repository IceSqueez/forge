use forge_widgets::{StepInfo, StepStatus};

use crate::screen::OnboardingStep;

pub struct OnboardingState {
    pub selected_platform: Option<String>,
    pub step_infos: Vec<StepInfo>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self::new()
    }
}

impl OnboardingState {
    pub fn new() -> Self {
        Self {
            selected_platform: None,
            step_infos: Self::build_step_infos(&OnboardingStep::Welcome),
        }
    }

    pub fn select_platform(&mut self, id: String) {
        self.selected_platform = Some(id);
    }

    pub fn sync_step(&mut self, step: &OnboardingStep) {
        self.step_infos = Self::build_step_infos(step);
    }

    fn build_step_infos(current: &OnboardingStep) -> Vec<StepInfo> {
        let step_index = match current {
            OnboardingStep::Welcome => 0,
            OnboardingStep::ConnectPlatform => 1,
            OnboardingStep::ConnectObs => 2,
            OnboardingStep::StarterPack => 3,
            OnboardingStep::Ready => 4,
        };

        let entries: [(&'static str, &'static str); 5] = [
            ("Welcome", "Theme picked"),
            ("Connect platform", "Optional"),
            ("Connect OBS", "Optional"),
            ("Starter pack", "Optional"),
            ("You're ready", "Start streaming"),
        ];

        entries
            .iter()
            .enumerate()
            .map(|(i, (label, sublabel))| {
                let status = if i < step_index {
                    StepStatus::Done
                } else if i == step_index {
                    StepStatus::Current
                } else {
                    StepStatus::Pending
                };
                StepInfo {
                    label,
                    sublabel,
                    status,
                }
            })
            .collect()
    }
}
