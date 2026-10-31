use forge_types::Variant;

use super::{MAX_PAUSE_MS, MAX_READY_MS, Validator};
use crate::harness::scenario::step::StepAction;

impl<'a> Validator<'a> {
    pub(super) fn check_action(&mut self, index: usize, location: &str, action: &'a StepAction) {
        match action {
            StepAction::ForgeReady { within_ms } => {
                if index > 0 {
                    self.report(location, "only the first step may be forge_ready");
                }
                self.in_range(format!("{location}.within_ms"), *within_ms, 1, MAX_READY_MS);
            }
            StepAction::TwitchSubscribed { types, within_ms } => {
                self.check_subscription_types(&format!("{location}.types"), types);
                self.check_within(location, *within_ms);
                self.session_subscribed = true;
                self.subscriptions_awaited
                    .extend(types.iter().map(String::as_str));
            }
            StepAction::Chat(message) => self.check_chat(location, message),
            StepAction::Crowd(crowd) => self.check_crowd(location, crowd),
            StepAction::TwitchEvent {
                subscription_type,
                event,
            } => self.check_twitch_event(location, subscription_type, event),
            StepAction::SessionReconnect { within_ms } => {
                if !self.session_subscribed {
                    self.report(
                        location,
                        "comes before any twitch_subscribed step, so no EventSub session is known to be live",
                    );
                }
                self.check_within(location, *within_ms);
            }
            StepAction::OverlayPage { overlay, within_ms } => {
                self.require_seeded_overlay(&format!("{location}.overlay"), overlay);
                self.check_within(location, *within_ms);
                self.pages_opened.insert(overlay.as_str());
            }
            StepAction::Pause { ms, reason } => {
                self.in_range(format!("{location}.ms"), *ms, 1, MAX_PAUSE_MS);
                self.not_blank(format!("{location}.reason"), reason);
            }
            StepAction::RunAction { action, .. } => {
                if !self.defines_action(action) {
                    self.report(
                        format!("{location}.action"),
                        format!("names action `{action}`, which the fixture does not define"),
                    );
                }
            }
            StepAction::KickChatJoined { .. }
            | StepAction::KickChat(_)
            | StepAction::KickPusherEvent { .. }
            | StepAction::KickStream { .. }
            | StepAction::KickChannelPolled { .. } => self.check_kick_action(location, action),
            StepAction::YoutubeChatPolled { .. }
            | StepAction::YoutubeChat(_)
            | StepAction::YoutubeChatEvent { .. }
            | StepAction::YoutubeBroadcast { .. } => self.check_youtube_action(location, action),
            StepAction::ObsOnline {}
            | StepAction::ObsIdentified { .. }
            | StepAction::ObsRestart { .. }
            | StepAction::ObsSceneSwitch { .. }
            | StepAction::ObsStream { .. }
            | StepAction::ObsInputMute { .. } => self.check_obs_action(location, action),
            StepAction::VtubeOnline {}
            | StepAction::VtubeAuthenticated { .. }
            | StepAction::VtubeHotkey { .. }
            | StepAction::VtubeModelLoad { .. }
            | StepAction::VtubeModelUnload {}
            | StepAction::VtubeModelConfigChanged {}
            | StepAction::VtubeTracking { .. }
            | StepAction::VtubeItemAdded { .. }
            | StepAction::VtubeItemRemoved { .. }
            | StepAction::VtubeExpression { .. } => self.check_vtube_action(location, action),
            StepAction::DonatelloDonation(_)
            | StepAction::MonobankTopUp(_)
            | StepAction::DonationsPolled { .. }
            | StepAction::ForgeRestart { .. } => self.check_donation_action(location, action),
            StepAction::SetGlobal { name, value, .. } => {
                self.not_blank(format!("{location}.name"), name);
                if let Err(e) = Variant::from_json(value.clone()) {
                    self.report(
                        format!("{location}.value"),
                        format!("cannot be stored as a global: {e}"),
                    );
                }
            }
        }
    }

    pub(super) fn require_seeded_overlay(&mut self, location: &str, overlay: &str) {
        if overlay.trim().is_empty() {
            self.report(location, "must not be blank");
        } else if !self.scenario.fixture.declares_overlay(overlay) {
            let message = format!("names overlay `{overlay}`, which the fixture does not declare");
            self.report(location, message);
        }
    }

    pub(super) fn defines_action(&self, name: &str) -> bool {
        self.scenario
            .fixture
            .actions()
            .iter()
            .any(|action| action.name == name)
    }
}
