use std::collections::{HashSet, VecDeque};

pub struct DedupSet {
    order: VecDeque<String>,
    seen: HashSet<String>,
    capacity: Option<usize>,
}

impl DedupSet {
    pub fn bounded(capacity: usize) -> Self {
        Self {
            order: VecDeque::with_capacity(capacity + 1),
            seen: HashSet::with_capacity(capacity),
            capacity: Some(capacity),
        }
    }

    pub fn unbounded() -> Self {
        Self {
            order: VecDeque::new(),
            seen: HashSet::new(),
            capacity: None,
        }
    }

    pub fn try_insert(&mut self, id: impl Into<String>) -> bool {
        let id = id.into();
        if self.seen.contains(&id) {
            return false;
        }
        if let Some(capacity) = self.capacity
            && self.order.len() >= capacity
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        self.seen.insert(id.clone());
        self.order.push_back(id);
        true
    }

    pub fn contains(&self, id: &str) -> bool {
        self.seen.contains(id)
    }

    pub fn retain_present<'a>(&mut self, current: impl IntoIterator<Item = &'a str>) {
        let current: HashSet<&str> = current.into_iter().collect();
        self.seen.retain(|id| current.contains(id.as_str()));
        self.order.retain(|id| current.contains(id.as_str()));
    }
}
