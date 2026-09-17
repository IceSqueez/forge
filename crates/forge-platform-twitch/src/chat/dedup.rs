use std::collections::{HashSet, VecDeque};

const WINDOW: usize = 512;

#[derive(Default)]
pub(super) struct MessageIdWindow {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

impl MessageIdWindow {
    /// Reports and records in one step; an empty id is never a duplicate.
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
