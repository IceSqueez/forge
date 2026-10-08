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
