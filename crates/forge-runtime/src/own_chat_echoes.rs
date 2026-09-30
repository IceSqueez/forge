use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use forge_types::{ChatSegment, ChatSource, UnifiedChatRow};
use tokio::time::Instant;

pub(crate) const CHAT_SENT_KIND: &str = "chat.send";

const ECHO_WINDOW: Duration = Duration::from_secs(60);
const TRACKED_MESSAGES_PER_SOURCE: usize = 64;

struct Seen {
    at: Instant,
    fingerprint: String,
}

#[derive(Default)]
struct SourceLedger {
    sent_awaiting_echo: VecDeque<Seen>,
    counted_rows: VecDeque<Seen>,
}

#[derive(Default)]
pub(crate) struct OwnChatEchoes {
    ledgers: HashMap<ChatSource, SourceLedger>,
}

impl OwnChatEchoes {
    pub(crate) fn record_sent(
        &mut self,
        source: ChatSource,
        message: &str,
        now: Instant,
    ) -> Option<Instant> {
        let fingerprint = fingerprint_of(message);
        if fingerprint.is_empty() {
            return None;
        }
        let ledger = self.ledger(source, now);
        if let Some(counted_at) = take_match(&mut ledger.counted_rows, &fingerprint) {
            return Some(counted_at);
        }
        push_bounded(
            &mut ledger.sent_awaiting_echo,
            Seen {
                at: now,
                fingerprint,
            },
        );
        None
    }

    pub(crate) fn is_own_echo(&mut self, row: &UnifiedChatRow, now: Instant) -> bool {
        let fingerprint = row_fingerprint(row);
        let ledger = self.ledger(row.source, now);
        if take_match(&mut ledger.sent_awaiting_echo, &fingerprint).is_some() {
            return true;
        }
        push_bounded(
            &mut ledger.counted_rows,
            Seen {
                at: now,
                fingerprint,
            },
        );
        false
    }

    fn ledger(&mut self, source: ChatSource, now: Instant) -> &mut SourceLedger {
        let ledger = self.ledgers.entry(source).or_default();
        let fresh = |seen: &Seen| now.saturating_duration_since(seen.at) <= ECHO_WINDOW;
        ledger.sent_awaiting_echo.retain(fresh);
        ledger.counted_rows.retain(fresh);
        ledger
    }
}

fn take_match(seen: &mut VecDeque<Seen>, fingerprint: &str) -> Option<Instant> {
    let position = seen
        .iter()
        .position(|entry| entry.fingerprint == fingerprint)?;
    seen.remove(position).map(|entry| entry.at)
}

fn push_bounded(seen: &mut VecDeque<Seen>, entry: Seen) {
    if seen.len() >= TRACKED_MESSAGES_PER_SOURCE {
        seen.pop_front();
    }
    seen.push_back(entry);
}

fn fingerprint_of(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn row_fingerprint(row: &UnifiedChatRow) -> String {
    let mut text = String::new();
    for segment in &row.body_segments {
        match segment {
            ChatSegment::Text { text: part } => text.push_str(part),
            ChatSegment::Emote { name, .. } => text.push_str(name),
            ChatSegment::Link { display, .. } if !display.is_empty() => text.push_str(display),
            ChatSegment::Link { url, .. } => text.push_str(url),
            ChatSegment::Mention { username } => {
                text.push('@');
                text.push_str(username);
            }
        }
    }
    fingerprint_of(&text)
}
