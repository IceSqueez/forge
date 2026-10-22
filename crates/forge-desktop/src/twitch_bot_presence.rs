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
