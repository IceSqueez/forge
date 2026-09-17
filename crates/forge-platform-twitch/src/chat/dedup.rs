use std::collections::{HashSet, VecDeque};

const WINDOW: usize = 512;

#[derive(Default)]
pub(super) struct MessageIdWindow {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl MessageIdWindow {
    pub(super) fn is_duplicate(&mut self, message_id: &str) -> bool {
        if message_id.is_empty() {
            return false;
        }
        if !self.seen.insert(message_id.to_owned()) {
            return true;
        }
        self.order.push_back(message_id.to_owned());
        if self.order.len() > WINDOW
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageIdWindow, WINDOW};

    #[test]
    fn an_id_is_a_duplicate_only_after_that_exact_id_was_seen() {
        let mut window = MessageIdWindow::default();

        assert!(
            !window.is_duplicate("msg-1"),
            "a first sighting is original"
        );
        assert!(window.is_duplicate("msg-1"), "the resend is a duplicate");
        assert!(
            !window.is_duplicate("msg-2"),
            "a different id must not be swept up by its neighbour"
        );
    }

    #[test]
    fn an_absent_message_id_is_never_a_duplicate() {
        // Why: `metadata.message_id` deserializes to the empty string when Twitch omits it, and
        // deduping on that would drop every id-less notification after the first.
        let mut window = MessageIdWindow::default();

        assert!(!window.is_duplicate(""));
        assert!(!window.is_duplicate(""));
    }

    #[test]
    fn the_oldest_id_is_forgotten_only_once_the_window_overflows() {
        let mut window = MessageIdWindow::default();
        for n in 0..WINDOW {
            assert!(
                !window.is_duplicate(&format!("msg-{n}")),
                "fixture ids must be distinct"
            );
        }

        assert!(
            window.is_duplicate("msg-0"),
            "a window filled exactly to capacity still remembers its oldest id"
        );

        assert!(!window.is_duplicate(&format!("msg-{WINDOW}")));
        assert!(
            !window.is_duplicate("msg-0"),
            "the id one past capacity must evict the oldest, not a newer one"
        );
        assert!(
            window.is_duplicate(&format!("msg-{}", WINDOW - 1)),
            "eviction must take the oldest id only"
        );
    }
}
