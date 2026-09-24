use std::collections::HashMap;

use forge_components::{BadgeKind, ForgePalette, fmt_relative_time, hash_accent};
use forge_storage::Viewer;
use gpui::Rgba;
use time::OffsetDateTime;

use crate::chat_feed::{AuthorActivity, AuthorIndex};

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

fn sub_status(role: Option<BadgeKind>) -> SubStatus {
    match role {
        Some(BadgeKind::Broadcaster) => SubStatus::Unlimited,
        Some(BadgeKind::Subscriber | BadgeKind::Founder) => SubStatus::Subscribed,
        _ => SubStatus::None,
    }
}

#[derive(Default)]
pub(crate) struct ViewerDirectory {
    viewers: Vec<Viewer>,
    by_name: HashMap<String, usize>,
}

impl ViewerDirectory {
    /// On a username shared across platforms the first listed viewer wins.
    pub fn new(viewers: Vec<Viewer>) -> Self {
        let mut by_name = HashMap::with_capacity(viewers.len());
        for (ix, viewer) in viewers.iter().enumerate() {
            by_name.entry(viewer.username.clone()).or_insert(ix);
        }
        Self { viewers, by_name }
    }

    pub fn viewers(&self) -> &[Viewer] {
        &self.viewers
    }

    pub fn get(&self, username: &str) -> Option<&Viewer> {
        self.by_name
            .get(username)
            .and_then(|ix| self.viewers.get(*ix))
    }
}

pub(crate) fn summary_from_activity(
    username: &str,
    activity: &AuthorActivity,
    palette: &ForgePalette,
) -> ViewerSummary {
    let avatar_letter = username
        .chars()
        .next()
        .map_or('?', |c| c.to_ascii_uppercase());
    ViewerSummary {
        username: username.to_owned(),
        role: activity.role,
        message_count: activity.message_count as u64,
        last_seen_label: fmt_relative_time(Some(activity.last_received_at)),
        avatar_letter,
        avatar_color: hash_accent(username, palette),
        watch_time: DASH.to_owned(),
        sub: sub_status(activity.role),
        follow: DASH.to_owned(),
    }
}

pub(crate) fn enrich_with_storage(
    mut summary: ViewerSummary,
    viewer: Option<&Viewer>,
) -> ViewerSummary {
    if let Some(v) = viewer {
        summary.message_count = v.message_count;
        summary.last_seen_label = fmt_relative_time(Some(v.last_seen_at));
        summary.watch_time = watch_time_since(v.first_seen_at);
    }
    summary
}

pub(crate) fn author_summary(
    username: &str,
    authors: &AuthorIndex,
    directory: &ViewerDirectory,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let activity = authors.get(username)?;
    Some(enrich_with_storage(
        summary_from_activity(username, activity, palette),
        directory.get(username),
    ))
}

