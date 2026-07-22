use std::sync::{Arc, PoisonError, RwLock};

pub struct Shared<T>(Arc<RwLock<Arc<T>>>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(RwLock::new(Arc::new(value))))
    }

    pub fn from_arc(value: Arc<T>) -> Self {
        Self(Arc::new(RwLock::new(value)))
    }

    /// The read guard is released before returning; the caller may hold the returned snapshot across an `.await`.
    pub fn load(&self) -> Arc<T> {
        Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn store(&self, value: T) {
        self.store_arc(Arc::new(value));
    }

    pub fn store_arc(&self, value: Arc<T>) {
        let mut guard = self.0.write().unwrap_or_else(PoisonError::into_inner);
        *guard = value;
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_snapshot_is_frozen_while_later_store_swaps_subsequent_loads() {
        let shared = Shared::new(vec![1, 2, 3]);
        let snapshot = shared.load();

        shared.store(vec![9, 9]);

        assert_eq!(
            *snapshot,
            vec![1, 2, 3],
            "old snapshot must not observe the store"
        );
        assert_eq!(
            *shared.load(),
            vec![9, 9],
            "a fresh load must observe the store"
        );
    }

    #[test]
    fn store_arc_keeps_the_callers_arc_identity() {
        let shared = Shared::new(0u32);
        let arc = Arc::new(42u32);

        shared.store_arc(Arc::clone(&arc));

        assert!(
            Arc::ptr_eq(&shared.load(), &arc),
            "store_arc must publish the exact Arc, not wrap the value in a new allocation",
        );
    }

    #[test]
    fn clones_share_one_cell_so_a_store_through_one_is_seen_by_the_other() {
        let a = Shared::new(1u32);
        let b = a.clone();
        let published = Arc::new(7u32);

        a.store_arc(Arc::clone(&published));

        assert!(
            Arc::ptr_eq(&b.load(), &published),
            "a clone must alias the same cell, not a detached copy",
        );
    }
}
