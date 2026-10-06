use std::collections::HashMap;

use forge_components::{BadgeKind, ForgePalette, fmt_relative_time, hash_accent};
use forge_storage::Viewer;
use gpui::{Rgba, SharedString};

use crate::chat_author::{AuthorKey, viewer_platform};
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
    pub key: AuthorKey,
    pub username: String,
    pub role: Option<BadgeKind>,
    pub message_count: u64,
    pub last_seen_label: String,
    pub avatar_letter: char,
    pub avatar_color: Rgba,
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
    by_key: HashMap<AuthorKey, usize>,
}

impl ViewerDirectory {
    pub fn new(viewers: Vec<Viewer>) -> Self {
        let mut by_key = HashMap::with_capacity(viewers.len());
        for (ix, viewer) in viewers.iter().enumerate() {
            let platform = viewer_platform(&viewer.platform);
            by_key
                .entry(AuthorKey::by_viewer_id(platform, &viewer.viewer_id))
                .or_insert(ix);
            by_key
                .entry(AuthorKey::by_name(platform, &viewer.username))
                .or_insert(ix);
        }
        Self { viewers, by_key }
    }

    pub fn viewers(&self) -> &[Viewer] {
        &self.viewers
    }

    pub fn get(&self, key: &AuthorKey) -> Option<&Viewer> {
        self.by_key.get(key).and_then(|ix| self.viewers.get(*ix))
    }
}

pub(crate) fn summary_from_activity(
    key: &AuthorKey,
    activity: &AuthorActivity,
    palette: &ForgePalette,
) -> ViewerSummary {
    let username = activity.name.as_ref();
    let avatar_letter = username
        .chars()
        .next()
        .map_or('?', |c| c.to_ascii_uppercase());
    ViewerSummary {
        key: key.clone(),
        username: username.to_owned(),
        role: activity.role,
        message_count: activity.message_count as u64,
        last_seen_label: fmt_relative_time(Some(activity.last_received_at)),
        avatar_letter,
        avatar_color: hash_accent(username, palette),
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
    }
    summary
}

pub(crate) fn author_summary(
    key: &AuthorKey,
    authors: &AuthorIndex,
    directory: &ViewerDirectory,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let activity = authors.get(key)?;
    Some(enrich_with_storage(
        summary_from_activity(key, activity, palette),
        directory.get(key),
    ))
}

pub(crate) fn selected_summary(
    selected: Option<&AuthorKey>,
    authors: &AuthorIndex,
    directory: &ViewerDirectory,
    palette: &ForgePalette,
) -> Option<ViewerSummary> {
    let key = displayed_viewer(selected, authors)?;
    author_summary(&key, authors, directory, palette)
}

pub(crate) fn displayed_viewer(
    selected: Option<&AuthorKey>,
    authors: &AuthorIndex,
) -> Option<AuthorKey> {
    selected
        .filter(|key| authors.get(key).is_some())
        .or_else(|| authors.newest().map(|(newest, _)| newest))
        .cloned()
}

