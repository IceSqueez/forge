use reqwest::StatusCode;
use thiserror::Error;
use tracing::debug;

use super::TwitchBotAccount;
use crate::chat::refusal_text;
use crate::credentials::load;
use crate::helix::{HelixError, HelixMethod, HelixRequest};

const MODERATORS_PATH: &str = "/helix/moderation/moderators";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotModerator {
    Unknown,
    Moderator,
    NotModerator,
}

impl BotModerator {
    pub(super) fn wire_name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Moderator => "moderator",
            Self::NotModerator => "not_moderator",
        }
    }
}

#[derive(Debug, Error)]
pub enum BotModeratorError {
    #[error("no bot account is signed in")]
    BotNotConnected,
    #[error("sign in to the streamer account again to grant moderator management")]
    BroadcasterReauthRequired,
    #[error("the bot account is banned in this channel; unban it first")]
    BotBanned,
    #[error("the bot account is a VIP; remove its VIP status first")]
    BotIsVip,
    #[error("Twitch refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Unavailable(String),
}

impl TwitchBotAccount {
    pub async fn refresh_moderator_status(&self) {
        let Some(transport) = self.broadcaster_transport.get() else {
            return;
        };
        let Some(bot_id) = self.connected_user_id() else {
            return;
        };
        let Ok(Some(broadcaster)) = load(self.repo.as_ref()).await else {
            return;
        };
        let request = HelixRequest::new(HelixMethod::Get, MODERATORS_PATH)
            .query("broadcaster_id", broadcaster.user_id)
            .query("user_id", bot_id);
        match transport.execute(request).await {
            Ok(body) => {
                let listed = body["data"].as_array().is_some_and(|data| !data.is_empty());
                self.set_moderator(if listed {
                    BotModerator::Moderator
                } else {
                    BotModerator::NotModerator
                });
            }
            Err(e) => debug!(error = %e, "bot moderator status could not be determined"),
        }
    }

    pub async fn make_moderator(&self) -> Result<(), BotModeratorError> {
        let transport = self.broadcaster_transport.get().ok_or_else(|| {
            BotModeratorError::Unavailable("the streamer connection is not ready".to_owned())
        })?;
        let bot_id = self
            .connected_user_id()
            .ok_or(BotModeratorError::BotNotConnected)?;
        let broadcaster = load(self.repo.as_ref())
            .await
            .map_err(|e| BotModeratorError::Unavailable(e.to_string()))?
            .ok_or(BotModeratorError::BroadcasterReauthRequired)?;
        let request = HelixRequest::new(HelixMethod::Post, MODERATORS_PATH)
            .query("broadcaster_id", broadcaster.user_id)
            .query("user_id", bot_id);
        match transport.execute(request).await {
            Ok(_) => {
                self.set_moderator(BotModerator::Moderator);
                Ok(())
            }
            Err(HelixError::ReauthRequired) => Err(BotModeratorError::BroadcasterReauthRequired),
            Err(HelixError::Http { status, body }) => self.classify_rejection(status, &body),
            Err(HelixError::RateLimited) => Err(BotModeratorError::Refused(
                "Twitch is rate limiting moderator changes; try again shortly".to_owned(),
            )),
            Err(other) => Err(BotModeratorError::Unavailable(other.to_string())),
        }
    }

    fn classify_rejection(&self, status: u16, body: &str) -> Result<(), BotModeratorError> {
        let reason = refusal_text(body);
        let lowered = reason.to_lowercase();
        if status == StatusCode::UNPROCESSABLE_ENTITY.as_u16() {
            return Err(BotModeratorError::BotIsVip);
        }
        if status == StatusCode::BAD_REQUEST.as_u16() {
            if lowered.contains("already") {
                self.set_moderator(BotModerator::Moderator);
                return Ok(());
            }
            if lowered.contains("ban") {
                return Err(BotModeratorError::BotBanned);
            }
        }
        Err(BotModeratorError::Refused(reason))
    }
}
