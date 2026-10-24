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
        if self.capacity == Some(0) {
            return true;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(capacity: usize, ids: &[&str]) -> DedupSet {
        let mut set = DedupSet::bounded(capacity);
        for id in ids {
            set.try_insert(*id);
        }
        set
    }

    #[test]
    fn first_sighting_of_an_id_is_accepted() {
        let mut set = DedupSet::bounded(4);
        assert!(set.try_insert("a"));
    }

    #[test]
    fn repeated_id_is_rejected_as_duplicate() {
        let mut set = filled(4, &["a"]);
        assert!(!set.try_insert("a"));
    }

    #[test]
    fn filling_to_capacity_evicts_nothing() {
        let set = filled(3, &["a", "b", "c"]);
        assert!(["a", "b", "c"].iter().all(|id| set.contains(id)));
    }

    #[test]
    fn inserting_one_over_capacity_evicts_only_the_oldest() {
        let set = filled(3, &["a", "b", "c", "d"]);
        assert!(!set.contains("a"));
        assert!(["b", "c", "d"].iter().all(|id| set.contains(id)));
    }

    #[test]
    fn evicted_id_is_accepted_again() {
        let mut set = filled(2, &["a", "b", "c"]);
        assert!(set.try_insert("a"));
    }

    #[test]
    fn rejected_duplicate_does_not_refresh_its_eviction_order() {
        let mut set = filled(3, &["a", "b", "c"]);
        set.try_insert("a");
        set.try_insert("d");
        assert!(!set.contains("a"));
        assert!(set.contains("b"));
    }

    #[test]
    fn zero_capacity_set_accepts_every_insert_and_stores_nothing() {
        let mut set = DedupSet::bounded(0);
        assert!(set.try_insert("a"));
        assert!(set.try_insert("a"));
        assert!(!set.contains("a"));
    }

    #[test]
    fn capacity_one_set_keeps_only_the_latest_id() {
        let mut set = filled(1, &["a", "b"]);
        assert!(!set.contains("a"));
        assert!(!set.try_insert("b"));
    }

    #[test]
    fn unbounded_set_never_evicts() {
        let mut set = DedupSet::unbounded();
        for n in 0..10_000 {
            set.try_insert(n.to_string());
        }
        assert!(set.contains("0"));
    }

    #[test]
    fn retain_present_forgets_ids_missing_from_the_current_view() {
        let mut set = filled(4, &["a", "b", "c"]);
        set.retain_present(["b", "z"]);
        assert!(!set.contains("a"));
        assert!(!set.contains("c"));
        assert!(set.contains("b"));
    }

    #[test]
    fn retain_present_does_not_adopt_unseen_ids() {
        let mut set = filled(4, &["a"]);
        set.retain_present(["a", "z"]);
        assert!(set.try_insert("z"));
    }

    #[test]
    fn retain_present_frees_capacity_for_later_inserts() {
        let mut set = filled(3, &["a", "b", "c"]);
        set.retain_present(["b", "c"]);
        set.try_insert("d");
        assert!(["b", "c", "d"].iter().all(|id| set.contains(id)));
    }

    #[test]
    fn eviction_after_retain_still_drops_the_oldest_survivor() {
        let mut set = filled(3, &["a", "b", "c"]);
        set.retain_present(["b", "c"]);
        set.try_insert("d");
        set.try_insert("e");
        assert!(!set.contains("b"));
        assert!(["c", "d", "e"].iter().all(|id| set.contains(id)));
    }
}
