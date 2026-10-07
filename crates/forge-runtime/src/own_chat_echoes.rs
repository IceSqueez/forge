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
    uncounted_rows: VecDeque<Seen>,
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
        if take_match(&mut ledger.uncounted_rows, &fingerprint).is_some() {
            return None;
        }
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

    pub(crate) fn note_uncounted_row(&mut self, row: &UnifiedChatRow, now: Instant) {
        let fingerprint = row_fingerprint(row);
        if fingerprint.is_empty() {
            return;
        }
        let ledger = self.ledger(row.source, now);
        if take_match(&mut ledger.sent_awaiting_echo, &fingerprint).is_some() {
            return;
        }
        push_bounded(
            &mut ledger.uncounted_rows,
            Seen {
                at: now,
                fingerprint,
            },
        );
    }

    fn ledger(&mut self, source: ChatSource, now: Instant) -> &mut SourceLedger {
        let ledger = self.ledgers.entry(source).or_default();
        let fresh = |seen: &Seen| now.saturating_duration_since(seen.at) <= ECHO_WINDOW;
        ledger.sent_awaiting_echo.retain(fresh);
        ledger.counted_rows.retain(fresh);
        ledger.uncounted_rows.retain(fresh);
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
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn row_fingerprint(row: &UnifiedChatRow) -> String {
    let mut text = String::new();
    for segment in &row.body_segments {
        match segment {
            ChatSegment::Text { text: part } => text.push_str(part),
            ChatSegment::Emote { name, .. } => text.push_str(name),
            ChatSegment::Mention { username } => {
                text.push('@');
                text.push_str(username);
            }
        }
    }
    fingerprint_of(&text)
}

#[cfg(test)]
mod tests {
    use forge_types::{EventId, ModerationMarks};
    use time::OffsetDateTime;

    use super::*;

    const SECOND: Duration = Duration::from_secs(1);
    const MILLI: Duration = Duration::from_millis(1);

    fn row_on(source: ChatSource, segments: Vec<ChatSegment>) -> UnifiedChatRow {
        UnifiedChatRow {
            id: "row".to_owned(),
            event_id: EventId::new(),
            source,
            received_at: OffsetDateTime::UNIX_EPOCH,
            author: "forge_helper".to_owned(),
            author_id: None,
            author_color: None,
            body_segments: segments,
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn text(part: &str) -> ChatSegment {
        ChatSegment::Text {
            text: part.to_owned(),
        }
    }

    fn twitch_row(body: &str) -> UnifiedChatRow {
        row_on(ChatSource::Twitch, vec![text(body)])
    }

    #[test]
    fn an_echo_after_its_send_is_recognised_exactly_once() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        echoes.record_sent(ChatSource::Twitch, "follow the channel", t0);

        let echo = echoes.is_own_echo(&twitch_row("follow the channel"), t0 + SECOND);
        let copy = echoes.is_own_echo(&twitch_row("follow the channel"), t0 + 2 * SECOND);

        assert_eq!((echo, copy), (true, false));
    }

    #[test]
    fn a_send_after_its_echo_reports_when_the_echo_was_counted() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        echoes.is_own_echo(&twitch_row("follow the channel"), t0);

        let counted_at = echoes.record_sent(ChatSource::Twitch, "follow the channel", t0 + SECOND);

        assert_eq!(counted_at, Some(t0));
    }

    #[test]
    fn a_counted_row_is_retracted_by_one_send_only() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        echoes.is_own_echo(&twitch_row("gg"), t0);
        echoes.record_sent(ChatSource::Twitch, "gg", t0);

        assert_eq!(echoes.record_sent(ChatSource::Twitch, "gg", t0), None);
    }

    #[test]
    fn matching_ignores_whitespace_and_reads_emotes_mentions_and_links_as_typed() {
        let emote = ChatSegment::Emote {
            id: "25".to_owned(),
            name: "Kappa".to_owned(),
        };
        let mention = ChatSegment::Mention {
            username: "alice".to_owned(),
        };
        let bare_link = ChatSegment::Link {
            url: "https://forge.example".to_owned(),
            display: String::new(),
        };
        let shown_link = ChatSegment::Link {
            url: "https://forge.example".to_owned(),
            display: "forge.example".to_owned(),
        };
        for (sent, segments, expected) in [
            ("hello   world ", vec![text("hello world")], true),
            ("hi Kappa", vec![text("hi "), emote], true),
            ("thanks @alice", vec![text("thanks "), mention], true),
            (
                "see https://forge.example",
                vec![text("see "), bare_link],
                true,
            ),
            ("see forge.example", vec![text("see "), shown_link], true),
            ("hello", vec![text("hello!")], false),
        ] {
            let t0 = Instant::now();
            let mut echoes = OwnChatEchoes::default();
            echoes.record_sent(ChatSource::Twitch, sent, t0);

            assert_eq!(
                echoes.is_own_echo(&row_on(ChatSource::Twitch, segments), t0),
                expected,
                "sent {sent:?}"
            );
        }
    }

    #[test]
    fn a_send_matches_only_rows_from_its_own_platform() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        echoes.record_sent(ChatSource::Kick, "gg", t0);

        assert!(!echoes.is_own_echo(&twitch_row("gg"), t0));
    }

    #[test]
    fn a_blank_send_never_hides_a_row() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        echoes.record_sent(ChatSource::Twitch, " \n ", t0);

        assert!(!echoes.is_own_echo(&row_on(ChatSource::Twitch, vec![]), t0));
    }

    #[test]
    fn an_unmatched_send_awaits_its_echo_for_sixty_seconds_then_expires() {
        for (elapsed, expected) in [(60 * SECOND, true), (60 * SECOND + MILLI, false)] {
            let t0 = Instant::now();
            let mut echoes = OwnChatEchoes::default();
            echoes.record_sent(ChatSource::Twitch, "gg", t0);

            assert_eq!(
                echoes.is_own_echo(&twitch_row("gg"), t0 + elapsed),
                expected,
                "after {elapsed:?}"
            );
        }
    }

    #[test]
    fn a_counted_row_stays_retractable_for_sixty_seconds_then_expires() {
        for (elapsed, expected) in [(60 * SECOND, true), (60 * SECOND + MILLI, false)] {
            let t0 = Instant::now();
            let mut echoes = OwnChatEchoes::default();
            echoes.is_own_echo(&twitch_row("gg"), t0);

            assert_eq!(
                echoes
                    .record_sent(ChatSource::Twitch, "gg", t0 + elapsed)
                    .is_some(),
                expected,
                "after {elapsed:?}"
            );
        }
    }

    #[test]
    fn only_the_latest_sixty_four_sends_per_platform_await_an_echo() {
        let t0 = Instant::now();
        let mut echoes = OwnChatEchoes::default();
        for n in 0..=TRACKED_MESSAGES_PER_SOURCE {
            echoes.record_sent(ChatSource::Twitch, &format!("reminder {n}"), t0);
        }

        let oldest = echoes.is_own_echo(&twitch_row("reminder 0"), t0);
        let second_oldest = echoes.is_own_echo(&twitch_row("reminder 1"), t0);

        assert_eq!((oldest, second_oldest), (false, true));
    }
}
