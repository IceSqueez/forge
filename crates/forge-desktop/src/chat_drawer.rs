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

#[derive(Clone)]
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
        last_seen_label: relative_since(last.received_at),
        avatar_letter,
        avatar_color: viewer_hash_color(username, palette),
        watch_time: DASH.to_owned(),
        sub: sub_status(role),
        follow: DASH.to_owned(),
    })
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{BadgeKind, CATPPUCCIN_MOCHA, ChatBody, Platform};
    use forge_storage::{Viewer, ViewerPlatform};
    use time::{Duration, OffsetDateTime};

    use super::{
        DASH, SubStatus, drawer_matches, enrich_with_storage, relative_since, selected_summary,
        sub_status, synthesize_from_chat, unique_authors, watch_time_since,
    };
    use crate::chat_feed::ChatMessage;

    fn msg(username: &str, badges: Vec<BadgeKind>) -> ChatMessage {
        ChatMessage {
            id: "".into(),
            event_id: forge_types::EventId::new(),
            timestamp: "".into(),
            received_at: OffsetDateTime::now_utc(),
            platform: Platform::Twitch,
            badges,
            username: username.into(),
            author_color: None,
            body: ChatBody::Message("".into()),
            is_event: false,
            is_bot: false,
            moderated: false,
            reply: None,
        }
    }

    fn viewer(
        username: &str,
        message_count: u64,
        first_seen_at: OffsetDateTime,
        last_seen_at: OffsetDateTime,
    ) -> Viewer {
        Viewer {
            viewer_id: "id".into(),
            platform: ViewerPlatform::Twitch,
            username: username.into(),
            first_seen_at,
            last_seen_at,
            message_count,
            custom_greeting: false,
        }
    }

    #[test]
    fn drawer_matches_is_empty_or_case_insensitive_substring() {
        let cases = [
            ("Alice", "", true),
            ("Alice", "ali", true),
            ("Alice", "lic", true),
            ("Alice", "bob", false),
            ("Alice", "alicee", false),
        ];
        for (username, search, expected) in cases {
            assert_eq!(
                drawer_matches(username, search),
                expected,
                "username={username:?} search={search:?}"
            );
        }
    }

    #[test]
    fn unique_authors_dedups_keeping_newest_first() {
        let messages = [
            msg("alice", vec![]),
            msg("bob", vec![]),
            msg("alice", vec![]),
            msg("carol", vec![]),
        ];
        assert_eq!(unique_authors(&messages), vec!["carol", "alice", "bob"]);
    }

    #[test]
    fn unique_authors_drops_empty_usernames() {
        let messages = [msg("alice", vec![]), msg("", vec![]), msg("bob", vec![])];
        assert_eq!(unique_authors(&messages), vec!["bob", "alice"]);
    }

    #[test]
    fn synthesize_uses_latest_role_and_counts_only_that_author() {
        let messages = [
            msg("alice", vec![BadgeKind::Broadcaster]),
            msg("bob", vec![BadgeKind::Moderator]),
            msg("alice", vec![BadgeKind::Vip, BadgeKind::Subscriber]),
        ];
        let summary = synthesize_from_chat("alice", &messages, &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.role, Some(BadgeKind::Vip));
        assert_eq!(summary.avatar_letter, 'A');
    }

    #[test]
    fn synthesize_role_is_none_when_latest_row_has_no_badges() {
        let messages = [msg("alice", vec![])];
        let summary = synthesize_from_chat("alice", &messages, &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.role, None);
    }

    #[test]
    fn synthesize_returns_none_when_author_absent() {
        let messages = [msg("alice", vec![])];
        assert!(synthesize_from_chat("ghost", &messages, &CATPPUCCIN_MOCHA).is_none());
    }

    #[test]
    fn enrich_overlays_storage_fields_and_leaves_role_untouched() {
        let messages = [msg("alice", vec![BadgeKind::Subscriber])];
        let summary = synthesize_from_chat("alice", &messages, &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.watch_time, DASH);

        let now = OffsetDateTime::now_utc();
        let stored = viewer(
            "alice",
            99,
            now - Duration::minutes(120),
            now - Duration::days(2),
        );
        let enriched = enrich_with_storage(summary, &[stored]);

        assert_eq!(enriched.message_count, 99);
        assert_eq!(enriched.watch_time, "2h 0m");
        assert_eq!(enriched.last_seen_label, "2d");
        assert_eq!(enriched.role, Some(BadgeKind::Subscriber));
        assert!(enriched.sub == SubStatus::Subscribed);
    }

    #[test]
    fn enrich_leaves_synthesized_values_when_no_viewer_matches() {
        let messages = [msg("alice", vec![])];
        let summary = synthesize_from_chat("alice", &messages, &CATPPUCCIN_MOCHA).unwrap();
        let now = OffsetDateTime::now_utc();
        let other = viewer("someone-else", 99, now, now);
        let enriched = enrich_with_storage(summary, &[other]);

        assert_eq!(enriched.message_count, 1);
        assert_eq!(enriched.watch_time, DASH);
        assert_eq!(enriched.last_seen_label, "now");
    }

    #[test]
    fn sub_status_derives_from_role() {
        let cases = [
            (Some(BadgeKind::Broadcaster), SubStatus::Unlimited),
            (Some(BadgeKind::Subscriber), SubStatus::Subscribed),
            (Some(BadgeKind::Founder), SubStatus::Subscribed),
            (Some(BadgeKind::Moderator), SubStatus::None),
            (Some(BadgeKind::Vip), SubStatus::None),
            (None, SubStatus::None),
        ];
        for (role, expected) in cases {
            assert!(sub_status(role) == expected, "role={role:?}");
        }
    }

    #[test]
    fn selected_summary_falls_back_to_latest_author_when_none_selected() {
        let messages = [msg("alice", vec![]), msg("bob", vec![])];
        let summary = selected_summary(None, &messages, &[], &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.username, "bob");
    }

    #[test]
    fn selected_summary_fallback_skips_a_trailing_empty_author() {
        let messages = [msg("alice", vec![]), msg("", vec![])];
        let summary = selected_summary(None, &messages, &[], &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_uses_the_selected_author() {
        let messages = [msg("alice", vec![]), msg("bob", vec![])];
        let summary = selected_summary(Some("alice"), &messages, &[], &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_absent_selection_falls_back_to_latest_author() {
        let messages = [msg("alice", vec![])];
        let summary = selected_summary(Some("ghost"), &messages, &[], &CATPPUCCIN_MOCHA).unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_is_none_without_any_authored_message() {
        assert!(selected_summary(None, &[], &[], &CATPPUCCIN_MOCHA).is_none());
    }

    #[test]
    fn relative_since_labels_each_bucket() {
        let now = OffsetDateTime::now_utc();
        let cases = [
            (3, "now"),
            (30, "30s"),
            (120, "2 min"),
            (7200, "2h"),
            (2 * 86_400, "2d"),
        ];
        for (secs_ago, expected) in cases {
            assert_eq!(
                relative_since(now - Duration::seconds(secs_ago)),
                expected,
                "secs_ago={secs_ago}"
            );
        }
    }

    #[test]
    fn relative_since_clamps_future_timestamps_to_now() {
        let future = OffsetDateTime::now_utc() + Duration::seconds(120);
        assert_eq!(relative_since(future), "now");
    }

    #[test]
    fn watch_time_since_formats_minutes_and_hours() {
        let now = OffsetDateTime::now_utc();
        let cases = [(0, "0 min"), (30, "30 min"), (60, "1h 0m"), (150, "2h 30m")];
        for (mins_ago, expected) in cases {
            assert_eq!(
                watch_time_since(now - Duration::minutes(mins_ago)),
                expected,
                "mins_ago={mins_ago}"
            );
        }
    }
}
