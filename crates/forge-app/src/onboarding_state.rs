use std::time::SystemTime;

use forge_widgets::{StepInfo, StepStatus};

use crate::screen::OnboardingStep;

#[derive(Debug, Clone)]
pub struct DeviceCodeSession {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: SystemTime,
    pub status: DeviceCodeStatus,
}

#[derive(Debug, Clone)]
pub enum DeviceCodeStatus {
    Requesting,
    Waiting,
    Success,
    Error(String),
    MissingClientId,
}

pub struct OnboardingState {
    pub selected_platform: Option<String>,
    pub step_infos: Vec<StepInfo>,
    pub device_code: Option<DeviceCodeSession>,
    pub obs_url: String,
    pub obs_password: String,
    pub obs_connecting: bool,
    pub obs_connect_error: Option<String>,
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
            device_code: None,
            obs_url: "ws://127.0.0.1:4455".to_string(),
            obs_password: String::new(),
            obs_connecting: false,
            obs_connect_error: None,
        }
    }

    pub fn clear_device_code(&mut self) {
        self.device_code = None;
    }

    pub fn select_platform(&mut self, id: String) {
        self.selected_platform = Some(id);
    }

    pub fn continue_label(&self) -> &'static str {
        match self.selected_platform.as_deref() {
            Some("twitch") => "Continue with Twitch",
            Some("youtube") => "Continue with YouTube",
            Some("kick") => "Continue with Kick",
            Some("trovo") => "Continue with Trovo",
            _ => "Continue",
        }
    }

    pub fn sync_step(&mut self, step: &OnboardingStep) {
        self.step_infos = Self::build_step_infos(step);
    }

    fn build_step_infos(current: &OnboardingStep) -> Vec<StepInfo> {
        let step_index = match current {
            OnboardingStep::Welcome => 0,
            OnboardingStep::ConnectPlatform | OnboardingStep::DeviceCodeFlow(_) => 1,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continue_label_with_no_selection_is_generic() {
        let state = OnboardingState::new();
        assert_eq!(state.continue_label(), "Continue");
    }

    #[test]
    fn continue_label_twitch() {
        let mut state = OnboardingState::new();
        state.select_platform("twitch".into());
        assert_eq!(state.continue_label(), "Continue with Twitch");
    }

    #[test]
    fn continue_label_youtube() {
        let mut state = OnboardingState::new();
        state.select_platform("youtube".into());
        assert_eq!(state.continue_label(), "Continue with YouTube");
    }

    #[test]
    fn continue_label_kick() {
        let mut state = OnboardingState::new();
        state.select_platform("kick".into());
        assert_eq!(state.continue_label(), "Continue with Kick");
    }

    #[test]
    fn continue_label_trovo() {
        let mut state = OnboardingState::new();
        state.select_platform("trovo".into());
        assert_eq!(state.continue_label(), "Continue with Trovo");
    }

    #[test]
    fn device_code_flow_maps_to_connect_platform_step_index() {
        let mut state = OnboardingState::new();
        state.sync_step(&OnboardingStep::DeviceCodeFlow("twitch".into()));
        let connect_platform_step = &state.step_infos[1];
        assert_eq!(connect_platform_step.status, StepStatus::Current);
    }

    #[test]
    fn sync_step_marks_prior_steps_done() {
        let mut state = OnboardingState::new();
        state.sync_step(&OnboardingStep::ConnectObs);
        assert_eq!(state.step_infos[0].status, StepStatus::Done);
        assert_eq!(state.step_infos[1].status, StepStatus::Done);
        assert_eq!(state.step_infos[2].status, StepStatus::Current);
    }
}
