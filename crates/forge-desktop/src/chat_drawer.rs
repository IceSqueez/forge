use std::collections::HashSet;

use forge_components::{BadgeKind, ForgePalette};
use forge_storage::Viewer;
use gpui::Rgba;
use time::OffsetDateTime;

use crate::chat_feed::ChatMessage;

pub(crate) const DASH: &str = "-";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubStatus {
    Unlimited,
    Subscribed,
    None,
}

pub(crate) struct ViewerSummary {
    pub username: String,
    pub role: Option<BadgeKind>,
    pub message_count: u64,
    pub last_seen_label: String,
    pub avatar_letter: char,
    pub avatar_color: Rgba,
    pub watch_time: String,
    pub sub: SubStatus,
    pub follow: String,
}

pub(crate) fn drawer_matches(username: &str, search: &str) -> bool {
    search.is_empty() || username.to_ascii_lowercase().contains(search)
}

/// Distinct non-empty chat authors, most-recent-first.
pub(crate) fn unique_authors(messages: &[ChatMessage]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    messages
        .iter()
        .rev()
        .filter_map(|m| {
            let name = m.username.as_ref();
            if name.is_empty() || !seen.insert(name.to_owned()) {
                None
            } else {
                Some(name.to_owned())
            }
        })
        .collect()
}

fn sub_status(role: Option<BadgeKind>) -> SubStatus {
    match role {
        Some(BadgeKind::Broadcaster) => SubStatus::Unlimited,
        Some(BadgeKind::Subscriber | BadgeKind::Founder) => SubStatus::Subscribed,
        _ => SubStatus::None,
    }
}

pub(crate) fn synthesize_from_chat(
    username: &str,
    messages: &[ChatMessage],
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let count = messages
        .iter()
        .filter(|m| m.username.as_ref() == username)
        .count();
    if count == 0 {
        return None;
    }
    let last = messages
        .iter()
        .rev()
        .find(|m| m.username.as_ref() == username)?;
    let role = last.badges.first().copied();
    let avatar_letter = username
        .chars()
        .next()
        .map_or('?', |c| c.to_ascii_uppercase());

    Some(ViewerSummary {
        username: username.to_owned(),
        role,
        message_count: count as u64,
        last_seen_label: "now".to_owned(),
        avatar_letter,
        avatar_color: viewer_hash_color(username, palette),
        watch_time: DASH.to_owned(),
        sub: sub_status(role),
        follow: DASH.to_owned(),
    })
}

/// Overlays the persisted `message_count`, real `last_seen`, and a `watch_time`
/// derived from `first_seen_at`; leaves chat-derived role/sub untouched.
pub(crate) fn enrich_with_storage(mut summary: ViewerSummary, viewers: &[Viewer]) -> ViewerSummary {
    if let Some(v) = viewers.iter().find(|v| v.username == summary.username) {
        summary.message_count = v.message_count;
        summary.last_seen_label = relative_since(v.last_seen_at);
        summary.watch_time = watch_time_since(v.first_seen_at);
    }
    summary
}

pub(crate) fn selected_summary(
    selected: Option<&str>,
    messages: &[ChatMessage],
    viewers: &[Viewer],
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    if let Some(sel) = selected
        && let Some(summary) = synthesize_from_chat(sel, messages, palette)
    {
        return Some(enrich_with_storage(summary, viewers));
    }
    let last = messages.iter().rev().find(|m| !m.username.is_empty())?;
    synthesize_from_chat(last.username.as_ref(), messages, palette)
        .map(|s| enrich_with_storage(s, viewers))
}

pub(crate) fn viewer_hash_color(username: &str, palette: &ForgePalette) -> Rgba {
    let idx = username
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(u32::from(b))) as usize
        % 8;
    [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
        palette.accent_pink_light,
        palette.accent_teal,
    ][idx]
}

pub(crate) fn relative_since(dt: OffsetDateTime) -> String {
    let secs = (OffsetDateTime::now_utc() - dt).whole_seconds().max(0);
    if secs < 5 {
        "now".to_owned()
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{} min", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

fn watch_time_since(first_seen: OffsetDateTime) -> String {
    let mins = (OffsetDateTime::now_utc() - first_seen)
        .whole_minutes()
        .max(0);
    if mins >= 60 {
        format!("{}h {}m", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}
