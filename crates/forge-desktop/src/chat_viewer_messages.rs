use std::collections::HashMap;

use forge_components::Platform;
use forge_storage::{ChatAuthorKey, ChatAuthorTally};
use forge_types::ChatSource;
use time::Duration;

use crate::chat_author::{AuthorHandle, AuthorKey};
use crate::chat_feed::{ChatFeed, ChatMessage};

pub(crate) fn stored_author(key: &AuthorKey) -> Option<ChatAuthorKey> {
    match &key.handle {
        AuthorHandle::ViewerId(id) => Some(ChatAuthorKey {
            source: chat_source(key.platform),
            author_id: id.to_string(),
        }),
        AuthorHandle::Name(_) => None,
    }
}

fn chat_source(platform: Platform) -> ChatSource {
    match platform {
        Platform::Twitch => ChatSource::Twitch,
        Platform::YouTube => ChatSource::YouTube,
        Platform::Kick => ChatSource::Kick,
    }
}

fn newer_than_stored(tally: Option<&ChatAuthorTally>, message: &ChatMessage) -> bool {
    match tally.and_then(|tally| tally.newest_at) {
        None => true,
        Some(newest) => newest
            .checked_add(Duration::MILLISECOND)
            .is_some_and(|first_unsaved| message.received_at >= first_unsaved),
    }
}

#[derive(Default)]
pub(crate) struct ViewerMessages {
    stored: HashMap<AuthorKey, ChatAuthorTally>,
    unsaved: HashMap<AuthorKey, Vec<ChatMessage>>,
    scanned_through: u64,
    held: Option<AuthorKey>,
}

impl ViewerMessages {
    pub fn count(&self, key: &AuthorKey) -> u64 {
        let stored = self.stored.get(key).map_or(0, |tally| tally.messages);
        stored.saturating_add(self.unsaved(key).len() as u64)
    }

    pub fn stored_tally(&self, key: &AuthorKey) -> Option<ChatAuthorTally> {
        self.stored.get(key).copied()
    }

