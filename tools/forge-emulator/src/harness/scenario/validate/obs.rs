use super::{MAX_WAIT_MS, Validator};
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn require_fake_obs(&mut self, location: &str) {
        if self.scenario.fakes.obs.is_none() {
            self.report(location, "needs a fake OBS: add fixture.obs and fakes.obs");
        }
    }

    pub(super) fn check_obs_action(&mut self, location: &str, action: &StepAction) {
        let obs = self.scenario.fakes.obs.as_ref();
        match action {
            StepAction::ObsOnline {} => {
                if self.obs_online {
                    self.report(
                        location,
                        "the fake OBS is already running; set fakes.obs.online_at_boot to false to start it from a step",
                    );
                }
                self.obs_online = true;
            }
            StepAction::ObsIdentified { within_ms } => {
                self.check_within(location, *within_ms);
            }
            StepAction::ObsRestart { down_ms } => {
                if !self.obs_online {
                    self.report(
                        location,
                        "the fake OBS is not running yet, so there is nothing to restart",
                    );
                }
                self.in_range(format!("{location}.down_ms"), *down_ms, 1, MAX_WAIT_MS);
            }
            StepAction::ObsSceneSwitch { scene }
                if obs.is_some_and(|obs| !obs.declares_scene(scene)) =>
            {
                self.report(
                    format!("{location}.scene"),
                    format!("names scene `{scene}`, which fakes.obs.scenes does not list"),
                );
            }
            StepAction::ObsInputMute { input, .. }
                if obs.is_some_and(|obs| !obs.declares_input(input)) =>
            {
                self.report(
                    format!("{location}.input"),
                    format!("names input `{input}`, which fakes.obs.inputs does not list"),
                );
            }
            _ => {}
        }
    }
}
