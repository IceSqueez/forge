use forge_platform_twitch::vocabulary::topics;

use super::Validator;

const CHAT_SUBSCRIPTION: &str = topics::CHANNEL_CHAT_MESSAGE;

impl<'a> Validator<'a> {
    pub(super) fn require_fake_twitch(&mut self, location: &str) {
        if !self.fake_twitch {
            self.report(
                location,
                "needs a fake Twitch: add fixture.twitch and fakes.twitch",
            );
        }
    }

    pub(super) fn check_subscription_types(&mut self, location: &str, types: &[String]) {
        if types.is_empty() {
            self.report(location, "must list at least one subscription type");
        }
        for (index, kind) in types.iter().enumerate() {
            let at = format!("{location}[{index}]");
            if kind.trim().is_empty() {
                self.report(at, "must not be blank");
            } else if types[..index].contains(kind) {
                self.report(at, format!("repeats `{kind}`"));
            }
        }
    }

    pub(super) fn require_chat_subscription(&mut self, location: &str) {
        if !self.subscriptions_awaited.contains(CHAT_SUBSCRIPTION) {
            self.report(
                location,
                format!(
                    "sends chat before any twitch_subscribed step waits for {CHAT_SUBSCRIPTION}, so the fake may have nowhere to deliver it"
                ),
            );
        }
    }

    pub(super) fn check_twitch_event(
        &mut self,
        location: &str,
        subscription_type: &str,
        event: &serde_json::Value,
    ) {
        self.not_blank(format!("{location}.subscription_type"), subscription_type);
        if !event.is_object() {
            self.report(
                format!("{location}.event"),
                "must be a JSON object: EventSub carries every event payload as one",
            );
        }
        if !subscription_type.trim().is_empty()
            && !self.subscriptions_awaited.contains(subscription_type)
        {
            let message = format!(
                "injects `{subscription_type}` before any twitch_subscribed step waits for it, so the fake may have nowhere to deliver it"
            );
            self.report(location, message);
        }
    }
}