pub(crate) fn selected_summary(
    selected: Option<&str>,
    authors: &AuthorIndex,
    directory: &ViewerDirectory,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    selected
        .and_then(|sel| author_summary(sel, authors, directory, palette))
        .or_else(|| {
            let newest = authors.newest()?;
            author_summary(newest, authors, directory, palette)
        })
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
    use forge_components::{BadgeKind, ChatBody, FORGE_DEFAULT, Platform};
    use forge_storage::{Viewer, ViewerPlatform};
    use time::{Duration, OffsetDateTime};

    use super::{
        DASH, SubStatus, ViewerDirectory, author_summary, drawer_matches, enrich_with_storage,
        selected_summary, sub_status, watch_time_since,
    };
    use crate::chat_feed::{ChatFeed, ChatMessage};

    fn feed_of(messages: &[ChatMessage]) -> ChatFeed {
        let mut feed = ChatFeed::new();
        feed.seed(messages.to_vec());
        feed
    }

    fn unique_authors(messages: &[ChatMessage]) -> Vec<String> {
        feed_of(messages)
            .authors()
            .newest_first()
            .map(ToString::to_string)
            .collect()
    }

    fn synthesize_from_chat(
        username: &str,
        messages: &[ChatMessage],
    ) -> Option<super::ViewerSummary> {
        author_summary(
            username,
            feed_of(messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
    }

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
        let summary = synthesize_from_chat("alice", &messages).unwrap();
        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.role, Some(BadgeKind::Vip));
        assert_eq!(summary.avatar_letter, 'A');
    }

    #[test]
    fn synthesize_role_is_none_when_latest_row_has_no_badges() {
        let messages = [msg("alice", vec![])];
        let summary = synthesize_from_chat("alice", &messages).unwrap();
        assert_eq!(summary.role, None);
    }

    #[test]
    fn synthesize_returns_none_when_author_absent() {
        let messages = [msg("alice", vec![])];
        assert!(synthesize_from_chat("ghost", &messages).is_none());
    }

    #[test]
    fn enrich_overlays_storage_fields_and_leaves_role_untouched() {
        let messages = [msg("alice", vec![BadgeKind::Subscriber])];
        let summary = synthesize_from_chat("alice", &messages).unwrap();
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.watch_time, DASH);

        let now = OffsetDateTime::now_utc();
        let stored = viewer(
            "alice",
            99,
            now - Duration::minutes(120),
            now - Duration::days(2),
        );
        let enriched = enrich_with_storage(summary, Some(&stored));

        assert_eq!(enriched.message_count, 99);
        assert_eq!(enriched.watch_time, "2h 0m");
        assert_eq!(enriched.last_seen_label, "fmt_relative_days");
        assert_eq!(enriched.role, Some(BadgeKind::Subscriber));
        assert!(enriched.sub == SubStatus::Subscribed);
    }

    #[test]
    fn enrich_leaves_synthesized_values_when_no_viewer_matches() {
        let messages = [msg("alice", vec![])];
        let summary = synthesize_from_chat("alice", &messages).unwrap();
        let now = OffsetDateTime::now_utc();
        let other = viewer("someone-else", 99, now, now);
        let enriched = enrich_with_storage(summary, ViewerDirectory::new(vec![other]).get("alice"));

        assert_eq!(enriched.message_count, 1);
        assert_eq!(enriched.watch_time, DASH);
        assert_eq!(enriched.last_seen_label, "fmt_relative_seconds");
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
        let summary = selected_summary(
            None,
            feed_of(&messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "bob");
    }

    #[test]
    fn selected_summary_fallback_skips_a_trailing_empty_author() {
        let messages = [msg("alice", vec![]), msg("", vec![])];
        let summary = selected_summary(
            None,
            feed_of(&messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_uses_the_selected_author() {
        let messages = [msg("alice", vec![]), msg("bob", vec![])];
        let summary = selected_summary(
            Some("alice"),
            feed_of(&messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_absent_selection_falls_back_to_latest_author() {
        let messages = [msg("alice", vec![])];
        let summary = selected_summary(
            Some("ghost"),
            feed_of(&messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn selected_summary_is_none_without_any_authored_message() {
        assert!(
            selected_summary(
                None,
                ChatFeed::new().authors(),
                &ViewerDirectory::default(),
                &FORGE_DEFAULT
            )
            .is_none()
        );
    }

    #[test]
    fn viewer_directory_resolves_a_shared_username_to_the_first_listed_viewer() {
        let now = OffsetDateTime::now_utc();
        let directory = ViewerDirectory::new(vec![
            viewer("alice", 7, now, now),
            viewer("alice", 99, now, now),
        ]);

        assert_eq!(directory.get("alice").unwrap().message_count, 7);
        assert!(directory.get("bob").is_none());
    }

    #[test]
    fn summaries_count_only_the_rows_still_retained_after_eviction() {
        let mut feed = ChatFeed::new();
        feed.set_capacity(3);
        for name in ["alice", "alice", "bob", "alice", "carol"] {
            feed.push(msg(name, vec![]));
        }

        let summary = |name: &str| {
            author_summary(
                name,
                feed.authors(),
                &ViewerDirectory::default(),
                &FORGE_DEFAULT,
            )
        };
        assert_eq!(summary("alice").unwrap().message_count, 1);
        assert_eq!(summary("bob").unwrap().message_count, 1);
        assert!(summary("ghost").is_none());
    }

    #[test]
    fn selected_summary_falls_back_to_the_newest_author_once_the_selection_is_evicted() {
        let mut feed = ChatFeed::new();
        feed.set_capacity(2);
        for name in ["alice", "bob", "carol"] {
            feed.push(msg(name, vec![]));
        }

        let summary = selected_summary(
            Some("alice"),
            feed.authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "carol");
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