    pub fn unsaved(&self, key: &AuthorKey) -> &[ChatMessage] {
        self.unsaved.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn absorb(&mut self, feed: &ChatFeed) -> bool {
        let from = self.scanned_through.max(feed.start_seq());
        self.scanned_through = feed.end_seq();
        let mut changed = false;
        for message in (from..feed.end_seq()).filter_map(|seq| feed.get(seq)) {
            changed |= self.note(message, None);
        }
        changed
    }

    pub fn replace_tallies(
        &mut self,
        tallies: impl IntoIterator<Item = (AuthorKey, ChatAuthorTally)>,
        feed: &ChatFeed,
    ) {
        let mut stored: HashMap<AuthorKey, ChatAuthorTally> = tallies.into_iter().collect();
        let mut unsaved = HashMap::new();
        if let Some(held) = &self.held {
            match self.stored.remove(held) {
                Some(tally) => stored.insert(held.clone(), tally),
                None => stored.remove(held),
            };
            if let Some(rows) = self.unsaved.remove(held) {
                unsaved.insert(held.clone(), rows);
            }
        }
        self.stored = stored;
        self.unsaved = unsaved;
        let held = self.held.clone();
        for message in (feed.start_seq()..feed.end_seq()).filter_map(|seq| feed.get(seq)) {
            self.note(message, held.as_ref());
        }
        self.scanned_through = feed.end_seq();
    }

    pub fn adopt(&mut self, key: &AuthorKey, tally: ChatAuthorTally, feed: &ChatFeed) {
        self.unsaved.remove(key);
        self.stored.insert(key.clone(), tally);
        for message in (feed.start_seq()..feed.end_seq()).filter_map(|seq| feed.get(seq)) {
            if message.author_key().as_ref() == Some(key) {
                self.note(message, None);
            }
        }
    }

    pub fn hold(&mut self, key: AuthorKey) {
        self.held = Some(key);
    }

    pub fn release(&mut self) {
        self.held = None;
    }

    fn note(&mut self, message: &ChatMessage, skip: Option<&AuthorKey>) -> bool {
        if message.is_event {
            return false;
        }
        let Some(key) = message.author_key() else {
            return false;
        };
        if skip == Some(&key) || !newer_than_stored(self.stored.get(&key), message) {
            return false;
        }
        let rows = self.unsaved.entry(key).or_default();
        if !message.id.is_empty() && rows.iter().any(|row| row.id == message.id) {
            return false;
        }
        rows.push(message.clone());
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_components::{ChatBody, Platform};
    use forge_storage::{ChatAuthorKey, ChatAuthorTally};
    use forge_types::{ChatSource, EventId};
    use time::{Duration, OffsetDateTime};

    use super::{ViewerMessages, stored_author};
    use crate::chat_author::AuthorKey;
    use crate::chat_feed::{ChatFeed, ChatMessage};

    type Author = (Platform, Option<&'static str>, &'static str);

    const ANN: Author = (Platform::Twitch, Some("42"), "ann");
    const BOB: Author = (Platform::Twitch, Some("7"), "bob");
    const ANN_BY_NAME: Author = (Platform::Kick, None, "ann");

    fn base() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn secs(at: i64) -> OffsetDateTime {
        base() + Duration::seconds(at)
    }

    fn key_of((platform, id, name): Author) -> AuthorKey {
        match id {
            Some(id) => AuthorKey::by_viewer_id(platform, id),
            None => AuthorKey::by_name(platform, name),
        }
    }

    fn line_at(id: &str, at: OffsetDateTime, (platform, author_id, name): Author) -> ChatMessage {
        ChatMessage {
            id: id.to_owned().into(),
            event_id: EventId::new(),
            timestamp: "00:00:00".into(),
            received_at: at,
            platform,
            badges: vec![],
            username: name.into(),
            author_id: author_id.map(Into::into),
            author_color: None,
            body: ChatBody::Message("hi".into()),
            is_event: false,
            is_bot: false,
            moderated: false,
            reply: None,
        }
    }

    fn line(id: &str, at: i64, author: Author) -> ChatMessage {
        line_at(id, secs(at), author)
    }

    fn tally(messages: u64, newest: Option<i64>) -> ChatAuthorTally {
        ChatAuthorTally {
            messages,
            newest_at: newest.map(secs),
        }
    }

    fn feed_of(messages: Vec<ChatMessage>) -> ChatFeed {
        let mut feed = ChatFeed::new();
        for message in messages {
            feed.push(message);
        }
        feed
    }

    fn absorbed(feed: &ChatFeed) -> ViewerMessages {
        let mut ledger = ViewerMessages::default();
        ledger.absorb(feed);
        ledger
    }

    fn refresh(
        ledger: &mut ViewerMessages,
        feed: &ChatFeed,
        tallies: Vec<(Author, ChatAuthorTally)>,
    ) {
        ledger.replace_tallies(
            tallies
                .into_iter()
                .map(|(author, tally)| (key_of(author), tally)),
            feed,
        );
    }

    fn unsaved_ids(ledger: &ViewerMessages, author: Author) -> Vec<String> {
        ledger
            .unsaved(&key_of(author))
            .iter()
            .map(|message| message.id.to_string())
            .collect()
    }

    #[test]
    fn stored_author_asks_by_viewer_id_on_the_viewers_platform_and_skips_name_only_keys() {
        let cases = [
            (
                AuthorKey::by_viewer_id(Platform::Twitch, "42"),
                Some((ChatSource::Twitch, "42")),
            ),
            (
                AuthorKey::by_viewer_id(Platform::YouTube, "UCx"),
                Some((ChatSource::YouTube, "UCx")),
            ),
            (
                AuthorKey::by_viewer_id(Platform::Kick, "9"),
                Some((ChatSource::Kick, "9")),
            ),
            (AuthorKey::by_name(Platform::Kick, "ann"), None),
        ];
        for (key, expected) in cases {
            let expected = expected.map(|(source, id)| ChatAuthorKey {
                source,
                author_id: id.to_owned(),
            });
            assert_eq!(stored_author(&key), expected, "{key:?}");
        }
    }

    #[test]
    fn count_adds_only_feed_lines_newer_than_the_newest_stored_row_to_the_stored_tally() {
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            line("m2", 20, ANN),
            line("m3", 30, ANN),
            line("m4", 40, ANN),
        ]);
        let mut ledger = absorbed(&feed);

        refresh(&mut ledger, &feed, vec![(ANN, tally(5, Some(20)))]);

        assert_eq!(
            (ledger.count(&key_of(ANN)), unsaved_ids(&ledger, ANN)),
            (7, vec!["m3".to_owned(), "m4".to_owned()])
        );
    }

