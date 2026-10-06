use async_trait::async_trait;
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility,
};
use reqwest::StatusCode;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::builtin::TwitchIntegrationBundle;
use crate::helix::{HelixError, HelixMethod, HelixRequest};

const BANNED_USERS_PATH: &str = "/helix/moderation/banned";
const BROADCASTER_ID_PARAM: &str = "broadcaster_id";
const FIRST_PARAM: &str = "first";
const AFTER_PARAM: &str = "after";
const MAX_PAGE_SIZE: u8 = 100;

#[derive(Deserialize)]
struct BannedUsersPage {
    #[serde(default)]
    data: Vec<BannedUserRow>,
    #[serde(default)]
    pagination: Pagination,
}

#[derive(Deserialize, Default)]
struct Pagination {
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct BannedUserRow {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    user_login: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    expires_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    moderator_login: String,
    #[serde(default)]
    moderator_name: String,
}

fn banned_users_request(broadcaster_id: &str, after: Option<&BanPageToken>) -> HelixRequest {
    let request = HelixRequest::new(HelixMethod::Get, BANNED_USERS_PATH)
        .query(BROADCASTER_ID_PARAM, broadcaster_id)
        .query(FIRST_PARAM, MAX_PAGE_SIZE.to_string());
    match after
        .map(BanPageToken::as_str)
        .filter(|raw| !raw.is_empty())
    {
        Some(cursor) => request.query(AFTER_PARAM, cursor),
        None => request,
    }
}

fn non_empty(raw: String) -> Option<String> {
    (!raw.is_empty()).then_some(raw)
}

fn parse_instant(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

fn duration_of(expires_at: &str) -> Option<BanDuration> {
    if expires_at.is_empty() {
        return Some(BanDuration::Permanent);
    }
    parse_instant(expires_at).map(BanDuration::Until)
}

fn entry_from_row(row: BannedUserRow) -> Option<BanEntry> {
    if row.user_id.is_empty() {
        return None;
    }
    let duration = duration_of(&row.expires_at)?;
    Some(BanEntry {
        name: non_empty(row.user_name)
            .or_else(|| non_empty(row.user_login))
            .unwrap_or_else(|| row.user_id.clone()),
        viewer_id: row.user_id,
        reason: non_empty(row.reason),
        moderator: non_empty(row.moderator_name).or_else(|| non_empty(row.moderator_login)),
        created_at: parse_instant(&row.created_at),
        duration,
        unban: UnbanAbility::Allowed,
    })
}

fn page_from_response(response: serde_json::Value) -> BanListOutcome {
    let Ok(page) = serde_json::from_value::<BannedUsersPage>(response) else {
        return BanListOutcome::Unavailable(BanListUnavailable::Transport);
    };
    BanListOutcome::Page(BanPage {
        entries: page.data.into_iter().filter_map(entry_from_row).collect(),
        next: page
            .pagination
            .cursor
            .and_then(non_empty)
            .map(BanPageToken::new),
    })
}

fn unavailability_of(error: &HelixError) -> BanListUnavailable {
    match error {
        HelixError::Credentials(_) => BanListUnavailable::NotConnected,
        HelixError::ReauthRequired => BanListUnavailable::MissingScope,
        HelixError::Http { status, .. } if *status == StatusCode::FORBIDDEN.as_u16() => {
            BanListUnavailable::MissingScope
        }
        HelixError::Http { .. } | HelixError::RateLimited | HelixError::Transport(_) => {
            BanListUnavailable::Transport
        }
    }
}

#[async_trait]
impl BanListSource for TwitchIntegrationBundle {
    async fn list_bans(&self, after: Option<&BanPageToken>) -> BanListOutcome {
        let broadcaster_id = self.broadcaster_id();
        if broadcaster_id.is_empty() {
            return BanListOutcome::Unavailable(BanListUnavailable::NotConnected);
        }
        match self
            .helix()
            .execute(banned_users_request(broadcaster_id, after))
            .await
        {
            Ok(response) => page_from_response(response),
            Err(error) => BanListOutcome::Unavailable(unavailability_of(&error)),
        }
    }
}