pub(crate) fn current_name(
    key: &AuthorKey,
    authors: &AuthorIndex,
    directory: &ViewerDirectory,
) -> Option<SharedString> {
    authors
        .get(key)
        .map(|activity| activity.name.clone())
        .or_else(|| {
            directory
                .get(key)
                .map(|viewer| SharedString::from(viewer.username.clone()))
        })
        .or_else(|| key.fallback_name().cloned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{BadgeKind, ChatBody, FORGE_DEFAULT, Platform};
    use forge_storage::{Viewer, ViewerPlatform};
    use time::{Duration, OffsetDateTime};

    use super::{
        SubStatus, ViewerDirectory, author_summary, current_name, displayed_viewer, drawer_matches,
        enrich_with_storage, selected_summary, sub_status,
    };
    use crate::chat_author::AuthorKey;
    use crate::chat_feed::{ChatFeed, ChatMessage};

    fn key(name: &str) -> AuthorKey {
        AuthorKey::by_name(Platform::Twitch, name)
    }

    fn feed_of(messages: &[ChatMessage]) -> ChatFeed {
        let mut feed = ChatFeed::new();
        feed.seed(messages.to_vec());
        feed
    }

    fn synthesize_from_chat(
        username: &str,
        messages: &[ChatMessage],
    ) -> Option<super::ViewerSummary> {
        author_summary(
            &key(username),
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
            author_id: None,
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

        let now = OffsetDateTime::now_utc();
        let stored = viewer(
            "alice",
            99,
            now - Duration::minutes(120),
            now - Duration::days(2),
        );
        let enriched = enrich_with_storage(summary, Some(&stored));

        assert_eq!(enriched.message_count, 99);
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
        let enriched = enrich_with_storage(
            summary,
            ViewerDirectory::new(vec![other]).get(&key("alice")),
        );

        assert_eq!(enriched.message_count, 1);
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
    fn the_card_shows_the_selected_viewer_while_in_the_feed_else_the_newest_author() {
        let alice_then_bob = [msg("alice", vec![]), msg("bob", vec![])];
        for (selected, messages, expected) in [
            (Some(key("alice")), &alice_then_bob[..], Some(key("alice"))),
            (Some(key("ghost")), &alice_then_bob[..], Some(key("bob"))),
            (
                Some(AuthorKey::by_name(Platform::Kick, "alice")),
                &alice_then_bob[..],
                Some(key("bob")),
            ),
            (None, &alice_then_bob[..], Some(key("bob"))),
            (Some(key("alice")), &[][..], None),
            (None, &[][..], None),
        ] {
            assert_eq!(
                displayed_viewer(selected.as_ref(), feed_of(messages).authors()),
                expected,
                "selected {selected:?}"
            );
        }
    }

    #[test]
    fn selected_summary_uses_the_selected_author() {
        let messages = [msg("alice", vec![]), msg("bob", vec![])];
        let summary = selected_summary(
            Some(&key("alice")),
            feed_of(&messages).authors(),
            &ViewerDirectory::default(),
            &FORGE_DEFAULT,
        )
        .unwrap();
        assert_eq!(summary.username, "alice");
    }

    #[test]
    fn viewer_directory_resolves_a_shared_username_to_the_first_listed_viewer() {
        let now = OffsetDateTime::now_utc();
        let directory = ViewerDirectory::new(vec![
            viewer("alice", 7, now, now),
            viewer("alice", 99, now, now),
        ]);

        assert_eq!(directory.get(&key("alice")).unwrap().message_count, 7);
        assert!(directory.get(&key("bob")).is_none());
    }

    fn stored(platform: ViewerPlatform, viewer_id: &str, username: &str, count: u64) -> Viewer {
        Viewer {
            viewer_id: viewer_id.into(),
            platform,
            ..viewer(
                username,
                count,
                OffsetDateTime::now_utc(),
                OffsetDateTime::now_utc(),
            )
        }
    }

    fn spoken(platform: Platform, viewer_id: &str, username: &str) -> ChatMessage {
        ChatMessage {
            platform,
            author_id: Some(viewer_id.to_owned().into()),
            ..msg(username, vec![])
        }
    }

    #[test]
    fn viewer_directory_finds_a_viewer_by_platform_and_id_or_platform_and_username_only() {
        let directory = ViewerDirectory::new(vec![
            stored(ViewerPlatform::Twitch, "t1", "alice", 1),
            stored(ViewerPlatform::Kick, "k1", "bob", 2),
        ]);
        for (key, expected) in [
            (AuthorKey::by_viewer_id(Platform::Twitch, "t1"), Some(1)),
            (AuthorKey::by_name(Platform::Twitch, "alice"), Some(1)),
            (AuthorKey::by_viewer_id(Platform::Kick, "k1"), Some(2)),
            (AuthorKey::by_name(Platform::Kick, "bob"), Some(2)),
            (AuthorKey::by_viewer_id(Platform::Kick, "t1"), None),
            (AuthorKey::by_name(Platform::YouTube, "alice"), None),
            (AuthorKey::by_viewer_id(Platform::Twitch, "alice"), None),
            (AuthorKey::by_name(Platform::Twitch, "t1"), None),
        ] {
            assert_eq!(
                directory.get(&key).map(|viewer| viewer.message_count),
                expected,
                "{key:?}"
            );
        }
    }

    #[test]
    fn a_renamed_viewer_card_shows_the_chat_name_with_the_stored_count_found_by_id() {
        let feed = feed_of(&[spoken(Platform::Twitch, "t1", "alice_new")]);
        let directory =
            ViewerDirectory::new(vec![stored(ViewerPlatform::Twitch, "t1", "alice_old", 99)]);

        let summary = author_summary(
            &AuthorKey::by_viewer_id(Platform::Twitch, "t1"),
            feed.authors(),
            &directory,
            &FORGE_DEFAULT,
        )
        .unwrap();

        assert_eq!(
            (summary.username.as_str(), summary.message_count),
            ("alice_new", 99)
        );
    }

    #[test]
    fn current_name_prefers_the_chat_name_then_the_stored_name_then_the_name_key() {
        let feed = feed_of(&[spoken(Platform::Twitch, "t1", "alice_new")]);
        let directory = ViewerDirectory::new(vec![
            stored(ViewerPlatform::Twitch, "t1", "alice_old", 1),
            stored(ViewerPlatform::Twitch, "t2", "bob", 1),
        ]);
        for (key, expected) in [
            (
                AuthorKey::by_viewer_id(Platform::Twitch, "t1"),
                Some("alice_new"),
            ),
            (AuthorKey::by_viewer_id(Platform::Twitch, "t2"), Some("bob")),
            (AuthorKey::by_name(Platform::Twitch, "carol"), Some("carol")),
            (AuthorKey::by_viewer_id(Platform::Twitch, "t3"), None),
        ] {
            assert_eq!(
                current_name(&key, feed.authors(), &directory).as_deref(),
                expected,
                "{key:?}"
            );
        }
    }
}