    #[test]
    fn a_feed_line_is_unsaved_only_from_one_millisecond_after_the_newest_stored_row() {
        let newest = Some(0);
        for (offset_nanos, expected) in [
            (-1_000_000, 1),
            (0, 1),
            (999_999, 1),
            (1_000_000, 2),
            (1_000_001, 2),
        ] {
            let at = base() + Duration::nanoseconds(offset_nanos);
            let feed = feed_of(vec![line_at("m1", at, ANN)]);
            let mut ledger = absorbed(&feed);

            refresh(&mut ledger, &feed, vec![(ANN, tally(1, newest))]);

            assert_eq!(
                ledger.count(&key_of(ANN)),
                expected,
                "{offset_nanos} ns after the newest stored row"
            );
        }
    }

    #[test]
    fn a_viewer_without_stored_rows_counts_every_feed_line() {
        let feed = feed_of(vec![line("m1", 10, ANN), line("m2", 20, ANN)]);
        for (name, tallies) in [
            ("no tally", vec![]),
            ("empty tally", vec![(ANN, tally(0, None))]),
        ] {
            let mut ledger = absorbed(&feed);

            refresh(&mut ledger, &feed, tallies);

            assert_eq!(ledger.count(&key_of(ANN)), 2, "{name}");
        }
    }

