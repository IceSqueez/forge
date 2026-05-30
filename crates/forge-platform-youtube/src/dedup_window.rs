use std::collections::{HashSet, VecDeque};

pub(crate) const DEDUP_WINDOW_SIZE: usize = 500;

pub(crate) struct DedupWindow {
    window: VecDeque<String>,
    seen: HashSet<String>,
    capacity: usize,
}

impl DedupWindow {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity + 1),
            seen: HashSet::with_capacity(capacity),
            capacity,
        }
    }

    /// Returns `true` when `id` was new and has been recorded. Returns `false` for duplicates.
    pub(crate) fn try_insert(&mut self, id: String) -> bool {
        if self.seen.contains(&id) {
            return false;
        }
        if self.window.len() >= self.capacity
            && let Some(evicted) = self.window.pop_front()
        {
            self.seen.remove(&evicted);
        }
        self.seen.insert(id.clone());
        self.window.push_back(id);
        true
    }
}
