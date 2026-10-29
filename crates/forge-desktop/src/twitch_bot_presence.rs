use std::sync::Arc;

use forge_platform_twitch::vocabulary::BotStatusChanged;
use forge_platform_twitch::{BotModerator, BotStatus, TwitchBotAccount};
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

    pub fn apply_status_changed(&mut self, changed: &BotStatusChanged) -> bool {
        let Some(status) = changed.bot_status() else {
            return false;
        };
        self.heard_live_status = true;
        self.apply(status, changed.moderator)
    }

    pub fn apply_seed(&mut self, status: BotStatus, moderator: BotModerator) -> bool {
        !self.heard_live_status && self.apply(status, moderator)
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
    use forge_platform_twitch::vocabulary::{BotStatusChanged, BotStatusTag};
    use forge_platform_twitch::{BotModerator, BotStatus};

    use super::TwitchBotPresence;

    fn status_changed(
        status: BotStatusTag,
        login: Option<&str>,
        moderator: BotModerator,
    ) -> BotStatusChanged {
        BotStatusChanged {
            status,
            login: login.map(str::to_owned),
            moderator,
        }
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
    fn a_live_status_change_replaces_status_and_moderator() {
        for (changed, status, moderator) in [
            (
                status_changed(
                    BotStatusTag::Connected,
                    Some("forgebot"),
                    BotModerator::NotModerator,
                ),
                connected("forgebot"),
                BotModerator::NotModerator,
            ),
            (
                status_changed(
                    BotStatusTag::ReauthRequired,
                    Some("forgebot"),
                    BotModerator::Moderator,
                ),
                BotStatus::ReauthRequired {
                    login: "forgebot".to_owned(),
                },
                BotModerator::Moderator,
            ),
            (
                status_changed(BotStatusTag::NotConnected, None, BotModerator::Unknown),
                BotStatus::NotConnected,
                BotModerator::Unknown,
            ),
        ] {
            let mut presence = TwitchBotPresence::new();
            presence.apply(connected("someone_else"), BotModerator::Moderator);

            presence.apply_status_changed(&changed);

            assert_eq!(
                (presence.status(), presence.moderator()),
                (&status, moderator)
            );
        }
    }

    #[test]
    fn a_status_change_without_the_login_it_needs_changes_nothing_and_does_not_block_the_seed() {
        for tag in [BotStatusTag::Connected, BotStatusTag::ReauthRequired] {
            let mut presence = TwitchBotPresence::new();

            let changed =
                presence.apply_status_changed(&status_changed(tag, None, BotModerator::Moderator));
            presence.apply_seed(connected("forgebot"), BotModerator::Moderator);

            assert!(!changed && presence.is_connected(), "{tag:?}");
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
        presence.apply_status_changed(&status_changed(
            BotStatusTag::NotConnected,
            None,
            BotModerator::Unknown,
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