    #[test]
    fn count_skips_events_other_viewers_and_the_same_id_on_another_platform() {
        let mut event = line("e1", 20, ANN);
        event.is_event = true;
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            event,
            line("b1", 30, BOB),
            line("y1", 40, (Platform::YouTube, Some("42"), "ann")),
        ]);

        let ledger = absorbed(&feed);

        assert_eq!(ledger.count(&key_of(ANN)), 1);
    }

    #[test]
    fn a_repeated_message_id_counts_once_but_lines_without_an_id_always_count() {
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            line("m1", 10, ANN),
            line("", 20, ANN),
            line("", 20, ANN),
        ]);

        let ledger = absorbed(&feed);

        assert_eq!(ledger.count(&key_of(ANN)), 3);
    }

    #[test]
    fn a_viewer_known_only_by_name_keeps_counting_feed_lines_through_a_refresh() {
        let feed = feed_of(vec![
            line("n1", 10, ANN_BY_NAME),
            line("n2", 20, ANN_BY_NAME),
        ]);
        let mut ledger = absorbed(&feed);

        refresh(&mut ledger, &feed, vec![(ANN, tally(9, Some(30)))]);

        assert_eq!(ledger.count(&key_of(ANN_BY_NAME)), 2);
    }

    #[test]
    fn absorb_counts_each_feed_line_once_and_reports_only_real_changes() {
        let mut feed = feed_of(vec![line("m1", 10, ANN)]);
        let mut ledger = ViewerMessages::default();
        let mut steps = Vec::new();

        steps.push((ledger.absorb(&feed), ledger.count(&key_of(ANN))));
        steps.push((ledger.absorb(&feed), ledger.count(&key_of(ANN))));
        feed.push(line("b1", 20, BOB));
        steps.push((ledger.absorb(&feed), ledger.count(&key_of(ANN))));
        feed.push(line("m2", 30, ANN));
        steps.push((ledger.absorb(&feed), ledger.count(&key_of(ANN))));

        assert_eq!(steps, [(true, 1), (false, 1), (true, 1), (true, 2)]);
    }

    #[test]
    fn a_line_evicted_from_the_feed_stays_counted() {
        let mut feed = ChatFeed::new();
        feed.set_capacity(2);
        let mut ledger = ViewerMessages::default();

        for (ix, at) in [10, 20, 30].into_iter().enumerate() {
            feed.push(line(&format!("m{ix}"), at, ANN));
            ledger.absorb(&feed);
        }

        assert_eq!(ledger.count(&key_of(ANN)), 3);
    }

    #[test]
    fn a_refresh_after_unsaved_lines_are_flushed_does_not_count_them_twice() {
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            line("m2", 20, ANN),
            line("m3", 30, ANN),
            line("m4", 40, ANN),
        ]);
        let mut ledger = absorbed(&feed);
        refresh(&mut ledger, &feed, vec![(ANN, tally(2, Some(20)))]);
        let before_flush = ledger.count(&key_of(ANN));

        refresh(&mut ledger, &feed, vec![(ANN, tally(4, Some(40)))]);

        assert_eq!((before_flush, ledger.count(&key_of(ANN))), (4, 4));
    }

    #[test]
    fn a_retention_prune_lowers_the_count_to_what_storage_still_holds() {
        let feed = feed_of(vec![line("m1", 10, ANN), line("m2", 20, ANN)]);
        let mut ledger = absorbed(&feed);
        refresh(&mut ledger, &feed, vec![(ANN, tally(250, Some(20)))]);

        refresh(&mut ledger, &feed, vec![(ANN, tally(100, Some(20)))]);

        assert_eq!(ledger.count(&key_of(ANN)), 100);
    }

    #[test]
    fn a_held_viewer_keeps_its_count_through_a_refresh_while_others_follow_storage() {
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            line("m2", 20, ANN),
            line("b1", 10, BOB),
        ]);
        let mut ledger = absorbed(&feed);
        refresh(
            &mut ledger,
            &feed,
            vec![(ANN, tally(5, Some(10))), (BOB, tally(3, Some(10)))],
        );
        ledger.hold(key_of(ANN));

        refresh(
            &mut ledger,
            &feed,
            vec![(ANN, tally(9, Some(20))), (BOB, tally(8, Some(10)))],
        );

        assert_eq!(
            (ledger.count(&key_of(ANN)), ledger.count(&key_of(BOB))),
            (6, 8)
        );
    }

    #[test]
    fn a_held_viewer_without_a_stored_tally_stays_feed_only_through_a_refresh() {
        let feed = feed_of(vec![line("m1", 10, ANN), line("m2", 20, ANN)]);
        let mut ledger = absorbed(&feed);
        ledger.hold(key_of(ANN));

        refresh(&mut ledger, &feed, vec![(ANN, tally(9, Some(20)))]);

        assert_eq!(ledger.count(&key_of(ANN)), 2);
    }

    #[test]
    fn a_held_viewer_still_counts_lines_sent_while_held() {
        let mut feed = feed_of(vec![line("m1", 10, ANN)]);
        let mut ledger = absorbed(&feed);
        refresh(&mut ledger, &feed, vec![(ANN, tally(5, Some(10)))]);
        ledger.hold(key_of(ANN));

        feed.push(line("m2", 20, ANN));
        ledger.absorb(&feed);
        refresh(&mut ledger, &feed, vec![(ANN, tally(6, Some(20)))]);

        assert_eq!(
            (ledger.count(&key_of(ANN)), unsaved_ids(&ledger, ANN)),
            (6, vec!["m2".to_owned()])
        );
    }

    #[test]
    fn releasing_the_hold_lets_the_next_refresh_move_the_count() {
        let feed = feed_of(vec![line("m1", 10, ANN)]);
        let mut ledger = absorbed(&feed);
        refresh(&mut ledger, &feed, vec![(ANN, tally(5, Some(10)))]);
        ledger.hold(key_of(ANN));
        refresh(&mut ledger, &feed, vec![(ANN, tally(9, Some(10)))]);

        ledger.release();
        refresh(&mut ledger, &feed, vec![(ANN, tally(9, Some(10)))]);

        assert_eq!(ledger.count(&key_of(ANN)), 9);
    }

    #[test]
    fn adopting_a_page_tally_while_held_recounts_only_feed_lines_newer_than_it() {
        let feed = feed_of(vec![
            line("m1", 10, ANN),
            line("m2", 20, ANN),
            line("m3", 30, ANN),
        ]);
        let mut ledger = absorbed(&feed);
        ledger.hold(key_of(ANN));

        ledger.adopt(&key_of(ANN), tally(5, Some(20)), &feed);

        assert_eq!(
            (ledger.count(&key_of(ANN)), unsaved_ids(&ledger, ANN)),
            (6, vec!["m3".to_owned()])
        );
    }

    #[test]
    fn adopting_one_viewers_page_leaves_other_viewers_counts_alone() {
        let feed = feed_of(vec![line("m1", 10, ANN), line("b1", 10, BOB)]);
        let mut ledger = absorbed(&feed);
        refresh(&mut ledger, &feed, vec![(BOB, tally(4, Some(10)))]);

        ledger.adopt(&key_of(ANN), tally(5, Some(10)), &feed);

        assert_eq!(ledger.count(&key_of(BOB)), 4);
    }

    #[test]
    fn a_held_viewer_keeps_the_adopted_page_tally_through_a_refresh() {
        let feed = feed_of(vec![line("m1", 10, ANN)]);
        let mut ledger = absorbed(&feed);
        ledger.hold(key_of(ANN));
        ledger.adopt(&key_of(ANN), tally(130, Some(10)), &feed);

        refresh(&mut ledger, &feed, vec![(ANN, tally(100, Some(10)))]);

        assert_eq!(
            ledger.stored_tally(&key_of(ANN)),
            Some(tally(130, Some(10)))
        );
    }
}
