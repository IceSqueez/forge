use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_platform_twitch::{BOT_STATUS_CHANGED_KIND, BotModerator, BotStatus, TwitchBotAccount};
use gpui::{AsyncApp, Entity};

pub struct TwitchBotPresence {
    status: BotStatus,
    moderator: BotModerator,
    composer_sends_as_bot: bool,
    heard_live_status: bool,
}

impl Default for TwitchBotPresence {
    fn default() -> Self {
        Self::new()
    }
}

impl TwitchBotPresence {
    pub fn new() -> Self {
        Self {
            status: BotStatus::NotConnected,
            moderator: BotModerator::Unknown,
            composer_sends_as_bot: false,
            heard_live_status: false,
        }
    }

    pub fn status(&self) -> &BotStatus {
        &self.status
    }

    pub fn moderator(&self) -> BotModerator {
        self.moderator
    }

    pub fn connected_login(&self) -> Option<&str> {
        match &self.status {
            BotStatus::Connected { login } => Some(login),
            BotStatus::NotConnected | BotStatus::ReauthRequired { .. } => None,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected_login().is_some()
    }

    pub fn composer_sends_as_bot(&self) -> bool {
        self.composer_sends_as_bot && self.is_connected()
    }

    pub fn set_composer_sends_as_bot(&mut self, as_bot: bool) -> bool {
        let next = as_bot && self.is_connected();
        let changed = self.composer_sends_as_bot != next;
        self.composer_sends_as_bot = next;
        changed
    }

    pub fn apply(&mut self, status: BotStatus, moderator: BotModerator) -> bool {
        if self.status == status && self.moderator == moderator {
            return false;
        }
        self.status = status;
        self.moderator = moderator;
        if !self.is_connected() {
            self.composer_sends_as_bot = false;
        }
        true
    }

    pub fn apply_event(&mut self, event: &Event) -> bool {
        if event.source != EventSource::Twitch || event.kind != BOT_STATUS_CHANGED_KIND {
            return false;
        }
        let payload = &event.payload;
        let login = payload.get("login").and_then(|value| value.as_str());
        let Some(status) = payload
            .get("status")
            .and_then(|value| value.as_str())
            .and_then(|wire| status_from_wire(wire, login))
        else {
            return false;
        };
        let moderator = payload
            .get("moderator")
            .and_then(|value| value.as_str())
            .and_then(moderator_from_wire)
            .unwrap_or(BotModerator::Unknown);
        self.heard_live_status = true;
        self.apply(status, moderator)
    }

    pub fn apply_seed(&mut self, status: BotStatus, moderator: BotModerator) -> bool {
        !self.heard_live_status && self.apply(status, moderator)
    }
}

fn status_from_wire(wire: &str, login: Option<&str>) -> Option<BotStatus> {
    match (wire, login) {
        ("not_connected", _) => Some(BotStatus::NotConnected),
        ("connected", Some(login)) => Some(BotStatus::Connected {
            login: login.to_owned(),
        }),
        ("reauth_required", Some(login)) => Some(BotStatus::ReauthRequired {
            login: login.to_owned(),
        }),
        _ => None,
    }
}

fn moderator_from_wire(wire: &str) -> Option<BotModerator> {
    match wire {
        "unknown" => Some(BotModerator::Unknown),
        "moderator" => Some(BotModerator::Moderator),
        "not_moderator" => Some(BotModerator::NotModerator),
        _ => None,
    }
}

pub fn start_twitch_bot_seed(
    cx: &mut AsyncApp,
    presence: Entity<TwitchBotPresence>,
    bot: Arc<TwitchBotAccount>,
    rt_handle: &tokio::runtime::Handle,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send((bot.status(), bot.moderator_status()));
    });
    cx.spawn(async move |cx| {
        if let Ok((status, moderator)) = rx.await {
            presence.update(cx, |presence, cx| {
                if presence.apply_seed(status, moderator) {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_platform_twitch::{BOT_STATUS_CHANGED_KIND, BotModerator, BotStatus};
    use serde_json::json;

    use super::TwitchBotPresence;

    fn status_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Twitch, BOT_STATUS_CHANGED_KIND, payload)
    }

    fn connected(login: &str) -> BotStatus {
        BotStatus::Connected {
            login: login.to_owned(),
        }
    }

    fn connected_presence() -> TwitchBotPresence {
        let mut presence = TwitchBotPresence::new();
        presence.apply(connected("forgebot"), BotModerator::Moderator);
        presence
    }

    #[test]
    fn status_event_reads_every_wire_status_with_its_login_and_moderator() {
        for (payload, status, moderator) in [
            (
                json!({"status": "connected", "login": "forgebot", "moderator": "not_moderator"}),
                connected("forgebot"),
                BotModerator::NotModerator,
            ),
            (
                json!({"status": "reauth_required", "login": "forgebot", "moderator": "moderator"}),
                BotStatus::ReauthRequired {
                    login: "forgebot".to_owned(),
                },
                BotModerator::Moderator,
            ),
            (
                json!({"status": "not_connected", "login": null, "moderator": "unknown"}),
                BotStatus::NotConnected,
                BotModerator::Unknown,
            ),
        ] {
            let mut presence = TwitchBotPresence::new();
            presence.apply(connected("someone_else"), BotModerator::Moderator);

            presence.apply_event(&status_event(payload));

            assert_eq!(
                (presence.status(), presence.moderator()),
                (&status, moderator)
            );
        }
    }

    #[test]
    fn status_event_with_an_unknown_moderator_word_reads_moderator_as_unknown() {
        let mut presence = TwitchBotPresence::new();

        presence.apply_event(&status_event(
            json!({"status": "connected", "login": "forgebot", "moderator": "owner"}),
        ));

        assert_eq!(presence.moderator(), BotModerator::Unknown);
    }

    #[test]
    fn events_of_another_kind_or_source_leave_the_presence_untouched() {
        let payload = json!({"status": "connected", "login": "forgebot", "moderator": "moderator"});
        for event in [
            Event::new(EventSource::Kick, BOT_STATUS_CHANGED_KIND, payload.clone()),
            Event::new(EventSource::Twitch, "twitch.chat.message", payload.clone()),
        ] {
            let mut presence = TwitchBotPresence::new();

            let changed = presence.apply_event(&event);

            assert!(!changed && !presence.is_connected(), "{}", event.kind);
        }
    }

    #[test]
    fn malformed_status_events_change_nothing_and_do_not_block_the_seed() {
        for payload in [
            json!({"status": "connected", "moderator": "moderator"}),
            json!({"status": "reauth_required", "login": null}),
            json!({"status": "linked", "login": "forgebot"}),
            json!({"login": "forgebot"}),
        ] {
            let mut presence = TwitchBotPresence::new();

            let changed = presence.apply_event(&status_event(payload.clone()));
            presence.apply_seed(connected("forgebot"), BotModerator::Moderator);

            assert!(!changed && presence.is_connected(), "{payload}");
        }
    }

    #[test]
    fn seed_applies_while_no_live_status_was_heard() {
        let mut presence = TwitchBotPresence::new();

        let changed = presence.apply_seed(connected("forgebot"), BotModerator::NotModerator);

        assert!(changed);
        assert_eq!(presence.connected_login(), Some("forgebot"));
    }

    #[test]
    fn late_seed_never_overwrites_a_live_status_even_one_equal_to_the_initial_state() {
        let mut presence = TwitchBotPresence::new();
        presence.apply_event(&status_event(
            json!({"status": "not_connected", "login": null, "moderator": "unknown"}),
        ));

        let changed = presence.apply_seed(connected("forgebot"), BotModerator::Moderator);

        assert!(!changed);
        assert_eq!(presence.status(), &BotStatus::NotConnected);
    }

    #[test]
    fn apply_reports_a_change_only_when_status_or_moderator_differs() {
        for (status, moderator, expected) in [
            (connected("forgebot"), BotModerator::Moderator, false),
            (connected("forgebot"), BotModerator::NotModerator, true),
            (connected("otherbot"), BotModerator::Moderator, true),
            (BotStatus::NotConnected, BotModerator::Moderator, true),
        ] {
            let mut presence = connected_presence();

            assert_eq!(
                presence.apply(status.clone(), moderator),
                expected,
                "{status:?} {moderator:?}"
            );
        }
    }

    #[test]
    fn composer_cannot_pick_the_bot_while_no_bot_is_connected() {
        for status in [
            BotStatus::NotConnected,
            BotStatus::ReauthRequired {
                login: "forgebot".to_owned(),
            },
        ] {
            let mut presence = TwitchBotPresence::new();
            presence.apply(status.clone(), BotModerator::Unknown);

            let changed = presence.set_composer_sends_as_bot(true);

            assert!(!changed && !presence.composer_sends_as_bot(), "{status:?}");
        }
    }

    #[test]
    fn composer_choice_toggles_between_bot_and_me_while_connected() {
        let mut presence = connected_presence();

        let to_bot = presence.set_composer_sends_as_bot(true);
        let again = presence.set_composer_sends_as_bot(true);
        let to_me = presence.set_composer_sends_as_bot(false);

        assert_eq!((to_bot, again, to_me), (true, false, true));
        assert!(!presence.composer_sends_as_bot());
    }

    #[test]
    fn leaving_connected_resets_the_composer_to_me_for_the_next_connection() {
        for leave in [
            BotStatus::NotConnected,
            BotStatus::ReauthRequired {
                login: "forgebot".to_owned(),
            },
        ] {
            let mut presence = connected_presence();
            presence.set_composer_sends_as_bot(true);

            presence.apply(leave.clone(), BotModerator::Unknown);
            presence.apply(connected("forgebot"), BotModerator::Moderator);

            assert!(!presence.composer_sends_as_bot(), "{leave:?}");
        }
    }

    #[test]
    fn a_moderator_only_change_keeps_the_composer_on_the_bot() {
        let mut presence = connected_presence();
        presence.set_composer_sends_as_bot(true);

        presence.apply(connected("forgebot"), BotModerator::NotModerator);

        assert!(presence.composer_sends_as_bot());
    }
}
