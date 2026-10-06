use axum::http::StatusCode;
use serde_json::{Value, json};

use super::config::FakeTwitchConfig;
use super::rewards::RewardAnswer;

pub(crate) const BAN: &str = "channel.ban";
pub(crate) const UNBAN: &str = "channel.unban";

const DEFAULT_PAGE: usize = 20;
const MAX_PAGE: usize = 100;
const CURSOR_PREFIX: &str = "bans-after-";
const NOT_BANNED: &str = "The user is not banned or in a timeout.";
const BAD_FIRST: &str = "The parameter first must be between 1 and 100.";
const BAD_CURSOR: &str = "The cursor in the after parameter is not valid.";
const FOREIGN_BROADCASTER_ID: &str =
    "The ID in broadcaster_id must match the user ID found in the request's OAuth token.";
const FOREIGN_MODERATOR_ID: &str =
    "The ID in moderator_id must match the user ID found in the request's OAuth token.";

#[derive(Default)]
pub(crate) struct BanStore {
    rows: Vec<Value>,
}

fn param<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn required<'a>(query: &'a [(String, String)], key: &str) -> Result<&'a str, RewardAnswer> {
    param(query, key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            refused(
                StatusCode::BAD_REQUEST,
                &format!("Missing required parameter {key}"),
            )
        })
}

fn refused(status: StatusCode, message: &str) -> RewardAnswer {
    RewardAnswer {
        status,
        body: json!({
            "error": status.canonical_reason().unwrap_or_default(),
            "status": status.as_u16(),
            "message": message,
        }),
        notification: None,
    }
}

fn text(event: &Value, key: &str) -> Value {
    json!(event[key].as_str().unwrap_or_default())
}

fn banned_row(event: &Value) -> Value {
    let expires_at = match event["ends_at"].as_str() {
        Some(ends_at) if event["is_permanent"] != Value::Bool(true) => ends_at,
        _ => "",
    };
    json!({
        "user_id": text(event, "user_id"),
        "user_login": text(event, "user_login"),
        "user_name": text(event, "user_name"),
        "expires_at": expires_at,
        "created_at": text(event, "banned_at"),
        "reason": text(event, "reason"),
        "moderator_id": text(event, "moderator_user_id"),
        "moderator_login": text(event, "moderator_user_login"),
        "moderator_name": text(event, "moderator_user_name"),
    })
}

fn page_size(query: &[(String, String)]) -> Result<usize, RewardAnswer> {
    match param(query, "first") {
        None => Ok(DEFAULT_PAGE),
        Some(raw) => raw
            .parse()
            .ok()
            .filter(|size| (1..=MAX_PAGE).contains(size))
            .ok_or_else(|| refused(StatusCode::BAD_REQUEST, BAD_FIRST)),
    }
}

fn page_offset(query: &[(String, String)]) -> Result<usize, RewardAnswer> {
    match param(query, "after") {
        None => Ok(0),
        Some(cursor) => cursor
            .strip_prefix(CURSOR_PREFIX)
            .and_then(|offset| offset.parse().ok())
            .ok_or_else(|| refused(StatusCode::BAD_REQUEST, BAD_CURSOR)),
    }
}

impl BanStore {
    pub(crate) fn remember(&mut self, subscription_type: &str, event: &Value) {
        let Some(user_id) = event["user_id"].as_str() else {
            return;
        };
        self.rows.retain(|row| row["user_id"] != user_id);
        if subscription_type == BAN {
            self.rows.push(banned_row(event));
        }
    }

    fn broadcaster_refusal(
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> Result<(), RewardAnswer> {
        if required(query, "broadcaster_id")? != config.broadcaster_user_id {
            return Err(refused(StatusCode::UNAUTHORIZED, FOREIGN_BROADCASTER_ID));
        }
        Ok(())
    }

    pub(crate) fn list(
        &self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> RewardAnswer {
        let page = Self::broadcaster_refusal(config, query)
            .and_then(|()| Ok((page_size(query)?, page_offset(query)?)));
        let (size, offset) = match page {
            Ok(page) => page,
            Err(refusal) => return refusal,
        };
        let data: Vec<Value> = self
            .rows
            .iter()
            .rev()
            .skip(offset)
            .take(size)
            .cloned()
            .collect();
        let next = offset + size;
        let pagination = if next < self.rows.len() {
            json!({ "cursor": format!("{CURSOR_PREFIX}{next}") })
        } else {
            json!({})
        };
        RewardAnswer {
            status: StatusCode::OK,
            body: json!({ "data": data, "pagination": pagination }),
            notification: None,
        }
    }

    pub(crate) fn unban(
        &mut self,
        config: &FakeTwitchConfig,
        query: &[(String, String)],
    ) -> RewardAnswer {
        let target = Self::broadcaster_refusal(config, query).and_then(|()| {
            Ok((
                required(query, "moderator_id")?,
                required(query, "user_id")?,
            ))
        });
        let (moderator_id, user_id) = match target {
            Ok(target) => target,
            Err(refusal) => return refusal,
        };
        if moderator_id != config.broadcaster_user_id {
            return refused(StatusCode::UNAUTHORIZED, FOREIGN_MODERATOR_ID);
        }
        let Some(index) = self.rows.iter().position(|row| row["user_id"] == user_id) else {
            return refused(StatusCode::BAD_REQUEST, NOT_BANNED);
        };
        let removed = self.rows.remove(index);
        let event = json!({
            "user_id": removed["user_id"],
            "user_login": removed["user_login"],
            "user_name": removed["user_name"],
            "broadcaster_user_id": config.broadcaster_user_id,
            "broadcaster_user_login": config.broadcaster_login,
            "broadcaster_user_name": config.broadcaster_login,
            "moderator_user_id": config.broadcaster_user_id,
            "moderator_user_login": config.broadcaster_login,
            "moderator_user_name": config.broadcaster_login,
        });
        RewardAnswer {
            status: StatusCode::NO_CONTENT,
            body: Value::Null,
            notification: Some((UNBAN, event)),
        }
    }
}
