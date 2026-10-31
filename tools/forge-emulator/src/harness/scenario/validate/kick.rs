use super::Validator;
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn require_fake_kick(&mut self, location: &str) {
        let scenario = self.scenario;
        if scenario.fixture.kick.is_none() || scenario.fakes.kick.is_none() {
            self.report(
                location,
                "needs a fake Kick: add fixture.kick and fakes.kick",
            );
        }
    }

    pub(super) fn check_kick_action(&mut self, location: &str, action: &StepAction) {
        match action {
            StepAction::KickChatJoined { within_ms }
            | StepAction::KickChannelPolled { within_ms } => {
                self.check_within(location, *within_ms);
            }
            StepAction::KickChat(message) => {
                self.not_blank(format!("{location}.text"), &message.text);
                self.not_blank(
                    format!("{location}.sender.username"),
                    &message.sender.username,
                );
                if message.sender.user_id == 0 {
                    self.report(format!("{location}.sender.user_id"), "must be non-zero");
                }
            }
            StepAction::KickPusherEvent { event, data } => {
                self.not_blank(format!("{location}.event"), event);
                if !data.is_object() {
                    self.report(
                        format!("{location}.data"),
                        "must be a JSON object: Kick events carry an object encoded as a string",
                    );
                }
            }
            StepAction::KickStream { live } => {
                if *live == self.kick_live {
                    self.report(
                        format!("{location}.live"),
                        format!(
                            "the fake Kick stream is already {}, so this step changes nothing",
                            if *live { "live" } else { "offline" }
                        ),
                    );
                }
                self.kick_live = *live;
            }
            _ => {}
        }
    }
}
