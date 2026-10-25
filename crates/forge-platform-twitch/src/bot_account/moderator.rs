use reqwest::StatusCode;
use thiserror::Error;
use tracing::debug;

use super::TwitchBotAccount;
use crate::chat::refusal_text;
use crate::credentials::load;
use crate::helix::{HelixError, HelixMethod, HelixRequest};

const MODERATORS_PATH: &str = "/helix/moderation/moderators";
const ALREADY_MODERATOR_MARKER: &str = "already";
const BAD_REQUEST: u16 = StatusCode::BAD_REQUEST.as_u16();

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
            .query("user_id", bot_id)
            .expect_status_with_excerpt(BAD_REQUEST, ALREADY_MODERATOR_MARKER);
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
        if status == BAD_REQUEST {
            if lowered.contains(ALREADY_MODERATOR_MARKER) {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use wiremock::ResponseTemplate;

    use super::*;
    use crate::bot_account::test_support::{BOT_LOGIN, BOT_USER_ID, BotHarness};
    use crate::bot_account::{BOT_STATUS_CHANGED_KIND, BotStatus};
    use crate::sub_actions::test_support::{MockTransport, SELF_USER_ID};

    fn listed() -> Result<serde_json::Value, HelixError> {
        Ok(serde_json::json!({"data": [{"user_id": BOT_USER_ID, "user_login": BOT_LOGIN}]}))
    }

    fn unlisted() -> Result<serde_json::Value, HelixError> {
        Ok(serde_json::json!({"data": []}))
    }

    async fn connected_with(
        broadcaster_responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (BotHarness, Arc<MockTransport>) {
        let harness = BotHarness::connected().await;
        let broadcaster = Arc::new(MockTransport::returning_sequence(broadcaster_responses));
        harness
            .bot
            .attach_broadcaster_transport(Arc::clone(&broadcaster) as _);
        (harness, broadcaster)
    }

    #[tokio::test]
    async fn moderator_lookup_classifies_the_bot_from_the_broadcasters_moderator_list() {
        let cases: [(Result<serde_json::Value, HelixError>, BotModerator); 3] = [
            (listed(), BotModerator::Moderator),
            (unlisted(), BotModerator::NotModerator),
            (
                Err(HelixError::Http {
                    status: 500,
                    body: "boom".to_owned(),
                }),
                BotModerator::Unknown,
            ),
        ];

        for (response, expected) in cases {
            let (harness, broadcaster) = connected_with(vec![response]).await;

            harness.bot.refresh_moderator_status().await;

            assert_eq!(harness.bot.moderator_status(), expected);
            let lookup = broadcaster.last_request();
            assert_eq!(lookup.method, HelixMethod::Get);
            assert_eq!(lookup.path, MODERATORS_PATH);
            assert!(
                lookup
                    .query
                    .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
            );
            assert!(
                lookup
                    .query
                    .contains(&("user_id".to_owned(), BOT_USER_ID.to_owned()))
            );
        }
    }

    #[tokio::test]
    async fn sync_status_looks_up_moderator_status_only_for_a_usable_bot() {
        for (validate_status, expected_lookups) in [(200, 1), (401, 0)] {
            let harness = BotHarness::with_bot_row().await;
            let broadcaster = Arc::new(MockTransport::returning(listed()));
            harness
                .bot
                .attach_broadcaster_transport(Arc::clone(&broadcaster) as _);
            harness
                .mount_validate(
                    validate_status,
                    crate::auth::TWITCH_BOT_SCOPES,
                    crate::bot_account::test_support::CLIENT_ID,
                )
                .await;

            harness.bot.sync_status().await;

            assert_eq!(
                broadcaster.call_count(),
                expected_lookups,
                "validate {validate_status}"
            );
        }
    }

    #[tokio::test]
    async fn a_moderator_only_change_publishes_the_status_event_with_the_moderator_field() {
        let (harness, _broadcaster) = connected_with(vec![listed(), listed()]).await;

        harness.bot.refresh_moderator_status().await;
        harness.bot.refresh_moderator_status().await;

        let payloads: Vec<serde_json::Value> = harness
            .bus
            .of_kind(BOT_STATUS_CHANGED_KIND)
            .into_iter()
            .map(|event| event.payload)
            .collect();
        assert_eq!(
            payloads,
            vec![
                serde_json::json!({"status": "connected", "login": BOT_LOGIN, "moderator": "unknown"}),
                serde_json::json!({"status": "connected", "login": BOT_LOGIN, "moderator": "moderator"}),
            ]
        );
    }

    #[tokio::test]
    async fn a_fresh_grant_looks_up_the_moderator_status() {
        let harness = BotHarness::without_bot_row().await;
        harness
            .bot
            .attach_broadcaster_transport(Arc::new(MockTransport::returning(listed())));
        let bundle = crate::auth::TwitchAuthBundle {
            access_token: forge_types::OAuthToken::new("grant_access"),
            refresh_token: None,
            user_info: crate::auth::UserInfo {
                id: BOT_USER_ID.to_owned(),
                login: BOT_LOGIN.to_owned(),
                display_name: BOT_LOGIN.to_owned(),
                broadcaster_type: Default::default(),
            },
            client_id: crate::bot_account::test_support::CLIENT_ID.to_owned(),
            expires_at: None,
        };

        harness.bot.adopt_grant(bundle).await.unwrap();

        assert_eq!(harness.bot.moderator_status(), BotModerator::Moderator);
    }

    #[tokio::test]
    async fn logout_forgets_the_moderator_status() {
        let (harness, _broadcaster) = connected_with(vec![listed()]).await;
        harness.bot.refresh_moderator_status().await;
        harness
            .mount("POST", "/revoke", ResponseTemplate::new(200))
            .await;

        harness.bot.logout().await.unwrap();

        assert_eq!(harness.bot.moderator_status(), BotModerator::Unknown);
        assert_eq!(harness.bot.status(), BotStatus::NotConnected);
    }

    fn http(status: u16, message: &str) -> Result<serde_json::Value, HelixError> {
        Err(HelixError::Http {
            status,
            body: serde_json::json!({"status": status, "message": message}).to_string(),
        })
    }

    #[tokio::test]
    async fn make_moderator_maps_every_twitch_answer_to_a_typed_outcome() {
        type Expectation = fn(&Result<(), BotModeratorError>) -> bool;
        let cases: [(
            Result<serde_json::Value, HelixError>,
            Expectation,
            BotModerator,
        ); 8] = [
            (
                Ok(serde_json::Value::Null),
                |r| r.is_ok(),
                BotModerator::Moderator,
            ),
            (
                http(400, "user is already a mod"),
                |r| r.is_ok(),
                BotModerator::Moderator,
            ),
            (
                http(400, "The user is banned from this channel"),
                |r| matches!(r, Err(BotModeratorError::BotBanned)),
                BotModerator::Unknown,
            ),
            (
                http(422, "user is a vip"),
                |r| matches!(r, Err(BotModeratorError::BotIsVip)),
                BotModerator::Unknown,
            ),
            (
                Err(HelixError::ReauthRequired),
                |r| matches!(r, Err(BotModeratorError::BroadcasterReauthRequired)),
                BotModerator::Unknown,
            ),
            (
                http(400, "Missing user_id"),
                |r| matches!(r, Err(BotModeratorError::Refused(m)) if m == "Missing user_id"),
                BotModerator::Unknown,
            ),
            (
                Err(HelixError::RateLimited),
                |r| matches!(r, Err(BotModeratorError::Refused(_))),
                BotModerator::Unknown,
            ),
            (
                Err(HelixError::Transport("offline".to_owned())),
                |r| matches!(r, Err(BotModeratorError::Unavailable(_))),
                BotModerator::Unknown,
            ),
        ];

        for (response, expected, status_after) in cases {
            let label = format!("{response:?}");
            let (harness, broadcaster) = connected_with(vec![response]).await;

            let result = harness.bot.make_moderator().await;

            assert!(expected(&result), "{label} -> {result:?}");
            assert_eq!(harness.bot.moderator_status(), status_after, "{label}");
            let request = broadcaster.last_request();
            assert_eq!(request.method, HelixMethod::Post, "{label}");
            assert!(
                request
                    .query
                    .contains(&("user_id".to_owned(), BOT_USER_ID.to_owned())),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn make_moderator_needs_a_signed_in_bot_and_a_streamer_connection() {
        let harness = BotHarness::without_bot_row().await;
        let without_streamer = harness.bot.make_moderator().await;
        let broadcaster = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        harness
            .bot
            .attach_broadcaster_transport(Arc::clone(&broadcaster) as _);

        let without_bot = harness.bot.make_moderator().await;

        assert!(matches!(
            without_streamer,
            Err(BotModeratorError::Unavailable(_))
        ));
        assert!(matches!(
            without_bot,
            Err(BotModeratorError::BotNotConnected)
        ));
        assert_eq!(broadcaster.call_count(), 0);
    }
}
