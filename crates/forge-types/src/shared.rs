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

    /// The read guard is released before returning, so the caller may hold the snapshot
    /// across an `.await` without keeping the lock.
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
